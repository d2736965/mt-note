use std::path::PathBuf;

use dioxus::logger::tracing;
use dioxus::prelude::*;
use kameo::actor::ActorRef;
use manganis::asset;
use models::{BrowserPrefs, EditorPrefs, GeneralPrefs, OpenMode, StartupTab, Theme, TypstPrefs};
use ui::hooks::use_app_state::use_app_state;
use ui::store::app_state::AppState;

use crate::frontend::components::bottom_bar::BottomBar;
use crate::frontend::components::permission_gate::PermissionGate;
use crate::frontend::components::toast::{ToastHost, provide_toast_context};
use crate::frontend::components::top_app_bar::TopAppBar;
use crate::frontend::file_watcher;
use crate::frontend::hooks::use_back_interceptor::{provide_back_intercept_context, use_back_interceptor};
use crate::frontend::i18n as i18n_support;
use crate::frontend::hooks::use_bottom_bar_state::use_bottom_bar_state;
use crate::frontend::hooks::use_top_app_bar_state::use_top_app_bar_state;
use crate::frontend::js_bridge::{DioxusBridge, js_api};
use crate::frontend::router::Route;
use crate::frontend::store::bottom_bar_state::BottomBarState;
use crate::frontend::store::file_opener::{FileOpenerActor, provide_file_opener};
use crate::frontend::store::storage::android::{AndroidBackend, AndroidStorage};
use crate::frontend::store::storage::jni;
use crate::frontend::store::top_app_bar_state::TopAppBarState;
use crate::frontend::views::home::components::search_dialog::provide_search_actor;

stylance::import_style!(styles, "app.module.scss");

