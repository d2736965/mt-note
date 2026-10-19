use crate::frontend::components::top_app_bar::components::TopAppBarMenu;
use crate::frontend::hooks::use_back_interceptor::use_back_interceptor;
use crate::frontend::hooks::use_top_app_bar_state::use_top_app_bar_state;
use crate::frontend::i18n as i18n_support;
use crate::frontend::router::Route;
use dioxus::logger::tracing;
use dioxus::prelude::*;
use dioxus_i18n::prelude::i18n;
use ui::components::icons::{ArrowBackIcon, MoreVertIcon, SearchIcon};
use ui::components::overlay::Overlay;

pub mod components;

stylance::import_style!(styles, "top_app_bar.module.scss");

/// TopAppBar 组件
#[component]
pub fn TopAppBar() -> Element {
    // i18n Context（渲染期取词自动订阅语言变化）
    let i18n_ctx = i18n();
    let mut menu_open = use_signal(|| false);
    let nav = navigator();
    let route = use_route::<Route>();
    let mut app_bar = match use_top_app_bar_state() {
        Ok(state) => state,
        Err(e) => {
            tracing::error!("TopAppBar: {e}");
            return rsx! { { i18n_support::translate(&i18n_ctx, "common-app-state-error") } };
        }
    };
    use_back_interceptor(menu_open(), move || {
        menu_open.set(false);
        app_bar.write().reset_menu_navigation();
    });

    let is_searching = *app_bar.read().search_mode();
    let show_back = app_bar
        .read()
        .show_back()
        .as_ref()
        .copied()
        .unwrap_or(!matches!(route, Route::Home {}));

    rsx! {
        header { class: if is_searching { stylance::classes!(styles::top_app_bar, styles::searching) } else { stylance::classes!(styles::top_app_bar) },

            if is_searching {
                button {
                    class: styles::leading,
                    onclick: move |_| {
                        app_bar.write().exit_search();
                    },
                    ArrowBackIcon {}
                }
            } else if show_back {
                button {
                    class: styles::leading,
                    onclick: move |_| {
                        nav.go_back();
                    },
                    ArrowBackIcon {}
                }
            }

            if is_searching {
                div { class: styles::search_field,

                    SearchIcon {}

                    input {
                        r#type: "search",
                        placeholder: i18n_support::translate(&i18n_ctx, "common-search"),
                        value: "{app_bar.read().search_query()}",
                        autofocus: true,
                        oninput: move |e| {
                            app_bar.write().set_search_query(e.value());
                        },
                    }
                }
            } else {
                h1 { class: styles::headline, {app_bar.read().title().to_string()} }
            }

            div { class: styles::trailing,

                for btn in app_bar.read().trailing_buttons().iter().take(4) {
                    {
                        let on_click = btn.on_click.clone();
                        let icon_path = btn.current_icon().to_string();
                        let active = btn.active;
                        let has_dual_icon = btn.icon_off.is_some();
                        let class = if active && has_dual_icon {
                            stylance::classes!(styles::trailing_btn)
                        } else if active {
                            stylance::classes!(styles::trailing_btn, styles::trailing_btn_active)
                        } else {
                            stylance::classes!(styles::trailing_btn)
                        };
                        rsx! {
                            button {
                                class,
                                onclick: move |_| {
                                    on_click.borrow_mut()();
                                },
                                svg { xmlns: "http://www.w3.org/2000/svg", view_box: "0 0 24 24",
                                    path { d: "{icon_path}" }
                                }
                            }
                        }
                    }
                }

                if !app_bar.read().menu_items().is_empty() {
                    button {
                        class: stylance::classes!(styles::trailing_btn, styles::trailing_btn_menu),
                        onclick: move |_| {
                            menu_open.set(!menu_open());
                        },
                        MoreVertIcon {}
                    }
                }
            }

            if menu_open() {
                Overlay {
                    on_close: move |_| {
                        menu_open.set(false);
                        app_bar.write().reset_menu_navigation();
                    },
                }
                TopAppBarMenu {
                    on_close: move |_| {
                        menu_open.set(false);
                        app_bar.write().reset_menu_navigation();
                    },
                }
            }
        }
    }
}
