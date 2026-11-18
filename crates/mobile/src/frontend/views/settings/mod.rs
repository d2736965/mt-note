use std::collections::HashMap;
use std::path::{Path, PathBuf};

use dioxus::logger::tracing;
use dioxus::prelude::*;
use dioxus_i18n::prelude::i18n;
use models::{LangPref, StartupTab, Theme, WordWrap};
use mt_note_fs_engine::{read_file, write_bytes};
use serde::Deserialize;
use serde::Serialize;
use ui::components::icons::{AddIcon, ChevronDownIcon, DeleteIcon, FolderIcon};
use ui::hooks::use_app_state::use_app_state;
use ui::store::app_state::AppState;

use crate::frontend::components::dir_picker::DirPicker;
use crate::frontend::components::toast::{ToastItem, push_toast};
use crate::frontend::hooks::use_top_app_bar_state::use_top_app_bar_state;
use crate::frontend::hooks::use_back_interceptor::use_back_interceptor;
use crate::frontend::i18n as i18n_support;
use crate::frontend::js_bridge::{DioxusBridge, js_api};
use crate::frontend::store::prefs::apply_persisted_prefs;
use crate::frontend::store::top_app_bar_state::MenuItem;
use crate::frontend::store::storage::android::{AndroidBackend, AndroidStorage};
use crate::frontend::views::editor::components::save_dialog::SaveDialog;
use crate::frontend::views::settings::components::confirm_dialog::ConfirmDialog;
use crate::frontend::views::settings::components::file_dialog::FileDialog;

stylance::import_style!(styles, "settings.module.scss");

pub mod components;

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
    // 备份/恢复浮层状态：另存为对话框、恢复文件选择器、恢复确认框
    // （确认框挂在待恢复路径上：Some 即展示）
    let mut show_backup_dialog = use_signal(|| false);
    let mut show_restore_picker = use_signal(|| false);
    let mut pending_restore_path = use_signal(|| None::<PathBuf>);
    let has_picker = show_dir_picker()
        || show_font_dir_picker()
        || show_language_menu()
        || show_backup_dialog()
        || show_restore_picker()
        || pending_restore_path().is_some();
    use_back_interceptor(has_picker, move || {
        if show_language_menu() {
            show_language_menu.set(false);
        } else if show_dir_picker() {
            show_dir_picker.set(false);
        } else if show_font_dir_picker() {
            show_font_dir_picker.set(false);
        } else if show_backup_dialog() {
            show_backup_dialog.set(false);
        } else if show_restore_picker() {
            show_restore_picker.set(false);
        } else if pending_restore_path().is_some() {
            pending_restore_path.set(None);
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

    // ===== 备份 / 恢复 =====
    let toasts: Signal<Vec<ToastItem>> = use_context();
    // 备份文件默认目录：浏览器默认目录优先，其次 Documents
    let backup_initial_dir = state
        .read()
        .browser_prefs()
        .default_dir
        .clone()
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/sdcard/Documents"));

    // 备份：读取 persist.json 原始键值表，套元信息壳后写到用户选定路径
    // （本地读写毫秒级完成，不做进度提示，直接出结果 toast）
    let do_backup = move |target: PathBuf| {
        show_backup_dialog.set(false);
        spawn(async move {
            let i18n_ctx = i18n_ctx;
            match backup_to_file(&target) {
                Ok(()) => push_toast(
                    toasts,
                    i18n_support::translate_with(
                        &i18n_ctx,
                        "settings-backup-done",
                        &[("path", target.display().to_string().into())],
                    ),
                ),
                Err(e) => push_toast(
                    toasts,
                    i18n_support::translate_err(&i18n_ctx, "err-backup-write", &e),
                ),
            }
        });
    };

    // 恢复：合并写回备份内容后，立即重应用全部偏好（signal 驱动，主题/
    // 编辑器等设置即时生效；收藏列表在回到主页挂载时重读）
    let mut do_restore = move |path: PathBuf| {
        pending_restore_path.set(None);
        spawn(async move {
            let mut i18n_ctx = i18n_ctx;
            match restore_from_file(&path) {
                Ok(count) => {
                    if let Ok(backend) = AndroidBackend::new() {
                        let storage = AndroidStorage::new(backend);
                        apply_persisted_prefs(&mut state, &storage, &mut i18n_ctx);
                    }
                    push_toast(
                        toasts,
                        i18n_support::translate_with(
                            &i18n_ctx,
                            "settings-restored",
                            &[("count", (count as i64).into())],
                        ),
                    );
                }
                Err(e) => push_toast(
                    toasts,
                    i18n_support::translate_err(&i18n_ctx, e.message_key(), &e.to_string()),
                ),
            }
        });
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

            div { class: styles::settings_section,
                h2 { class: styles::section_title, { i18n_support::translate(&i18n_ctx, "settings-section-backup") } }
                div { class: styles::setting_item,
                    div { class: styles::setting_info,
                        span { class: styles::setting_label,
                            { i18n_support::translate(&i18n_ctx, "settings-backup") }
                        }
                        span { class: styles::setting_description,
                            { i18n_support::translate(&i18n_ctx, "settings-backup-desc") }
                        }
                    }
                    div { class: styles::backup_actions,
                        button {
                            class: stylance::classes!(styles::backup_btn, styles::primary),
                            r#type: "button",
                            onclick: move |_| show_backup_dialog.set(true),
                            { i18n_support::translate(&i18n_ctx, "settings-backup-btn") }
                        }
                        button {
                            class: stylance::classes!(styles::backup_btn, styles::secondary),
                            r#type: "button",
                            onclick: move |_| show_restore_picker.set(true),
                            { i18n_support::translate(&i18n_ctx, "settings-restore-btn") }
                        }
                    }
                }
            }

            // 备份另存为对话框：选目录 + 文件名（默认 mt-note-backup.json）
            if show_backup_dialog() {
                SaveDialog {
                    initial_dir: backup_initial_dir.clone(),
                    default_name: "mt-note-backup".to_string(),
                    extension: "json".to_string(),
                    on_save: do_backup,
                    on_cancel: move |_| show_backup_dialog.set(false),
                }
            }

            // 恢复：先选备份文件，再确认覆盖
            if show_restore_picker() {
                FileDialog {
                    initial_dir: backup_initial_dir.clone(),
                    on_select: move |path: PathBuf| {
                        show_restore_picker.set(false);
                        pending_restore_path.set(Some(path));
                    },
                    on_cancel: move |_| show_restore_picker.set(false),
                }
            }

            if let Some(path) = pending_restore_path() {
                ConfirmDialog {
                    message: i18n_support::translate(&i18n_ctx, "settings-restore-confirm"),
                    on_confirm: move |_| do_restore(path.clone()),
                    on_cancel: move |_| pending_restore_path.set(None),
                }
            }
        }
    }
}

/// 备份文件壳结构：`data` 为 persist.json 的原始键值表。
#[derive(Serialize, Deserialize)]
struct BackupFile {
    app: String,
    format: u32,
    data: HashMap<String, String>,
}

/// 备份壳校验字段：非本应用或未知格式版本的文件拒绝恢复。
const BACKUP_APP_ID: &str = "mt-note";
const BACKUP_FORMAT: u32 = 1;

/// 恢复失败原因：区分「文件读不出来」与「不是合法备份文件」两类提示。
#[derive(thiserror::Error, Debug)]
enum RestoreError {
    #[error("{0}")]
    Read(String),
    #[error("{0}")]
    Format(String),
}

impl RestoreError {
    /// 对应的 toast 文案键。
    fn message_key(&self) -> &'static str {
        match self {
            RestoreError::Read(_) => "err-restore-read",
            RestoreError::Format(_) => "err-restore-format",
        }
    }
}

