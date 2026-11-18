//! 备份恢复文件选择对话框。
//!
//! 浏览目录 + 点选 `.json` 备份文件。结构与「另存为」对话框（SaveDialog）
//! 同源：面包屑导航 + DirLister 列目录；区别是本对话框不输入文件名，
//! 点击文件即选中回调。

use std::path::{Path, PathBuf};

use dioxus::prelude::*;
use dioxus_i18n::prelude::i18n;
use mt_note_fs_engine::{DirEntry, DirLister};
use ui::components::icons::{FileIcon, FolderIcon};

use crate::frontend::i18n as i18n_support;

stylance::import_style!(styles, "file_dialog.module.scss");

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
            .unwrap_or("")
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

/// 备份恢复文件选择对话框。
///
/// - `initial_dir`：打开时的初始目录。
/// - `on_select`：点击 `.json` 文件时回调，参数为完整文件路径。
/// - `on_cancel`：点击取消或遮罩时回调。
#[component]
pub fn FileDialog(
    initial_dir: PathBuf,
    on_select: EventHandler<PathBuf>,
    on_cancel: EventHandler<()>,
) -> Element {
    let i18n_ctx = i18n();
    let mut current_dir = use_signal(move || initial_dir.clone());
    let entries = use_resource(move || {
        let dir = current_dir();
        async move { DirLister::new(&dir).list(false).unwrap_or_default() }
    });

    let entries_vec: Vec<DirEntry> = entries().unwrap_or_default();
    let dirs: Vec<&DirEntry> = entries_vec.iter().filter(|e| e.is_dir()).collect();
    let json_files: Vec<&DirEntry> = entries_vec
        .iter()
        .filter(|e| !e.is_dir() && e.name().to_lowercase().ends_with(".json"))
        .collect();
    let has_content = !dirs.is_empty() || !json_files.is_empty();
    let crumbs = breadcrumbs_of(&current_dir());

    rsx! {
        div {
            class: styles::file_dialog,
            onclick: move |_| on_cancel.call(()),

            div {
                class: styles::dialog,
                onclick: move |e: Event<MouseData>| e.stop_propagation(),

                div { class: styles::header, { i18n_support::translate(&i18n_ctx, "settings-restore-pick") } }

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

                // 目录 + .json 备份文件列表
                div { class: styles::entry_list,
                    if !has_content {
                        div { class: styles::empty_hint,
                            { i18n_support::translate(&i18n_ctx, "settings-restore-no-json") }
                        }
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
                    for entry in json_files.iter() {
                        button {
                            class: styles::entry,
                            onclick: {
                                let target = entry.path().to_path_buf();
                                move |_| on_select.call(target.clone())
                            },
                            FileIcon { class: styles::entry_icon }
                            span { class: styles::entry_name, "{entry.name()}" }
                        }
                    }
                }

                // 操作按钮
                div { class: styles::action_row,
                    button {
                        class: styles::btn_cancel,
                        onclick: move |_| on_cancel.call(()),
                        { i18n_support::translate(&i18n_ctx, "common-cancel") }
                    }
                }
            }
        }
    }
}
