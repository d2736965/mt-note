//! M3 Navigation Bar——水平容器，每个目的地垂直布局（icon 上 / label 下），
//! 整体可点击，active 通过着色区分（无药丸）。内容由 NavBarConfig 注入。

use dioxus::prelude::*;

use crate::frontend::store::bottom_bar_state::{NavBarConfig, NavBarItems};

stylance::import_style!(styles, "nav_bar.module.scss");

#[component]
pub fn NavBar(config: NavBarConfig) -> Element {
    let items: Vec<&crate::frontend::store::bottom_bar_state::NavBarItem> = match &config.items {
        NavBarItems::Three(a) => a.iter().collect(),
        NavBarItems::Four(a) => a.iter().collect(),
        NavBarItems::Five(a) => a.iter().collect(),
    };

    rsx! {
        nav { class: styles::navbar,
            for item in items {
                {
                    let on_click = item.on_click.clone();
                    let icon_path = item.icon.to_string();
                    let label = item.label.clone();
                    let active = item.active;
                    rsx! {
                        button {
                            class: if active { stylance::classes!(styles::item, styles::active) }
                                   else { stylance::classes!(styles::item) },
                            onclick: move |_| {
                                on_click.borrow_mut()();
                            },
                            span { class: styles::icon_pill,
                                svg { xmlns: "http://www.w3.org/2000/svg", view_box: "0 0 24 24",
                                    path { d: "{icon_path}" }
                                }
                            }
                            span { class: styles::label, {label} }
                        }
                    }
                }
            }
        }
    }
}
