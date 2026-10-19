use crate::frontend::hooks::use_top_app_bar_state::use_top_app_bar_state;
use crate::frontend::store::top_app_bar_state::MenuItem;
use dioxus::logger::tracing;
use dioxus::prelude::*;

#[component]
pub fn MenuEntry(item: MenuItem, on_close: EventHandler<()>) -> Element {
    let mut app_bar = match use_top_app_bar_state() {
        Ok(state) => state,
        Err(e) => {
            tracing::error!("MenuEntry: {e}");
            return rsx! {};
        }
    };

    let MenuItem {
        label,
        on_click,
        children,
    } = item;

    match children {
        Some(sub_items) => rsx! {
            button {
                class: stylance::classes!(super::styles::item, super::styles::has_submenu),
                onclick: move |_| {
                    app_bar.write().open_submenu(sub_items.clone());
                },
                span { {label} }
                span { class: super::styles::arrow, "▶" }
            }
        },
        None => {
            let handler = on_click.clone();
            rsx! {
                button {
                    class: super::styles::item,
                    onclick: move |_| {
                        on_close.call(());
                        if let Some(h) = &handler {
                            h.borrow_mut()();
                        }
                    },
                    span { {label} }
                }
            }
        }
    }
}
