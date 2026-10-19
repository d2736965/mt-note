use dioxus::prelude::*;
use mt_note_editor_lsp::types::Completion;

use super::super::styles;

#[component]
pub fn CompletionPopup(
    x: f64,
    y: f64,
    items: Vec<Completion>,
    selected: Option<usize>,
    on_select: EventHandler<Completion>,
) -> Element {
    if items.is_empty() {
        return rsx! {};
    }

    rsx! {
        div {
            class: styles::completion_popup,
            style: "left: {x}px; top: {y}px;",
            for (idx , item) in items.iter().enumerate() {
                {
                    let is_active = Some(idx) == selected;
                    let class: String = if is_active {
                        stylance::classes!(styles::completion_item, styles::active)
                    } else {
                        styles::completion_item.to_string()
                    };
                    let item = item.clone();
                    rsx! {
                        div { class, onclick: move |_| on_select.call(item.clone()),
                            span { class: styles::completion_label, "{item.label}" }
                            if let Some(detail) = &item.detail {
                                span { class: styles::completion_detail, "{detail}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
