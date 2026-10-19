//! M3 Bottom App Bar——水平容器，action 按钮自然排列、溢出水平滚动。
//! 场景：文本编辑器的底部操作栏。内容由 BottomAppBarConfig 注入。
use dioxus::prelude::*;

use crate::frontend::store::bottom_bar_state::BottomAppBarConfig;

stylance::import_style!(styles, "bottom_app_bar.module.scss");

#[component]
pub fn BottomAppBar(config: BottomAppBarConfig) -> Element {
    rsx! {
        div { class: styles::bottom_app_bar,
            for action in config.actions.iter() {
                {
                    let on_click = action.on_click.clone();
                    let icon_path = action.icon.to_string();
                    rsx! {
                        button {
                            class: styles::action,
                            onmousedown: move |e| {
                                e.prevent_default();
                                on_click.borrow_mut()();
                            },
                            svg { xmlns: "http://www.w3.org/2000/svg", view_box: "0 0 24 24",
                                path { d: "{icon_path}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
