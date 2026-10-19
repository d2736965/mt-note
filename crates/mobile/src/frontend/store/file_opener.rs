//! 文件打开 Actor。
//!
//! 统一处理"打开文件"的并发请求：串行判断文件类型，
//! 返回结果由调用方（Dioxus 组件，有 navigator 上下文）执行导航。
//! 不支持的文件在 Actor 内直接调 JNI 弹系统选择器（JNI 不需 Dioxus 上下文）。
//! 通过 kameo Actor 串行化消息处理，避免快速连点导致路由栈竞争。

use std::path::PathBuf;

use dioxus::logger::tracing;
use dioxus::prelude::*;
use kameo::actor::ActorRef;
use kameo::message::Message;
use kameo::prelude::*;
use mt_note_fs_engine::{FileCategory, is_text_file};

use crate::frontend::js_bridge::{DioxusBridge, js_api};
use crate::frontend::router::Route;
use crate::frontend::store::storage::jni::open_with_external_app;

/// 文件打开 Actor：串行处理打开请求，避免路由栈竞争。
#[derive(Actor)]
#[actor(name = "file_opener")]
pub struct FileOpenerActor;

/// 打开文件的结果。
#[derive(Clone, Debug, PartialEq, kameo::Reply)]
pub enum OpenResult {
    /// 文件支持编辑/预览，应导航到 Editor（路径返回供调用方 nav.push）。
    Editor(PathBuf),
    /// 不支持，已弹出系统选择器。调用方无需动作。
    External,
}

/// 打开文件消息：Actor 串行判断后返回结果。
///
/// 跨文件调用，需 `pub`。手动定义结构体 + impl Message
/// （`#[messages]` 宏无法控制生成结构体的可见性）。
#[derive(Clone, Debug)]
pub struct Open {
    pub path: PathBuf,
}

impl Message<Open> for FileOpenerActor {
    type Reply = OpenResult;

    async fn handle(
        &mut self,
        msg: Open,
        _ctx: &mut kameo::message::Context<Self, Self::Reply>,
    ) -> Self::Reply {
        let path = msg.path;
        let category = FileCategory::from_path(&path);
        if category.is_previewable() {
            return OpenResult::Editor(path);
        }
        match is_text_file(&path).await {
            Ok(true) => OpenResult::Editor(path),
            _ => {
                // 不支持或读取失败，弹系统选择器（JNI 不需 Dioxus 上下文）
                if let Err(e) = open_with_external_app(&path.to_string_lossy()) {
                    tracing::error!("open_with_external_app({}): {e}", path.display());
                }
                OpenResult::External
            }
        }
    }
}

/// 在 App 层提供 FileOpenerActor 的引用容器。
///
/// Actor 在注册时立即 spawn，生命周期独立于任何组件：跨路由不卸载，全局单例。
/// 必须在 App（layout 组件，跨路由不卸载）中调用。
pub fn provide_file_opener() -> Signal<Option<ActorRef<FileOpenerActor>>> {
    let actor = FileOpenerActor::spawn(FileOpenerActor);
    let signal: Signal<Option<ActorRef<FileOpenerActor>>> = Signal::new(Some(actor));
    use_context_provider(|| signal)
}

/// 请求 Actor 打开文件，返回结果。调用方根据结果执行 nav.push 或忽略。
///
/// `actor_ctx` 必须在组件渲染体通过 `use_context` 获取后传入。
pub async fn ask_open(
    actor_ctx: &Signal<Option<ActorRef<FileOpenerActor>>>,
    path: PathBuf,
) -> Option<OpenResult> {
    let actor = match actor_ctx.read().clone() {
        Some(a) => a,
        None => {
            tracing::warn!("FileOpenerActor not present in context");
            return Some(OpenResult::External);
        }
    };
    match actor.ask(Open { path }).await {
        Ok(r) => Some(r),
        Err(e) => {
            tracing::error!("FileOpenerActor ask failed: {e:?}");
            None
        }
    }
}

/// 将路径推入 Editor 路由。供调用方在 match OpenResult::Editor 后调用。
pub fn push_editor(path: &std::path::Path) {
    // 导航离开前主动保存当前首页滚动位置，避免组件卸载时 scroll 事件覆盖。
    js_api::scroll::save_home_scroll::<DioxusBridge>();
    navigator().push(Route::Editor {
        path: urlencoding::encode(&path.to_string_lossy()).to_string(),
    });
}
