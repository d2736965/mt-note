use dioxus::prelude::*;
use dioxus_i18n::prelude::i18n;
use ui::components::icons::ChevronRightIcon;

use crate::frontend::hooks::use_top_app_bar_state::use_top_app_bar_state;
use crate::frontend::i18n as i18n_support;
use crate::frontend::store::top_app_bar_state::MenuItem;

stylance::import_style!(about_styles, "about.module.scss");

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[component]
pub fn About() -> Element {
    let i18n_ctx = i18n();
    if let Ok(mut app_bar) = use_top_app_bar_state() {
        let nav = navigator();
        app_bar.write().set_title(i18n_support::translate(&i18n_ctx, "common-about"));
        app_bar.write().set_menu_items(vec![
            MenuItem::item(i18n_support::translate(&i18n_ctx, "common-settings"), move || {
                nav.push("/settings");
            }),
            MenuItem::item(i18n_support::translate(&i18n_ctx, "common-home"), move || {
                nav.push("/");
            }),
        ]);
    }
    let nav = navigator();

    rsx! {
        div { class: about_styles::container,
            div { class: about_styles::header,
                span { class: about_styles::app_name, "Mt Note v{APP_VERSION}" }
            }
            p { class: about_styles::description, { i18n_support::translate(&i18n_ctx, "about-desc") } }
            div { class: about_styles::license_section,
                button {
                    class: about_styles::license_row,
                    onclick: move |_| {
                        nav.push("/license");
                    },
                    span { class: about_styles::license_label, { i18n_support::translate(&i18n_ctx, "common-license") } }
                    span { class: about_styles::license_value, "MIT OR Apache-2.0" }
                    ChevronRightIcon { class: about_styles::license_chevron }
                }
            }
            p { class: about_styles::footer, "© 2026 d2736965" }
        }
    }
}
