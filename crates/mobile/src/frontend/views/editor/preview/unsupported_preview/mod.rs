//! 不支持预览的文件类型占位组件。

use dioxus::prelude::*;
use dioxus_i18n::prelude::i18n;
use ui::components::icons::CodeIcon;

stylance::import_style!(styles, "unsupported_preview.module.scss");

use crate::frontend::i18n as i18n_support;

/// 不支持预览的文件类型占位组件。
#[component]
pub fn UnsupportedPreview() -> Element {
    let i18n_ctx = i18n();
    rsx! {
        div { class: styles::preview,
            div { class: styles::preview_placeholder,
                CodeIcon { class: styles::placeholder_icon }
                span { { i18n_support::translate(&i18n_ctx, "preview-unsupported") } }
            }
        }
    }
}
