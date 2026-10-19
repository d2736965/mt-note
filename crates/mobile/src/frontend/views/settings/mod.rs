use std::path::PathBuf;

use dioxus::logger::tracing;
use dioxus::prelude::*;
use dioxus_i18n::prelude::i18n;
use models::{LangPref, StartupTab, Theme, WordWrap};
use ui::components::icons::{AddIcon, ChevronDownIcon, DeleteIcon, FolderIcon};
use ui::hooks::use_app_state::use_app_state;
use ui::store::app_state::AppState;

use crate::frontend::components::dir_picker::DirPicker;
use crate::frontend::hooks::use_top_app_bar_state::use_top_app_bar_state;
use crate::frontend::hooks::use_back_interceptor::use_back_interceptor;
use crate::frontend::i18n as i18n_support;
use crate::frontend::js_bridge::{DioxusBridge, js_api};
use crate::frontend::store::top_app_bar_state::MenuItem;
use crate::frontend::store::storage::android::{AndroidBackend, AndroidStorage};

stylance::import_style!(styles, "settings.module.scss");

#[component]
pub fn Settings() -> Element {
    let i18n_ctx = i18n();
    if let Ok(mut app_bar) = use_top_app_bar_state() {
        let nav = navigator();
        app_bar.write().set_title(i18n_support::translate(&i18n_ctx, "common-settings"));
        app_bar.write().set_menu_items(vec![
            MenuItem::item(i18n_support::translate(&i18n_ctx, "common-home"), move || {
                nav.push("/");
            }),
            MenuItem::item(i18n_support::translate(&i18n_ctx, "common-about"), move || {
                nav.push("/about");
            }),
        ]);
    }

    let mut state = match use_app_state() {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Settings: {e}");
            return rsx! {
                div { { i18n_support::translate(&i18n_ctx, "common-app-state-error") } }
            };
        }
    };

    let current_theme = use_memo(move || *state.read().theme());
    let current_startup_tab = use_memo(move || state.read().general_prefs().startup_tab);
    let current_lang_pref = use_memo(move || state.read().general_prefs().language.clone());
    let current_word_wrap = use_memo(move || state.read().editor_prefs().word_wrap);
    let current_show_line_numbers = use_memo(move || state.read().editor_prefs().show_line_numbers);

    let mut new_font_path = use_signal(String::new);
    let mut show_font_dir_picker = use_signal(|| false);
    let mut default_dir_input = use_signal(|| None::<String>);
    let current_default_dir = use_memo(move || state.read().browser_prefs().default_dir.clone());
    let mut show_dir_picker = use_signal(|| false);

    let mut show_language_menu = use_signal(|| false);
    let has_picker = show_dir_picker() || show_font_dir_picker() || show_language_menu();
    use_back_interceptor(has_picker, move || {
        if show_language_menu() {
            show_language_menu.set(false);
        } else if show_dir_picker() {
            show_dir_picker.set(false);
        } else if show_font_dir_picker() {
            show_font_dir_picker.set(false);
        }
    });

    let save_typst_prefs = move || {
        if let Ok(backend) = AndroidBackend::new() {
            let storage = AndroidStorage::new(backend);
            if let Err(e) = storage.save(state.read().typst_prefs()) {
                tracing::error!("persist typst_prefs: {e:?}");
            }
        }
    };

    let save_editor_prefs = move || {
        if let Ok(backend) = AndroidBackend::new() {
            let storage = AndroidStorage::new(backend);
            if let Err(e) = storage.save(state.read().editor_prefs()) {
                tracing::error!("persist editor_prefs: {e:?}");
            }
        }
    };

    let save_browser_prefs = move || {
        if let Ok(backend) = AndroidBackend::new() {
            let storage = AndroidStorage::new(backend);
            if let Err(e) = storage.save(state.read().browser_prefs()) {
                tracing::error!("persist browser_prefs: {e:?}");
            }
        }
    };

    let save_general_prefs = move || {
        if let Ok(backend) = AndroidBackend::new() {
            let storage = AndroidStorage::new(backend);
            if let Err(e) = storage.save(state.read().general_prefs()) {
                tracing::error!("persist general_prefs: {e:?}");
            }
        }
    };

    // 切换语言：写持久化偏好 + 应用解析结果（System 需按系统语言即时解析），
    // 通知 i18n Context 触发全树文案重渲染，最后落盘。
    let mut switch_language = move |pref: LangPref| {
        let resolved = i18n_support::resolve_language(&pref);
        state.write().general_prefs_mut().language = pref;
        let mut i18n_ctx = i18n_ctx;
        i18n_support::set_language(&mut i18n_ctx, &resolved);
        // TS 侧词典同步（markdown 工具条/灯箱等 JS 动态文案）
        js_api::i18n::set_locale::<DioxusBridge>(&resolved);
        save_general_prefs();
    };

    // 当前生效语言标签：显式选择直接用；跟随系统按系统语言协商（JNI 结果
    // 进程内缓存）。用于右段显示当前语言自名与列表高亮。
    let resolved_lang = i18n_support::resolve_language(&state.read().general_prefs().language);
    let current_lang_name = i18n_support::locale_options()
        .iter()
        .find(|(tag, _)| *tag == resolved_lang)
        .map(|(_, name)| name.clone())
        .unwrap_or_else(|| resolved_lang.clone());

    rsx! {
        div { class: styles::settings_view,
            div { class: styles::settings_section,
                h2 { class: styles::section_title, { i18n_support::translate(&i18n_ctx, "settings-section-general") } }
                // 语言：两段式——左段「跟随系统」点击即随系统；右段显示当前语言，
                // 点击弹出纵向语言列表（选项由 locales/ 目录动态生成，语言自名固定原文不翻译）。
                div { class: styles::setting_item,
                    div { class: styles::setting_info,
                        span { class: styles::setting_label,
                            { i18n_support::translate(&i18n_ctx, "settings-language") }
                        }
                        span { class: styles::setting_description,
                            { i18n_support::translate(&i18n_ctx, "settings-language-desc") }
                        }
                    }
                    div { class: styles::lang_switcher,
                        div { class: styles::lang_segmented,
                            button {
                                class: if current_lang_pref().is_system() { stylance::classes!(styles::theme_btn, styles::active) } else { stylance::classes!(styles::theme_btn) },
                                r#type: "button",
                                onclick: move |_| switch_language(LangPref::system()),
                                { i18n_support::translate(&i18n_ctx, "settings-follow-system") }
                            }
                            button {
                                class: if current_lang_pref().is_system() { stylance::classes!(styles::theme_btn) } else { stylance::classes!(styles::theme_btn, styles::active) },
                                r#type: "button",
                                onclick: move |_| show_language_menu.toggle(),
                                { current_lang_name }
                                ChevronDownIcon { class: styles::lang_chevron }
                            }
                        }
                        if show_language_menu() {
                            div { class: styles::lang_menu_mask, onclick: move |_| show_language_menu.set(false) }
                            div { class: styles::lang_menu,
                                for (tag, name) in i18n_support::locale_options().to_vec() {
                                    button {
                                        class: if resolved_lang == tag { stylance::classes!(styles::lang_menu_item, styles::active) } else { stylance::classes!(styles::lang_menu_item) },
                                        r#type: "button",
                                        onclick: move |_| {
                                            show_language_menu.set(false);
                                            switch_language(LangPref::from_tag(&tag));
                                        },
                                        { name }
                                    }
                                }
                            }
                        }
                    }
                }


                div { class: styles::setting_item,
                    div { class: styles::setting_info,
                        span { class: styles::setting_label, { i18n_support::translate(&i18n_ctx, "settings-startup-page") } }
                        span { class: styles::setting_description,
                            { i18n_support::translate(&i18n_ctx, "settings-startup-page-desc") }
                        }
                    }
                    div { class: styles::theme_switcher,
                        button {
                            class: if current_startup_tab() == StartupTab::Files { stylance::classes!(styles::theme_btn, styles::active) } else { stylance::classes!(styles::theme_btn) },
                            onclick: move |_| {
                                state.write().general_prefs_mut().startup_tab = StartupTab::Files;
                                save_general_prefs();
                            },
                            { i18n_support::translate(&i18n_ctx, "settings-startup-files") }
                        }
                        button {
                            class: if current_startup_tab() == StartupTab::Favorites { stylance::classes!(styles::theme_btn, styles::active) } else { stylance::classes!(styles::theme_btn) },
                            onclick: move |_| {
                                state.write().general_prefs_mut().startup_tab = StartupTab::Favorites;
                                save_general_prefs();
                            },
                            { i18n_support::translate(&i18n_ctx, "settings-startup-favorites") }
                        }
                    }
                }

                div { class: styles::setting_item,
                    div { class: styles::setting_info,
                        span { class: styles::setting_label, { i18n_support::translate(&i18n_ctx, "settings-default-dir") } }
                        span { class: styles::setting_description,
                            { i18n_support::translate(&i18n_ctx, "settings-default-dir-desc") }
                        }
                    }
                }

                div { class: styles::dir_input_row,
                    div { class: styles::dir_input_wrapper,
                        input {
                            class: styles::dir_input,
                            r#type: "text",
                            placeholder: &i18n_support::translate(&i18n_ctx, "settings-default-dir-placeholder"),
                            value: match default_dir_input() {
                                Some(v) => v,
                                None => current_default_dir().unwrap_or_default(),
                            },
                            oninput: move |e| {
                                default_dir_input.set(Some(e.value()));
                            },
                        }
                        button {
                            class: styles::dir_picker_btn,
                            r#type: "button",
                            onclick: move |_| {
                                show_dir_picker.set(true);
                            },
                            FolderIcon { class: styles::dir_picker_icon }
                        }
                    }
                    button {
                        class: styles::dir_save_btn,
                        r#type: "button",
                        onclick: move |_| {
                            let raw = match default_dir_input() {
                                Some(v) => Some(v),
                                None => current_default_dir(),
                            }
                                .unwrap_or_default();
                            let trimmed = raw.trim();
                            let new_val = if trimmed.is_empty() { None } else { Some(trimmed.to_string()) };
                            state.write().browser_prefs_mut().default_dir = new_val.clone();
                            match new_val {
                                Some(d) => {
                                    state.write().set_current_dir(d.into());
                                }
                                None => {
                                    state.write().set_current_dir(AppState::default_dir());
                                }
                            }
                            default_dir_input.set(None);
                            save_browser_prefs();
                        },
                        { i18n_support::translate(&i18n_ctx, "common-save") }
                    }
                }

                if show_dir_picker() {
                    {
                        let initial = match default_dir_input() {
                            Some(v) if !v.trim().is_empty() => PathBuf::from(v.trim()),
                            _ => {
                                current_default_dir()
                                    .map(PathBuf::from)
                                    .unwrap_or_else(|| PathBuf::from("/sdcard"))
                            }
                        };
                        rsx! {
                            DirPicker {
                                initial_dir: initial,
                                on_select: move |path: PathBuf| {
                                    default_dir_input.set(Some(path.to_string_lossy().to_string()));
                                    show_dir_picker.set(false);
                                },
                                on_cancel: move |_| {
                                    show_dir_picker.set(false);
                                },
                            }
                        }
                    }
                }
            }

            div { class: styles::settings_section,
                h2 { class: styles::section_title, { i18n_support::translate(&i18n_ctx, "settings-section-appearance") } }
                div { class: styles::setting_item,
                    div { class: styles::setting_info,
                        span { class: styles::setting_label, { i18n_support::translate(&i18n_ctx, "settings-theme-mode") } }
                        span { class: styles::setting_description, { i18n_support::translate(&i18n_ctx, "settings-theme-mode-desc") } }
                    }
                    div { class: styles::theme_switcher,
                        button {
                            class: if current_theme() == Theme::Light { stylance::classes!(styles::theme_btn, styles::active) } else { stylance::classes!(styles::theme_btn) },
                            onclick: move |_| {
                                state.write().set_theme(Theme::Light);
                            },
                            { i18n_support::translate(&i18n_ctx, "settings-theme-light") }
                        }
                        button {
                            class: if current_theme() == Theme::Dark { stylance::classes!(styles::theme_btn, styles::active) } else { stylance::classes!(styles::theme_btn) },
                            onclick: move |_| {
                                state.write().set_theme(Theme::Dark);
                            },
                            { i18n_support::translate(&i18n_ctx, "settings-theme-dark") }
                        }
                    }
                }
            }

            div { class: styles::settings_section,
                h2 { class: styles::section_title, { i18n_support::translate(&i18n_ctx, "settings-section-editor") } }
                div { class: styles::setting_item,
                    div { class: styles::setting_info,
                        span { class: styles::setting_label, { i18n_support::translate(&i18n_ctx, "settings-wrap-mode") } }
                        span { class: styles::setting_description,
                            { i18n_support::translate(&i18n_ctx, "settings-wrap-mode-desc") }
                        }
                    }
                    div { class: styles::theme_switcher,
                        button {
                            class: if current_word_wrap() == WordWrap::Off { stylance::classes!(styles::theme_btn, styles::active) } else { stylance::classes!(styles::theme_btn) },
                            onclick: move |_| {
                                state.write().editor_prefs_mut().word_wrap = WordWrap::Off;
                                save_editor_prefs();
                            },
                            { i18n_support::translate(&i18n_ctx, "settings-wrap-off") }
                        }
                        button {
                            class: if current_word_wrap() == WordWrap::On { stylance::classes!(styles::theme_btn, styles::active) } else { stylance::classes!(styles::theme_btn) },
                            onclick: move |_| {
                                state.write().editor_prefs_mut().word_wrap = WordWrap::On;
                                save_editor_prefs();
                            },
                            { i18n_support::translate(&i18n_ctx, "settings-wrap-on") }
                        }
                    }
                }
                div { class: styles::setting_item,
                    div { class: styles::setting_info,
                        span { class: styles::setting_label, { i18n_support::translate(&i18n_ctx, "settings-show-line-numbers") } }
                        span { class: styles::setting_description, { i18n_support::translate(&i18n_ctx, "settings-show-line-numbers-desc") } }
                    }
                    label { class: styles::toggle_label,
                        input {
                            r#type: "checkbox",
                            class: styles::toggle_input,
                            checked: current_show_line_numbers(),
                            onchange: move |e| {
                                state.write().editor_prefs_mut().show_line_numbers = e.data.checked();
                                save_editor_prefs();
                            },
                        }
                        span { class: styles::toggle_switch }
                    }
                }
                div { class: styles::setting_item,
                    div { class: styles::setting_info,
                        span { class: styles::setting_label, { i18n_support::translate(&i18n_ctx, "settings-wrap-column") } }
                        span { class: styles::setting_description,
                            { i18n_support::translate(&i18n_ctx, "settings-wrap-column-desc") }
                        }
                    }
                    div { class: styles::stepper,
                        button {
                            class: styles::stepper_btn,
                            onclick: move |_| {
                                let col = state.read().editor_prefs().wrap_column;
                                if col > 20 {
                                    state.write().editor_prefs_mut().wrap_column = col - 10;
                                    save_editor_prefs();
                                }
                            },
                            "-"
                        }
                        span { class: styles::stepper_value, "{state.read().editor_prefs().wrap_column}" }
                        button {
                            class: styles::stepper_btn,
                            onclick: move |_| {
                                let col = state.read().editor_prefs().wrap_column;
                                if col < 120 {
                                    state.write().editor_prefs_mut().wrap_column = col + 10;
                                    save_editor_prefs();
                                }
                            },
                            "+"
                        }
                    }
                }
            }

            div { class: styles::settings_section,
                h2 { class: styles::section_title, { i18n_support::translate(&i18n_ctx, "settings-section-typst") } }
                div { class: styles::setting_item,
                    div { class: styles::setting_info,
                        span { class: styles::setting_label, { i18n_support::translate(&i18n_ctx, "settings-font-paths") } }
                        span { class: styles::setting_description,
                            { i18n_support::translate(&i18n_ctx, "settings-font-paths-desc") }
                        }
                    }
                }

                for (i , path) in state.read().typst_prefs().font_paths.iter().enumerate() {
                    div { key: "{i}", class: styles::font_path_item,
                        FolderIcon { class: styles::font_path_icon }
                        span { class: styles::font_path_text, "{path}" }
                        button {
                            class: styles::font_path_delete,
                            onclick: move |_| {
                                state.write().typst_prefs_mut().font_paths.remove(i);
                                save_typst_prefs();
                            },
                            DeleteIcon { class: styles::font_path_delete_icon }
                        }
                    }
                }

                div { class: styles::font_path_add,
                    div { class: styles::font_path_input_wrapper,
                        input {
                            class: styles::font_path_input,
                            r#type: "text",
                            placeholder: &i18n_support::translate(&i18n_ctx, "settings-font-path-placeholder"),
                            value: "{new_font_path}",
                            oninput: move |e| {
                                new_font_path.set(e.value());
                            },
                        }
                        button {
                            class: styles::font_path_picker_btn,
                            r#type: "button",
                            onclick: move |_| {
                                show_font_dir_picker.set(true);
                            },
                            FolderIcon { class: styles::font_path_picker_icon }
                        }
                    }
                    button {
                        class: styles::font_path_add_btn,
                        onclick: move |_| {
                            let path = new_font_path().trim().to_string();
                            if !path.is_empty() {
                                state.write().typst_prefs_mut().font_paths.push(path);
                                new_font_path.set(String::new());
                                save_typst_prefs();
                            }
                        },
                        AddIcon { class: styles::font_path_add_icon }
                    }
                }

                if show_font_dir_picker() {
                    {
                        let initial = {
                            let v = new_font_path();
                            if v.trim().is_empty() {
                                PathBuf::from("/sdcard")
                            } else {
                                PathBuf::from(v.trim())
                            }
                        };
                        rsx! {
                            DirPicker {
                                initial_dir: initial,
                                on_select: move |path: PathBuf| {
                                    new_font_path.set(path.to_string_lossy().to_string());
                                    show_font_dir_picker.set(false);
                                },
                                on_cancel: move |_| {
                                    show_font_dir_picker.set(false);
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}
