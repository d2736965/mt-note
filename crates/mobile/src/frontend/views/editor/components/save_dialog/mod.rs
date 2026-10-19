//! "另存为"模态对话框。
//!
//! 用于"保存为 PDF"等场景：浏览目录 + 输入文件名 + 保存/取消。
//! 基于 DirLister 列目录，遮罩 + 居中卡片。

use std::path::{Path, PathBuf};

use dioxus::prelude::*;
use dioxus_i18n::prelude::i18n;
use mt_note_fs_engine::{DirEntry, DirLister};
use ui::components::icons::FolderIcon;

use crate::frontend::i18n as i18n_support;

stylance::import_style!(styles, "save_dialog.module.scss");

/// 面包屑的一级。
struct Crumb {
    label: String,
    path: PathBuf,
}

/// 从路径算出各级面包屑（从根到该路径），反向构建避免绝对路径前缀问题。
fn breadcrumbs_of(path: &Path) -> Vec<Crumb> {
    let mut crumbs = Vec::new();
    let mut current = path.to_path_buf();
    loop {
        let name = current
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("/")
            .to_string();
        if name.is_empty() {
            break;
        }
        crumbs.push(Crumb {
            label: name,
            path: current.clone(),
        });
        match current.parent() {
            Some(parent) if parent != current => current = parent.to_path_buf(),
            _ => break,
        }
    }
    crumbs.reverse();
    crumbs
}

/// "另存为"模态对话框。
///
/// - `initial_dir`：打开时的初始目录（通常是源文件父目录）。
/// - `default_name`：默认文件名（不含扩展名）。
/// - `extension`：保存的扩展名（如 "pdf"），显示在文件名后。
/// - `on_save`：点击保存时回调，参数为完整目标路径（目录 + 输入名）。
/// - `on_cancel`：点击取消或遮罩时回调。
#[component]
pub fn SaveDialog(
    initial_dir: PathBuf,
    default_name: String,
    extension: String,
    on_save: EventHandler<PathBuf>,
    on_cancel: EventHandler<()>,
) -> Element {
    let i18n_ctx = i18n();
    let mut current_dir = use_signal(move || initial_dir.clone());
    let mut file_name = use_signal(move || default_name.clone());
    let entries = use_resource(move || {
        let dir = current_dir();
        async move {
            DirLister::new(&dir).list(false).unwrap_or_default()
        }
    });

    let entries_vec: Vec<DirEntry> = entries().unwrap_or_default();
    let dirs: Vec<&DirEntry> = entries_vec.iter().filter(|e| e.is_dir()).collect();
    let crumbs = breadcrumbs_of(&current_dir());
    let ext_upper = extension.to_uppercase();
    let default_filename = i18n_support::translate_with(
        &i18n_ctx,
        "editor-default-filename",
        &[("extension", extension.clone().into())],
    );

    rsx! {
        div {
            class: styles::save_dialog,
            onclick: move |_| on_cancel.call(()),

            div {
                class: styles::dialog,
                onclick: move |e: Event<MouseData>| e.stop_propagation(),

                div { class: styles::header, { i18n_support::translate_with(&i18n_ctx, "editor-save-as-ext", &[("ext", ext_upper.into())]) } }

                // 面包屑导航（可点回上级）
                div { class: styles::breadcrumb_bar,
                    for (i, crumb) in crumbs.iter().enumerate() {
                        span { class: styles::crumb_sep, "/" }
                        if i < crumbs.len() - 1 {
                            button {
                                class: styles::crumb_btn,
                                onclick: {
                                    let target = crumb.path.clone();
                                    move |_| current_dir.set(target.clone())
                                },
                                "{crumb.label}"
                            }
                        } else {
                            span { class: styles::crumb_current, "{crumb.label}" }
                        }
                    }
                }

                // 目录列表
                div { class: styles::dir_list,
                    if dirs.is_empty() {
                        div { class: styles::empty_hint, { i18n_support::translate(&i18n_ctx, "common-empty-dir") } }
                    }
                    for entry in dirs.iter() {
                        button {
                            class: styles::entry,
                            onclick: {
                                let target = entry.path().to_path_buf();
                                move |_| current_dir.set(target.clone())
                            },
                            FolderIcon { class: styles::entry_icon }
                            span { class: styles::entry_name, "{entry.name()}" }
                        }
                    }
                }

                // 文件名输入 + 扩展名
                div { class: styles::name_row,
                    input {
                        class: styles::name_input,
                        r#type: "text",
                        value: "{file_name}",
                        autofocus: true,
                        oninput: move |e| file_name.set(e.value()),
                    }
                    span { class: styles::ext_label, ".{extension}" }
                }

                // 操作按钮
                div { class: styles::action_row,
                    button {
                        class: "{styles::btn} {styles::btn_cancel}",
                        onclick: move |_| on_cancel.call(()),
                        { i18n_support::translate(&i18n_ctx, "common-cancel") }
                    }
                    button {
                        class: "{styles::btn} {styles::btn_save}",
                        onclick: move |_| {
                            let name = file_name();
                            let mut path = current_dir();
                            let file_part = if name.is_empty() {
                                default_filename.clone()
                            } else {
                                // 确保扩展名：若用户没加则补上
                                let dot_ext = format!(".{extension}");
                                if name.ends_with(&dot_ext) {
                                    name
                                } else {
                                    format!("{name}{dot_ext}")
                                }
                            };
                            path.push(file_part);
                            on_save.call(path);
                        },
                        { i18n_support::translate(&i18n_ctx, "common-save") }
                    }
                }
            }
        }
    }
}