#[component]
pub fn App() -> Element {
    let mut state = AppState::new();
    if state.current_dir().as_os_str().is_empty() {
        state.set_current_dir(AppState::default_dir());
    }
    AppState::provide(state);

    let app_bar = TopAppBarState::new();
    TopAppBarState::provide(app_bar);

    let bottom_bar = BottomBarState::new();
    BottomBarState::provide(bottom_bar);

    // 渲染体禁用裸 Signal::new——App 每次重渲染都会执行到这里，裸 new 会
    // 生成全新 signal 把状态重置（灯箱返回键拦截因此从未生效：事件循环
    // set(true) 触发重渲染，重渲染 new 出新的 false signal）。必须 use_signal。
    let editor_error = use_signal(|| None::<String>);
    use_context_provider(|| editor_error);

    let back_intercept_requested = provide_back_intercept_context();
    let entered_via_share: Signal<bool> = use_signal(|| false);
    use_context_provider(|| entered_via_share);

    // 图形灯箱开关状态：由下方 'md-lightbox-change' 事件循环维护，
    // App 层的返回键拦截读取它
    let lightbox_open = use_signal(|| false);
    use_context_provider(|| lightbox_open);

    provide_toast_context();
    provide_search_actor();
    provide_file_opener();

    // i18n Context：初始中文，启动 effect 按「持久化偏好 → 系统语言」解析后切换。
    // 供全树组件经 dioxus_i18n 的 context 取词。
    let i18n_ctx = i18n_support::use_init_app_i18n();

    let mut state = match use_app_state() {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("AppState 未初始化: {e}");
            return rsx! { { i18n_support::translate(&i18n_ctx, "common-app-state-error") } };
        }
    };

    // 初始化 JS 模块实例（幂等，只创建一次）
    use_effect(move || {
        js_api::events::create::<DioxusBridge>();
        js_api::back_interceptor::create::<DioxusBridge>();
        js_api::editor::create::<DioxusBridge>();
        js_api::markdown::create::<DioxusBridge>();
        js_api::pinch_zoom::create::<DioxusBridge>();
        js_api::svg_preview::create::<DioxusBridge>();
        js_api::page_indicator::create::<DioxusBridge>();
        js_api::pdf_preview::create::<DioxusBridge>();
        js_api::scroll::create::<DioxusBridge>();
        js_api::i18n::create::<DioxusBridge>();
        spawn(async {
            file_watcher::init_file_watcher().await;
        });
    });
    // 系统栏（状态栏+导航栏）颜色同步——独立 effect，只依赖 state。
    // 不与 storage 耦合，避免 backend 失败时丢失 state 订阅。
    use_effect(move || {
        let is_dark = state.read().theme().is_dark();
        // 使用 surface-container 色，与 TopAppBar / BottomAppBar 背景色一致，
        // 确保系统栏透明区域与相邻控件无缝衔接
        let surface_color = if is_dark { "#211F26" } else { "#F3EDF7" };
        if let Err(e) = jni::set_system_bar_theme(is_dark, surface_color) {
            tracing::error!("set_system_bar_theme: {e:?}");
        }
    });

    // 主题持久化：所有 JNI 调用集中在 use_effect 中，
    // 延迟到首帧渲染之后执行，避免 Wry 初始化期间的 JNI 冲突。
    let mut theme_loaded = use_signal(|| false);
    use_effect(move || {
        let mut i18n_ctx = i18n_ctx;
        if let Ok(backend) = AndroidBackend::new() {
            let storage = AndroidStorage::new(backend);
            if !theme_loaded() {
                if let Ok(Some(persisted)) = storage.load::<Theme>() {
                    state.write().set_theme(persisted);
                }
                if let Ok(Some(prefs)) = storage.load::<TypstPrefs>() {
                    state.write().set_typst_prefs(prefs);
                }
                if let Ok(Some(prefs)) = storage.load::<EditorPrefs>() {
                    state.write().set_editor_prefs(prefs);
                }
                if let Ok(Some(mode)) = storage.load::<OpenMode>() {
                    state.write().set_open_mode(mode);
                }
                if let Ok(Some(prefs)) = storage.load::<BrowserPrefs>() {
                    // 应用持久化的默认目录：仅当 current_dir 仍为初始默认值时覆盖，
                    // 避免覆盖用户在本次会话中已导航到的新目录。
                    if let Some(dir) = &prefs.default_dir
                        && !dir.is_empty()
                    {
                        state.write().set_current_dir(dir.into());
                    }
                    state.write().set_browser_prefs(prefs);
                }
                if let Ok(Some(prefs)) = storage.load::<GeneralPrefs>() {
                    // 启动页偏好仅在冷启动时应用；会话内切换 tab 不回写。
                    let tab = match prefs.startup_tab {
                        StartupTab::Files => 0,
                        StartupTab::Favorites => 1,
                    };
                    state.write().set_active_tab(tab);
                    state.write().set_general_prefs(prefs);
                }
                // 界面语言：显式选择优先，System 跟随系统（JNI 读取，进程内缓存），
                // 经协商匹配到已注册语言。
                let lang = i18n_support::resolve_language(&state.read().general_prefs().language);
                i18n_support::set_language(&mut i18n_ctx, &lang);
                // TS 侧词典同步（markdown 工具条/灯箱等 JS 动态文案）
                js_api::i18n::set_locale::<DioxusBridge>(&lang);
                theme_loaded.set(true);
            } else if let Err(e) = storage.save(state.read().theme()) {
                tracing::error!("persist theme: {e:?}");
            }
        } else {
            tracing::error!("android storage backend init failed");
        }
    });

    let route = use_route::<Route>();
    let mut prev_route = use_signal(|| route.clone());
    let mut should_show_back = use_signal(|| false);
    let app_bar_result = use_top_app_bar_state();
    let bottom_bar_result = use_bottom_bar_state();
    if *prev_route.read() != route {
        if let Ok(mut app_bar) = app_bar_result {
            app_bar.write().reset();
        }
        if let Ok(mut bottom_bar) = bottom_bar_result {
            bottom_bar.write().reset();
        }
        prev_route.set(route.clone());
    }
    {
        let show = app_bar_result
            .as_ref()
            .ok()
            .and_then(|ab| *ab.read().show_back())
            .unwrap_or(!matches!(route, Route::Home {}));
        should_show_back.set(show);
    }

    // Android 分享 Intent 事件驱动监听：
    // Kotlin 侧通过 WebView.evaluateJavascript 派发 CustomEvent 'shared-file'，
    // Rust 侧通过 Bridge.waitForEvent 等待该事件，零轮询。
    //
    // 冷启动流程：onCreate → handleIncomingIntent 设置 pendingSharedFile
    //   → onWebViewCreate 补发 notifySharedFileToWebView → JS 事件到达
    //   → Rust Promise resolve → 导航
    // 热启动流程：onNewIntent → handleIncomingIntent → notifySharedFileToWebView
    //   → JS 事件到达 → Rust Promise resolve → 导航
    //
    // JNI 一次性检查作为兜底：处理 onWebViewCreate 补发与 use_effect 首次
    // waitForEvent 之间的极短窗口（事件已在 JS 队列但 Promise 尚未创建）。
    let nav = navigator();

    use_effect(move || {
        let mut entered_via_share = entered_via_share;
        let actor_ctx = use_context::<Signal<Option<ActorRef<FileOpenerActor>>>>();
        spawn(async move {
            let jni_result = jni::get_and_clear_pending_shared_file();
            if let Ok(Some(path)) = jni_result
                && !path.is_empty()
            {
                tracing::info!("shared file from JNI: {path}");
                entered_via_share.set(true);
                open_via_actor(&actor_ctx, PathBuf::from(path)).await;
            }

            loop {
                let json = js_api::events::wait_for_event::<DioxusBridge>("shared-file").await;
                let parsed: serde_json::Value = match serde_json::from_str(&json) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::error!("shared-file: parse json failed: {e}");
                        continue;
                    }
                };
                let path = parsed["path"].as_str().unwrap_or("");
                if path.is_empty() {
                    tracing::debug!("shared-file event resolved with empty path");
                    continue;
                }
                tracing::info!("shared file from event: {path}");
                entered_via_share.set(true);
                open_via_actor(&actor_ctx, PathBuf::from(path)).await;
            }
        });
    });

    // Markdown 本地链接跳转事件驱动监听：
    // markdown.ts 拦截非 HTTP 链接点击，派发 CustomEvent 'link-click'，
    // Rust 侧解析相对路径（基于 data-md-base-path），导航到目标文件。
    // 放在 App 层（非 Markdown 组件）确保循环生命周期 = 应用生命周期，
    // 避免组件卸载/重渲染导致 spawn 循环失效。
    use_effect(move || {
        let actor_ctx = use_context::<Signal<Option<ActorRef<FileOpenerActor>>>>();
        spawn(async move {
            loop {
                let json = js_api::events::wait_for_event::<DioxusBridge>("link-click").await;
                let parsed: serde_json::Value = match serde_json::from_str(&json) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::error!("link-click: parse json failed: {e}");
                        continue;
                    }
                };
                let href = parsed["href"].as_str().unwrap_or("");
                let base = parsed["base"].as_str().unwrap_or("");
                if href.is_empty() {
                    continue;
                }
                tracing::info!("link-click: href={href}, base={base}");

                let resolved_path = if href.starts_with('/') {
                    PathBuf::from(href)
                } else {
                    let base_dir = if base.is_empty() {
                        PathBuf::new()
                    } else {
                        PathBuf::from(base)
                            .parent()
                            .map(PathBuf::from)
                            .unwrap_or_default()
                    };
                    base_dir.join(href)
                };

                if resolved_path.exists() {
                    open_via_actor(&actor_ctx, resolved_path).await;
                } else {
                    tracing::warn!(
                        "link-click: path does not exist: {}",
                        resolved_path.display()
                    );
                }
            }
        });
    });

    // 图形灯箱开关事件驱动监听：
    // image-lightbox.ts 打开/关闭浮层时派发 CustomEvent 'md-lightbox-change'，
    // Rust 侧同步 lightbox_open，供下方返回键拦截使用。
    // 放在 App 层与 link-click 循环同理：循环生命周期 = 应用生命周期。
    use_effect(move || {
        let mut lightbox_open = lightbox_open;
        spawn(async move {
            loop {
                let json =
                    js_api::events::wait_for_event::<DioxusBridge>("md-lightbox-change").await;
                let parsed: serde_json::Value = match serde_json::from_str(&json) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::error!("md-lightbox-change: parse json failed: {e}");
                        continue;
                    }
                };
                lightbox_open.set(parsed["open"].as_bool().unwrap_or(false));
            }
        });
    });

    // 灯箱打开时拦截 Android 返回键：关闭灯箱而非退出页面。放 App 层全局注册——
    // 灯箱可从 markdown 预览、Typst 预览等多处打开，原挂在 Markdown 组件上时
    // 其余视图开的灯箱返回键会穿透到下层（退出文件/返回目录）且浮层残留。
    use_back_interceptor(lightbox_open(), move || {
        js_api::markdown::close_lightbox::<DioxusBridge>();
    });

    // Android 返回键事件驱动监听：
    // Kotlin 侧 onBackPressed 拦截硬件返回键，派发 back-pressed CustomEvent，
    // Rust 侧收到事件后检查 __bi 拦截器栈：
    // - 有拦截器 → 触发对应浮层的关闭回调
    // - 无拦截器 → 检查 show_back 决定导航或回桌面
    use_effect(move || {
        let nav = nav;
        let back = should_show_back;
        let mut entered_via_share = entered_via_share;
        let mut back_intercept_requested = back_intercept_requested;
        spawn(async move {
            loop {
                js_api::events::wait_for_event::<DioxusBridge>("back-pressed").await;
                {
                    let val = js_api::back_interceptor::get_stack_top::<DioxusBridge>().await;
                    if let Some(id_str) = val.strip_prefix("i:") {
                        if let Ok(id) = id_str.parse::<usize>() {
                            back_intercept_requested.set(Some(id));
                        }
                    } else if *back.read() {
                        if entered_via_share() {
                            entered_via_share.set(false);
                            nav.go_back();
                            if let Err(e) = jni::move_task_to_back() {
                                tracing::error!("move_task_to_back: {e:?}");
                            }
                        } else {
                            nav.go_back();
                        }
                    } else if let Err(e) = jni::move_task_to_back() {
                        tracing::error!("move_task_to_back: {e:?}");
                    }
                }
            }
        });
    });

    let theme_class = match *state.read().theme() {
        Theme::Light => styles::theme_light,
        Theme::Dark => styles::theme_dark,
    };

    let has_bottom_bar = match use_bottom_bar_state() {
        Ok(b) => b.read().kind().is_some(),
        Err(e) => {
            tracing::error!("has_bottom_bar: {e}");
            false
        }
    };
    let page_class = if has_bottom_bar {
        stylance::classes!(styles::page, theme_class, styles::has_bottom_bar)
    } else {
        stylance::classes!(styles::page, theme_class)
    };

    rsx! {
        document::Stylesheet { href: ui::STYLES }
        document::Stylesheet { href: asset!("/assets/styles/dist/bundle-mobile.scss") }
        div { class: page_class,
            TopAppBar {}
            PermissionGate { Outlet::<Route> {} }
            BottomBar {}
            ToastHost {}
        }
    }
}

/// 通过 FileOpenerActor 打开文件。
///
/// `actor_ctx` 必须在组件渲染体通过 `use_context` 获取后传入。
async fn open_via_actor(actor_ctx: &Signal<Option<ActorRef<FileOpenerActor>>>, path: PathBuf) {
    use crate::frontend::store::file_opener::{OpenResult, ask_open, push_editor};
    if let Some(OpenResult::Editor(p)) = ask_open(actor_ctx, path).await {
        push_editor(&p);
    }
}