//! 日期时间插入对话框。
//!
//! 功能设计：
//! - 格式输入框 + 下拉选择（预定义格式 + 实时预览）
//! - 日期选择器 + 时间选择器（可选任意日期时间）
//! - 预览区：显示当前格式 × 选定日期时间的结果
//! - "使用当前日期时间" 复选框
//! - 快捷按钮：最近使用、仅日期、仅时间、yyyy-MM-dd
//! - 最近格式记忆（localStorage 持久化）

use dioxus::prelude::*;
use dioxus_i18n::prelude::i18n;

use crate::frontend::i18n as i18n_support;
use crate::frontend::js_bridge::{DioxusBridge, js_api};

stylance::import_style!(styles, "datetime_dialog.module.scss");

/// 预定义日期时间格式。
const PREDEFINED_FORMATS: &[&str] = &[
    "yyyy/MM/dd",
    "yyyy-MM-dd",
    "yyyy/MM/dd HH:mm",
    "yyyy-MM-dd HH:mm",
    "yyyy年MM月dd日 HH:mm",
    "yyyy年MM月dd日",
    "MM月dd日 HH:mm",
    "hh:mm",
    "HH:mm:ss",
    "hh:mm:ss",
    "dd-MM-yyyy HH:mm",
    "dd-MM-yyyy",
    "dd.MM.yyyy",
    "MM/dd/yyyy",
    "MMM yyyy",
    "dd hh:mm",
    "dd-MM-yyyy hh:mm",
    "dd-MM-yy",
    "MM/dd/yy",
    "dd.MM.yy",
    "yy/MM/dd",
];

