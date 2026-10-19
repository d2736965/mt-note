use std::time::Duration;

use dioxus::logger::tracing;
use dioxus::prelude::*;
use dioxus_i18n::prelude::i18n;
use ui::components::icons::{InfoIcon, StorageIcon};

use crate::frontend::i18n as i18n_support;
use crate::frontend::store::storage::jni;

stylance::import_style!(styles, "permission_gate.module.scss");

/// 权限检查间隔。仅在未授权时轮询，授权后立即停止，
/// 避免对 Dioxus/wry Android 运行时造成无谓的 cancel/restart 压力。
const POLL_INTERVAL: Duration = Duration::from_secs(2);

#[component]
pub fn PermissionGate(children: Element) -> Element {
    // i18n Context（渲染期取词自动订阅语言变化）
    let i18n_ctx = i18n();
    let mut check_result = use_resource(move || async move {
        // JNI 调用通过 spawn_blocking 放到独立线程执行，避免阻塞 Dioxus 运行时。
        tokio::task::spawn_blocking(|| match jni::is_storage_manager_granted() {
            Ok(v) => Ok(v),
            Err(e) => {
                tracing::error!("is_storage_manager_granted: {e:?}");
                Err(e.to_string())
            }
        })
        .await
        .map_err(|e| format!("spawn_blocking join error: {e:?}"))
        .and_then(|inner| inner)
    });

    // 条件轮询：仅在未授权（denied 或 error）时每 POLL_INTERVAL 重启资源 future，
    // 让用户从系统设置返回后能自动检测到授权。
    // None（检查中）和 Some(Ok(true))（已授权）时不轮询。
    // use_effect 只在依赖的 signal 变化时重跑，授权后这里不会再生效。
    use_effect(move || {
        let mut check_result = check_result;
        spawn(async move {
            loop {
                tokio::time::sleep(POLL_INTERVAL).await;
                // 读取当前结果决定是否继续轮询。
                // Ok(true) → 已授权，停止轮询。
                // 其它（None/Ok(false)/Err）→ 继续轮询。
                match check_result.cloned() {
                    Some(Ok(true)) => break,
                    _ => check_result.restart(),
                }
            }
        });
    });

    match check_result() {
        None => {
            rsx! {
                div { class: styles::gate_container,
                    div { class: styles::gate_content,
                        StorageIcon { class: styles::gate_icon }
                        span { class: styles::gate_checking, { i18n_support::translate(&i18n_ctx, "perm-checking") } }
                    }
                }
            }
        }
        Some(Ok(true)) => {
            rsx! {
                {children}
            }
        }
        Some(Ok(false)) | Some(Err(_)) => {
            rsx! {
                div { class: styles::gate_container,
                    div { class: styles::gate_content,
                        StorageIcon { class: styles::gate_icon }
                        h2 { class: styles::gate_title, { i18n_support::translate(&i18n_ctx, "perm-title") } }
                        p { class: styles::gate_desc,
                            { i18n_support::translate(&i18n_ctx, "perm-desc") }
                        }
                        div { class: styles::gate_hint,
                            InfoIcon { class: styles::gate_hint_icon }
                            span { { i18n_support::translate(&i18n_ctx, "perm-hint") } }
                        }
                        button {
                            class: styles::gate_button,
                            onclick: move |_| {
                                if let Err(e) = jni::open_all_files_access_settings() {
                                    tracing::error!("open_all_files_access_settings: {e:?}");
                                }
                            },
                            { i18n_support::translate(&i18n_ctx, "perm-open-settings") }
                        }
                        button {
                            class: styles::gate_retry,
                            onclick: move |_| {
                                check_result.restart();
                            },
                            { i18n_support::translate(&i18n_ctx, "perm-recheck") }
                        }
                    }
                }
            }
        }
    }
}
