use dioxus::prelude::*;

stylance::import_style!(styles, "overlay.module.scss");

/// 浮层遮罩组件
#[component]
pub fn Overlay(on_close: EventHandler<()>) -> Element {
    rsx! {
        div {
            class: styles::overlay,
            onclick: move |_| {
                on_close.call(());
            },
        }
    }
}
