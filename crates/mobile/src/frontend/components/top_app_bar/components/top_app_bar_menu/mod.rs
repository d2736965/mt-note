mod menu_entry;

use crate::frontend::hooks::use_top_app_bar_state::use_top_app_bar_state;
use crate::frontend::i18n as i18n_support;
use dioxus::logger::tracing;
use dioxus::prelude::*;
use dioxus_i18n::prelude::i18n;

use self::menu_entry::MenuEntry;

stylance::import_style!(styles, "menu.module.scss");

#[component]
pub fn TopAppBarMenu(on_close: EventHandler<()>) -> Element {
    // i18n Context（渲染期取词自动订阅语言变化）
    let i18n_ctx = i18n();
    let mut app_bar = match use_top_app_bar_state() {
        Ok(state) => state,
        Err(e) => {
            tracing::error!("TopAppBarMenu: {e}");
            return rsx! {};
        }
    };

    let items = app_bar.read().menu_items().to_vec();
    let has_parent = app_bar.read().menu_history_depth() > 0;

    rsx! {
        div { class: styles::dropdown,
            if has_parent {
                button {
                    class: stylance::classes!(styles::item, styles::back),
                    onclick: move |_| {
                        if let Err(e) = app_bar.write().close_submenu() {
                            tracing::error!("关闭子菜单失败: {e}");
                        }
                    },
                    { i18n_support::translate(&i18n_ctx, "common-back") }
                }
            }
            for item in items {
                MenuEntry { item, on_close }
            }
        }
    }
}
