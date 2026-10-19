//! 文件系统监听模块。
//!
//! 基于 kameo actor + notify crate，响应 inotify 事件。
//! 组件通过 `subscribe()` 获取 `watch::Receiver<FsEvent>` 订阅事件。

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use dioxus::logger::tracing;
use kameo::actor::ActorRef;
use kameo::prelude::*;
use notify::{Event, EventKind, RecursiveMode, Watcher};
use tokio::sync::watch;

// ── 对外类型 ──────────────────────────────────────────────────

/// 文件系统事件
#[derive(Clone, Debug)]
pub struct FsEvent {
    pub path: PathBuf,
    pub kind: FsEventKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FsEventKind {
    /// 文件内容修改
    FileModified,
    /// 目录内容变化（新增/删除/重命名）
    DirChanged,
}

// ── Actor 注册名 ──────────────────────────────────────────────

const ACTOR_NAME: &str = "file_watcher";

// ── Actor 定义 ────────────────────────────────────────────────

pub struct FileWatcherActor {
    watcher: Option<notify::RecommendedWatcher>,
    last_broadcast: HashMap<(PathBuf, FsEventKind), Instant>,
    ignored: HashMap<PathBuf, Instant>,
    event_tx: watch::Sender<FsEvent>,
    debounce: Duration,
}

impl Actor for FileWatcherActor {
    type Args = Duration;
    type Error = notify::Error;

    async fn on_start(
        debounce: Self::Args,
        actor_ref: ActorRef<Self>,
    ) -> Result<Self, Self::Error> {
        tracing::info!("FileWatcherActor::on_start: debounce={:?}", debounce);
        let (event_tx, _) = watch::channel(FsEvent {
            path: PathBuf::new(),
            kind: FsEventKind::DirChanged,
        });

        let actor_ref_clone = actor_ref.clone();

        let watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                let fs_event = Self::convert_event(&event);
                let msg = OnFsEvent { event: fs_event };
                if let Err(e) = actor_ref_clone.tell(msg).blocking_send() {
                    tracing::error!("blocking_send failed: {e}");
                }
            }
        })?;

        tracing::info!("FileWatcherActor::on_start: watcher created successfully");
        Ok(Self {
            watcher: Some(watcher),
            last_broadcast: HashMap::new(),
            ignored: HashMap::new(),
            event_tx,
            debounce,
        })
    }

    async fn on_stop(
        &mut self,
        _actor_ref: WeakActorRef<Self>,
        _reason: ActorStopReason,
    ) -> Result<(), Self::Error> {
        self.watcher.take();
        Ok(())
    }
}

impl FileWatcherActor {
    fn convert_event(event: &Event) -> FsEvent {
        let path = event.paths.first().cloned().unwrap_or_default();
        let kind = match event.kind {
            EventKind::Modify(_) => FsEventKind::FileModified,
            EventKind::Create(_) | EventKind::Remove(_) => FsEventKind::DirChanged,
            _ => FsEventKind::DirChanged,
        };
        FsEvent { path, kind }
    }
}

// ── 消息定义 ──────────────────────────────────────────────────

#[messages]
impl FileWatcherActor {
    #[message]
    fn watch_dir(&mut self, path: PathBuf) {
        tracing::info!("FileWatcherActor::watch_dir: {}", path.display());
        if let Some(ref mut watcher) = self.watcher
            && let Err(e) = watcher.watch(&path, RecursiveMode::NonRecursive)
        {
            tracing::error!("watch_dir({}): {e}", path.display());
        }
    }

    #[message]
    fn unwatch_dir(&mut self, path: PathBuf) {
        if let Some(ref mut watcher) = self.watcher
            && let Err(e) = watcher.unwatch(&path)
        {
            tracing::error!("unwatch_dir({}): {e}", path.display());
        }
    }

    #[message]
    fn watch_file(&mut self, path: PathBuf) {
        if let Some(ref mut watcher) = self.watcher
            && let Err(e) = watcher.watch(&path, RecursiveMode::NonRecursive)
        {
            tracing::error!("watch_file({}): {e}", path.display());
        }
    }

    #[message]
    fn unwatch_file(&mut self, path: PathBuf) {
        if let Some(ref mut watcher) = self.watcher
            && let Err(e) = watcher.unwatch(&path)
        {
            tracing::error!("unwatch_file({}): {e}", path.display());
        }
    }

    #[message]
    fn ignore_file(&mut self, path: PathBuf, duration_ms: u64) {
        let until = Instant::now() + Duration::from_millis(duration_ms);
        self.ignored.insert(path, until);
    }

