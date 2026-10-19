//! 「更多」tab：设置、关于等入口列表。

use dioxus::prelude::*;
use dioxus_i18n::prelude::i18n;
use ui::components::icons::{ChevronRightIcon, InfoIcon, SettingsIcon};

use crate::frontend::hooks::use_top_app_bar_state::use_top_app_bar_state;
use crate::frontend::i18n as i18n_support;

stylance::import_style!(styles, "tab3.module.scss");

/// 「更多」tab。
#[component]
pub fn Tab3() -> Element {
    // i18n Context（渲染期取词自动订阅语言变化）
    let i18n_ctx = i18n();
    if let Ok(mut app_bar) = use_top_app_bar_state() {
        app_bar
            .write()
            .set_title(i18n_support::translate(&i18n_ctx, "home-tab-more"));
    }
    let nav = navigator();

    rsx! {
        div { class: styles::more_list,
            button {
                class: styles::more_item,
                onclick: move |_| {
                    nav.push("/settings");
                },
                SettingsIcon { class: styles::item_icon }
                span { class: styles::item_label, { i18n_support::translate(&i18n_ctx, "common-settings") } }
                ChevronRightIcon { class: styles::item_chevron }
            }
            button {
                class: styles::more_item,
                onclick: move |_| {
                    nav.push("/about");
                },
                InfoIcon { class: styles::item_icon }
                span { class: styles::item_label, { i18n_support::translate(&i18n_ctx, "common-about") } }
                ChevronRightIcon { class: styles::item_chevron }
            }
        }
    }
}
