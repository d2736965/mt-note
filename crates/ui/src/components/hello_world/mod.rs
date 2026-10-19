use dioxus::prelude::*;

stylance::import_style!(styles, "hello_world.module.scss");

#[component]
pub fn HelloWorld() -> Element {
    rsx! {
        div { class: styles::container,
            h1 { class: styles::title, "Hello, World!" }
            p { class: styles::subtitle, "Welcome to Dioxus" }
        }
    }
}