    #[message(derive(Clone))]
    fn subscribe(&self) -> watch::Receiver<FsEvent> {
        self.event_tx.subscribe()
    }

    #[message(derive(Clone))]
    async fn on_fs_event(&mut self, event: FsEvent) {
        tracing::debug!("on_fs_event: {:?} {}", event.kind, event.path.display());
        let now = Instant::now();

        let ignored = self
            .ignored
            .get(&event.path)
            .copied()
            .filter(|&until| now < until);
        if ignored.is_some() {
            // 仍在忽略期内，丢弃事件
            tracing::debug!("ignored event for {}", event.path.display());
        } else {
            self.ignored.remove(&event.path);

            let broadcast_key = (event.path.clone(), event.kind.clone());
            let should_broadcast = !matches!(
                self.last_broadcast.get(&broadcast_key),
                Some(&last) if now.duration_since(last) < self.debounce,
            );

            if should_broadcast {
                self.last_broadcast.insert(broadcast_key, now);
                if let Err(e) = self.event_tx.send(event) {
                    tracing::error!("event_tx.send failed: {e}");
                }
            }
        }
    }
}

// ── 对外 API ──────────────────────────────────────────────────

/// 初始化文件监听器（全局单例，幂等）。
///
/// 在 App 组件中调用一次。
pub async fn init_file_watcher() {
    tracing::info!("init_file_watcher: starting...");
    let already_registered = ActorRef::<FileWatcherActor>::lookup(ACTOR_NAME)
        .ok()
        .flatten()
        .is_some();
    if !already_registered {
        tracing::info!("init_file_watcher: spawning actor...");
        let actor_ref = FileWatcherActor::spawn(Duration::from_millis(500));
        if let Err(e) = actor_ref.register(ACTOR_NAME) {
            tracing::error!("init_file_watcher: register failed: {e}");
        } else {
            tracing::info!("init_file_watcher: actor registered successfully");
        }
    } else {
        tracing::info!("init_file_watcher: already registered");
    }
}

fn lookup() -> Option<ActorRef<FileWatcherActor>> {
    ActorRef::<FileWatcherActor>::lookup(ACTOR_NAME)
        .ok()
        .flatten()
}

pub async fn watch_dir(path: &std::path::Path) {
    tracing::info!("watch_dir: {}", path.display());
    if let Some(actor_ref) = lookup()
        && let Err(e) = actor_ref
            .tell(WatchDir {
                path: path.to_path_buf(),
            })
            .await
    {
        tracing::error!("watch_dir tell failed: {e}");
    }
}

#[expect(dead_code, reason = "公共 API，暂未使用")]
pub async fn unwatch_dir(path: &std::path::Path) {
    if let Some(actor_ref) = lookup()
        && let Err(e) = actor_ref
            .tell(UnwatchDir {
                path: path.to_path_buf(),
            })
            .await
    {
        tracing::error!("unwatch_dir tell failed: {e}");
    }
}

pub async fn watch_file(path: &std::path::Path) {
    tracing::info!("watch_file: {}", path.display());
    if let Some(actor_ref) = lookup()
        && let Err(e) = actor_ref
            .tell(WatchFile {
                path: path.to_path_buf(),
            })
            .await
    {
        tracing::error!("watch_file tell failed: {e}");
    }
}

#[expect(dead_code, reason = "公共 API，暂未使用")]
pub async fn unwatch_file(path: &std::path::Path) {
    if let Some(actor_ref) = lookup()
        && let Err(e) = actor_ref
            .tell(UnwatchFile {
                path: path.to_path_buf(),
            })
            .await
    {
        tracing::error!("unwatch_file tell failed: {e}");
    }
}

#[expect(dead_code, reason = "公共 API，保留异步版本备用")]
pub async fn ignore_file(path: &std::path::Path, duration_ms: u64) {
    if let Some(actor_ref) = lookup()
        && let Err(e) = actor_ref
            .tell(IgnoreFile {
                path: path.to_path_buf(),
                duration_ms,
            })
            .await
    {
        tracing::error!("ignore_file tell failed: {e}");
    }
}

pub fn ignore_file_blocking(path: &std::path::Path, duration_ms: u64) {
    if let Some(actor_ref) = lookup()
        && let Err(e) = actor_ref
            .tell(IgnoreFile {
                path: path.to_path_buf(),
                duration_ms,
            })
            .blocking_send()
    {
        tracing::error!("ignore_file blocking_send failed: {e}");
    }
}

pub async fn subscribe() -> Option<watch::Receiver<FsEvent>> {
    let rx = lookup()?.ask(Subscribe).await.ok()?;
    tracing::info!("subscribe: got receiver");
    Some(rx)
}
