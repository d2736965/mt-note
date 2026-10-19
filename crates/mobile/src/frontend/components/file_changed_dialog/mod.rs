//! 文件已被外部修改的提示对话框。
//!
//! 遮罩 + 居中卡片，提供"重新加载"和"取消"两个操作。

use dioxus::prelude::*;
use dioxus_i18n::prelude::i18n;

use crate::frontend::i18n as i18n_support;

stylance::import_style!(styles, "file_changed_dialog.module.scss");

#[component]
pub fn FileChangedDialog(
    file_name: String,
    on_reload: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    // i18n Context（渲染期取词自动订阅语言变化）
    let i18n_ctx = i18n();
    let body = i18n_support::translate_with(
        &i18n_ctx,
        "file-changed-body",
        &[("name", file_name.into())],
    );

    rsx! {
        div {
            class: styles::file_changed_dialog,
            onclick: move |_| on_cancel.call(()),

            div {
                class: styles::dialog,
                onclick: move |e: Event<MouseData>| e.stop_propagation(),

                div { class: styles::header, { i18n_support::translate(&i18n_ctx, "file-changed-title") } }

                div { class: styles::body, "{body}" }

                div { class: styles::actions,
                    button {
                        class: styles::btn_cancel,
                        onclick: move |_| on_cancel.call(()),
                        { i18n_support::translate(&i18n_ctx, "common-cancel") }
                    }
                    button {
                        class: styles::btn_reload,
                        onclick: move |_| on_reload.call(()),
                        { i18n_support::translate(&i18n_ctx, "file-changed-reload") }
                    }
                }
            }
        }
    }
}
