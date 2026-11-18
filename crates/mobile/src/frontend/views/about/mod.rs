use dioxus::logger::tracing;
use dioxus::prelude::*;
use dioxus_i18n::prelude::i18n;
use ui::components::icons::{CodeIcon, DescriptionIcon};

use crate::frontend::hooks::use_top_app_bar_state::use_top_app_bar_state;
use crate::frontend::i18n as i18n_support;
use crate::frontend::store::storage::jni;
use crate::frontend::store::top_app_bar_state::MenuItem;

stylance::import_style!(about_styles, "about.module.scss");

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
/// 源码仓库地址（关于页「源码地址」行点按经原生浏览器打开）。
const REPO_URL: &str = "https://github.com/d2736965/mt-note";

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
                    class: about_styles::row,
                    onclick: move |_| {
                        // 无浏览器等异常由原生层捕获记日志，这里只需感知失败
                        if let Err(e) = jni::open_url(REPO_URL) {
                            tracing::error!("open_url({REPO_URL}): {e}");
                        }
                    },
                    CodeIcon { class: about_styles::row_icon }
                    span { class: about_styles::row_label, { i18n_support::translate(&i18n_ctx, "about-source-code") } }
                }
                button {
                    class: about_styles::row,
                    onclick: move |_| {
                        nav.push("/licenses");
                    },
                    DescriptionIcon { class: about_styles::row_icon }
                    span { class: about_styles::row_label, { i18n_support::translate(&i18n_ctx, "about-oss-licenses") } }
                }
            }
            button {
                class: about_styles::footer,
                onclick: move |_| {
                    nav.push("/license");
                },
                "© The MtNote Project Contributors"
            }
        }
    }
}