/// 日期时间插入对话框。
///
/// - `open`：控制对话框显示/隐藏。
/// - `content`：编辑器内容信号，插入后更新。
/// - `dirty`：脏标记信号，插入后设为 true。
#[component]
pub fn DatetimeDialog(open: Signal<bool>, content: Signal<String>, dirty: Signal<bool>) -> Element {
    let i18n_ctx = i18n();
    let mut format_str = use_signal(String::new);
    let mut date_str = use_signal(String::new);
    let mut time_str = use_signal(String::new);
    let use_current = use_signal(|| true);
    let mut show_dropdown = use_signal(|| false);
    let mut preview = use_signal(String::new);
    let mut recent_formats = use_signal(Vec::<String>::new);

    // 打开对话框时初始化各字段
    use_effect(move || {
        if open() {
            spawn(async move {
                // 加载最近格式
                let recent_json = js_api::editor::get_recent_formats::<DioxusBridge>().await;
                let recent: Vec<String> = serde_json::from_str(&recent_json).unwrap_or_default();
                recent_formats.set(recent.clone());

                // 默认格式：最近使用的第一个，否则 yyyy-MM-dd
                let default_fmt = recent
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "yyyy-MM-dd".to_string());
                format_str.set(default_fmt.clone());

                // 默认日期时间：当前
                let now =
                    js_api::editor::format_date_time::<DioxusBridge>("yyyy-MM-dd", None).await;
                date_str.set(now.clone());
                let now_time =
                    js_api::editor::format_date_time::<DioxusBridge>("HH:mm", None).await;
                time_str.set(now_time.clone());

                // 更新预览
                let preview_text = js_api::editor::preview_date_time::<DioxusBridge>(
                    &default_fmt,
                    &date_str(),
                    &time_str(),
                    true,
                )
                .await;
                preview.set(preview_text);
            });
        }
    });

    // 更新预览（当格式、日期、时间、use_current 变化时）
    let update_preview = {
        let format_str = format_str;
        let date_str = date_str;
        let time_str = time_str;
        let mut preview = preview;
        move || {
            let fmt = format_str();
            let d = date_str();
            let t = time_str();
            let uc = use_current();
            spawn(async move {
                let text =
                    js_api::editor::preview_date_time::<DioxusBridge>(&fmt, &d, &t, uc).await;
                preview.set(text);
            });
        }
    };

    if !open() {
        return rsx! {};
    }

    let insert_and_close = {
        let mut open = open;
        let mut content = content;
        let mut dirty = dirty;
        let format_str = format_str;
        move |text: String| {
            let fmt = format_str();
            spawn(async move {
                js_api::editor::save_recent_format::<DioxusBridge>(&fmt);
                js_api::editor::insert_text_at_cursor::<DioxusBridge>(&text);
                content.set(content());
                dirty.set(true);
            });
            open.set(false);
        }
    };

    // 日期选择器的 today 值
    let today = date_str();
    // 时间选择器的 now 值
    let now_time = time_str();

    rsx! {
        div { class: styles::datetime_dialog, onclick: move |_| open.set(false),

            div {
                class: styles::dialog,
                onclick: move |e: Event<MouseData>| e.stop_propagation(),

                div { class: styles::header, { i18n_support::translate(&i18n_ctx, "dt-title") } }

                div { class: styles::body,

                    // === 格式输入行 ===
                    div { class: styles::format_row,
                        input {
                            class: styles::format_input,
                            r#type: "text",
                            value: "{format_str}",
                            placeholder: "yyyy-MM-dd HH:mm",
                            oninput: {
                                let mut format_str = format_str;
                                let update = update_preview;
                                move |e| {
                                    format_str.set(e.value());
                                    update();
                                }
                            },
                            onfocus: move |_| show_dropdown.set(true),
                        }
                        button {
                            class: styles::dropdown_toggle,
                            onclick: move |e: Event<MouseData>| {
                                e.prevent_default();
                                show_dropdown.set(!show_dropdown());
                            },
                            "▾"
                        }
                    }

                    // === 格式下拉列表 ===
                    if show_dropdown() {
                        div { class: styles::dropdown,
                            for fmt in PREDEFINED_FORMATS.iter() {
                                button {
                                    class: styles::dropdown_item,
                                    onclick: move |e: Event<MouseData>| {
                                        e.prevent_default();
                                        let f = fmt.to_string();
                                        let mut format_str = format_str;
                                        let mut show_dropdown = show_dropdown;
                                        let update = update_preview;
                                        async move {
                                            format_str.set(f.clone());
                                            show_dropdown.set(false);
                                            update();
                                        }
                                    },
                                    span { class: styles::dropdown_item, "{fmt}" }
                                }
                            }
                        }
                    }

                    // === 日期选择器行 ===
                    div { class: styles::picker_row,
                        label { class: styles::picker_label, { i18n_support::translate(&i18n_ctx, "dt-date") } }
                        input {
                            class: styles::picker_input,
                            r#type: "date",
                            value: "{today}",
                            oninput: {
                                let mut date_str = date_str;
                                let mut use_current = use_current;
                                let update = update_preview;
                                move |e| {
                                    use_current.set(false);
                                    date_str.set(e.value());
                                    update();
                                }
                            },
                        }
                    }

                    // === 时间选择器行 ===
                    div { class: styles::picker_row,
                        label { class: styles::picker_label, { i18n_support::translate(&i18n_ctx, "dt-time") } }
                        input {
                            class: styles::picker_input,
                            r#type: "time",
                            value: "{now_time}",
                            oninput: {
                                let mut time_str = time_str;
                                let mut use_current = use_current;
                                let update = update_preview;
                                move |e| {
                                    use_current.set(false);
                                    time_str.set(e.value());
                                    update();
                                }
                            },
                        }
                    }

                    // === "使用当前日期时间" 复选框 ===
                    label {
                        class: styles::checkbox_row,
                        onclick: move |e: Event<MouseData>| e.prevent_default(),
                        input {
                            r#type: "checkbox",
                            class: styles::checkbox,
                            checked: use_current(),
                            onmousedown: move |e: Event<MouseData>| {
                                e.prevent_default();
                                let mut use_current = use_current;
                                let update = update_preview;
                                use_current.set(!use_current());
                                update();
                            },
                            onclick: move |e: Event<MouseData>| {
                                e.stop_propagation();
                            },
                            onchange: {
                                let mut use_current = use_current;
                                let update = update_preview;
                                move |e| {
                                    use_current.set(e.data.checked());
                                    update();
                                }
                            },
                        }
                        span { class: styles::checkbox_label, { i18n_support::translate(&i18n_ctx, "dt-use-current") } }
                    }

                    // === 预览区 ===
                    div { class: styles::preview_area,
                        div { class: styles::preview_label, { i18n_support::translate(&i18n_ctx, "dt-preview") } }
                        div { class: styles::preview_text, "{preview}" }
                    }

                    // === 快捷按钮 ===
                    div { class: styles::quick_buttons,
                        {
                            let recent = recent_formats();
                            if !recent.is_empty() {
                                let last = recent[0].clone();
                                let insert = insert_and_close;
                                rsx! {
                                    button {
                                        class: styles::quick_btn,
                                        onclick: move |e: Event<MouseData>| {
                                            e.prevent_default();
                                            let mut insert = insert;
                                            let mut format_str = format_str;
                                            let last = last.clone();
                                            spawn(async move {
                                                format_str.set(last.clone());
                                                let last2 = last.clone();
                                                let text = js_api::editor::format_date_time::<DioxusBridge>(&last2, None)
                                                    .await;
                                                insert(text);
                                            });
                                        },
                                        { i18n_support::translate(&i18n_ctx, "dt-recent") }
                                    }
                                }
                            } else {
                                rsx! {}
                            }
                        }
                        button {
                            class: styles::quick_btn,
                            onclick: move |e: Event<MouseData>| {
                                e.prevent_default();
                                let mut insert = insert_and_close;
                                spawn(async move {
                                    let fmt = js_api::editor::get_device_date_format::<DioxusBridge>().await;
                                    let text = js_api::editor::format_date_time::<DioxusBridge>(&fmt, None)
                                        .await;
                                    insert(text);
                                });
                            },
                            { i18n_support::translate(&i18n_ctx, "dt-only-date") }
                        }
                        button {
                            class: styles::quick_btn,
                            onclick: move |e: Event<MouseData>| {
                                e.prevent_default();
                                let mut insert = insert_and_close;
                                spawn(async move {
                                    let fmt = js_api::editor::get_device_time_format::<DioxusBridge>().await;
                                    let text = js_api::editor::format_date_time::<DioxusBridge>(&fmt, None)
                                        .await;
                                    insert(text);
                                });
                            },
                            { i18n_support::translate(&i18n_ctx, "dt-only-time") }
                        }
                        button {
                            class: styles::quick_btn,
                            onclick: move |e: Event<MouseData>| {
                                e.prevent_default();
                                let mut insert = insert_and_close;
                                let mut format_str = format_str;
                                spawn(async move {
                                    format_str.set("yyyy-MM-dd".to_string());
                                    let text = js_api::editor::format_date_time::<
                                        DioxusBridge,
                                    >("yyyy-MM-dd", None)
                                        .await;
                                    insert(text);
                                });
                            },
                            "yyyy-MM-dd"
                        }
                    }

                    // === 底部按钮 ===
                    div { class: styles::bottom_buttons,
                        button {
                            class: styles::cancel_btn,
                            onclick: move |e: Event<MouseData>| {
                                e.prevent_default();
                                open.set(false);
                            },
                            { i18n_support::translate(&i18n_ctx, "common-cancel") }
                        }
                        button {
                            class: styles::confirm_btn,
                            onclick: move |e: Event<MouseData>| {
                                e.prevent_default();
                                let mut insert = insert_and_close;
                                let preview = preview;
                                spawn(async move {
                                    let text = preview();
                                    insert(text);
                                });
                            },
                            { i18n_support::translate(&i18n_ctx, "common-confirm") }
                        }
                    }
                }
            }
        }
    }
}
