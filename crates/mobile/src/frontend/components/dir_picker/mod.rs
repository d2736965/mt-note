use std::path::{Path, PathBuf};

use dioxus::prelude::*;
use dioxus_i18n::prelude::i18n;
use mt_note_fs_engine::{DirEntry, DirLister};
use ui::components::icons::FolderIcon;

use crate::frontend::i18n as i18n_support;

stylance::import_style!(styles, "dir_picker.module.scss");

struct Crumb {
    label: String,
    path: PathBuf,
}

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

#[component]
pub fn DirPicker(
    initial_dir: PathBuf,
    on_select: EventHandler<PathBuf>,
    on_cancel: EventHandler<()>,
) -> Element {
    // i18n Context（渲染期取词自动订阅语言变化）
    let i18n_ctx = i18n();
    let mut current_dir = use_signal(move || initial_dir.clone());
    let entries = use_resource(move || {
        let dir = current_dir();
        async move { DirLister::new(&dir).list(false).unwrap_or_default() }
    });

    let entries_vec: Vec<DirEntry> = entries().unwrap_or_default();
    let dirs: Vec<&DirEntry> = entries_vec.iter().filter(|e| e.is_dir()).collect();
    let crumbs = breadcrumbs_of(&current_dir());

    rsx! {
        div { class: styles::dir_picker, onclick: move |_| on_cancel.call(()),

            div {
                class: styles::dialog,
                onclick: move |e: Event<MouseData>| e.stop_propagation(),

                div { class: styles::header, { i18n_support::translate(&i18n_ctx, "dirpicker-title") } }

                div { class: styles::breadcrumb_bar,
                    span { class: styles::crumb_sep, "/" }
                    for (i , crumb) in crumbs.iter().enumerate() {
                        if crumb.label == "/" {
                            {}
                        } else if i < crumbs.len() - 1 {
                            button {
                                class: styles::crumb_btn,
                                onclick: {
                                    let target = crumb.path.clone();
                                    move |_| current_dir.set(target.clone())
                                },
                                "{crumb.label}"
                            }
                            span { class: styles::crumb_sep, "/" }
                        } else {
                            span { class: styles::crumb_current, "{crumb.label}" }
                        }
                    }
                }

                div { class: styles::dir_list,
                    if dirs.is_empty() {
                        div { class: styles::empty_hint, { i18n_support::translate(&i18n_ctx, "dirpicker-no-subdirs") } }
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

                div { class: styles::current_path,
                    span { class: styles::current_path_label, { i18n_support::translate(&i18n_ctx, "dirpicker-current") } }
                    span { class: styles::current_path_value, "{current_dir().display()}" }
                }

                div { class: styles::action_row,
                    button {
                        class: "{styles::btn} {styles::btn_cancel}",
                        onclick: move |_| on_cancel.call(()),
                        { i18n_support::translate(&i18n_ctx, "common-cancel") }
                    }
                    button {
                        class: "{styles::btn} {styles::btn_select}",
                        onclick: move |_| {
                            on_select.call(current_dir());
                        },
                        { i18n_support::translate(&i18n_ctx, "dirpicker-select") }
                    }
                }
            }
        }
    }
}