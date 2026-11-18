//! 确认对话框（遮罩 + 卡片 + 正文 + 取消/确认）。
//!
//! 用于破坏性操作前的确认，如备份恢复覆盖当前数据。

use dioxus::prelude::*;
use dioxus_i18n::prelude::i18n;

use crate::frontend::i18n as i18n_support;

stylance::import_style!(styles, "confirm_dialog.module.scss");

/// 确认对话框。
///
/// - `message`：正文说明（含操作后果警示）。
/// - `on_confirm`：点击确认时回调。
/// - `on_cancel`：点击取消或遮罩时回调。
#[component]
pub fn ConfirmDialog(
    message: String,
    on_confirm: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let i18n_ctx = i18n();

    rsx! {
        div {
            class: styles::confirm_dialog,
            onclick: move |_| on_cancel.call(()),

            div {
                class: styles::dialog,
                onclick: move |e: Event<MouseData>| e.stop_propagation(),

                div { class: styles::body, "{message}" }

                div { class: styles::action_row,
                    button {
                        class: styles::btn_cancel,
                        onclick: move |_| on_cancel.call(()),
                        { i18n_support::translate(&i18n_ctx, "common-cancel") }
                    }
                    button {
                        class: styles::btn_confirm,
                        onclick: move |_| on_confirm.call(()),
                        { i18n_support::translate(&i18n_ctx, "common-confirm") }
                    }
                }
            }
        }
    }
}