/// 导出当前全部持久化数据（persist.json 原始键值表）到 `target`。
fn backup_to_file(target: &Path) -> Result<(), String> {
    let backend = AndroidBackend::new().map_err(|e| e.to_string())?;
    let config = backend.read_raw_config().map_err(|e| e.to_string())?;
    let payload = BackupFile {
        app: BACKUP_APP_ID.to_string(),
        format: BACKUP_FORMAT,
        data: config,
    };
    let content = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
    write_bytes(target, content.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

/// 从 `source` 读取备份文件并合并写回 persist 存储，返回恢复的键数。
///
/// 校验壳的 app/format 字段（不匹配视为格式错误）；逐 key 覆盖合并，
/// 保留备份里没有的本地 key（如未来版本新增的偏好项）。
fn restore_from_file(source: &Path) -> Result<usize, RestoreError> {
    let content = read_file(source).map_err(|e| RestoreError::Read(e.to_string()))?;
    let backup: BackupFile =
        serde_json::from_str(&content).map_err(|e| RestoreError::Format(e.to_string()))?;
    if backup.app != BACKUP_APP_ID {
        return Err(RestoreError::Format(format!(
            "app != {BACKUP_APP_ID}: {}",
            backup.app
        )));
    }
    if backup.format != BACKUP_FORMAT {
        return Err(RestoreError::Format(format!(
            "format != {BACKUP_FORMAT}: {}",
            backup.format
        )));
    }
    let backend = AndroidBackend::new().map_err(|e| RestoreError::Read(e.to_string()))?;
    let mut current = backend
        .read_raw_config()
        .map_err(|e| RestoreError::Read(e.to_string()))?;
    let count = backup.data.len();
    for (key, value) in backup.data {
        current.insert(key, value);
    }
    backend
        .write_raw_config(&current)
        .map_err(|e| RestoreError::Read(e.to_string()))?;
    Ok(count)
}
