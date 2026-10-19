use dioxus::prelude::*;
use dioxus_i18n::prelude::i18n;

stylance::import_style!(styles, "editor_error.module.scss");

use crate::frontend::i18n as i18n_support;

/// 编辑器错误信息的全局信号。
///
/// 由 `App` 通过 `use_context_provider` 提供，`EditorError` 页面读取，
/// 编辑器组件（`HighlightedLine`、`SemanticSpans`）在出错时写入。
pub type EditorErrorSignal = Signal<Option<String>>;

/// 编辑器错误页面。
///
/// 当编辑器渲染遇到不可恢复的错误时跳转到此页面，显示错误详情。
/// 错误信息通过 `EditorErrorSignal` 全局 context 传递。
#[component]
pub fn EditorError() -> Element {
    let i18n_ctx = i18n();
    let error_msg = use_context::<EditorErrorSignal>();
    let nav = navigator();

    let message = error_msg().unwrap_or_else(|| i18n_support::translate(&i18n_ctx, "common-unknown-error"));

    rsx! {
        div { class: styles::page,
            div { class: styles::card,
                div { class: styles::icon, "⚠" }
                h1 { class: styles::title, { i18n_support::translate(&i18n_ctx, "err-editor-title") } }
                p { class: styles::description,
                    { i18n_support::translate(&i18n_ctx, "err-editor-desc") }
                }
                div { class: styles::detail,
                    h3 { { i18n_support::translate(&i18n_ctx, "err-editor-detail") } }
                    pre { class: styles::message, "{message}" }
                }
                button {
                    class: styles::back_button,
                    onclick: move |_| nav.go_back(),
                    { i18n_support::translate(&i18n_ctx, "common-return") }
                }
            }
        }
    }
}
