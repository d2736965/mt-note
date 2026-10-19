use std::path::PathBuf;

use dioxus::logger::tracing;
use dioxus::prelude::*;
use dioxus_i18n::prelude::i18n;
use models::Favorites;
use mt_note_fs_engine::entry_display_name;

use crate::frontend::components::toast::{ToastItem, push_toast};
use crate::frontend::i18n as i18n_support;
use crate::frontend::js_bridge::{DioxusBridge, js_api};
use crate::frontend::store::storage::android::{AndroidBackend, AndroidStorage};

stylance::import_style!(styles, "ctx_menu.module.scss");

/// 长按菜单页：一级（操作列表）或「复制路径」二级页。
#[derive(Clone, Copy, PartialEq)]
pub enum CtxMenuPage {
    Main,
    CopyPath,
}

#[component]
pub fn CtxMenu(
    target: PathBuf,
    mut page: Signal<CtxMenuPage>,
    mut favorites: Signal<Favorites>,
    on_rename: EventHandler<PathBuf>,
    on_delete: EventHandler<PathBuf>,
    on_close: EventHandler<()>,
) -> Element {
    let toast = use_context::<Signal<Vec<ToastItem>>>();
    // 各 onclick 闭包独立持有副本，避免多闭包 move 同一值
    let target_name = entry_display_name(&target);
    let target_for_full = target.clone();
    let target_for_fav = target.clone();
    let target_for_rename = target.clone();
    let target_for_delete = target.clone();
    let is_dir = target.is_dir();
    // 文件所在目录的完整路径（根路径下无父目录则不提供该项）
    let parent_str = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_string_lossy().into_owned());
    let target_str = target.to_string_lossy().into_owned();
    let is_favorited = favorites.read().dirs.iter().any(|d| d == &target_str);

    // i18n Context（渲染期取词，语言切换重渲染即刷新）；toast 文案在
    // onclick 闭包内使用，渲染期捕获成局部副本供各闭包独立持有
    let i18n_ctx = i18n();
    let toast_copied_full = i18n_support::translate(&i18n_ctx, "ctx-copied-full");
    let toast_copied_name = i18n_support::translate(&i18n_ctx, "ctx-copied-name");
    let toast_copied_dir_path = i18n_support::translate(&i18n_ctx, "ctx-copied-dir-path");

    rsx! {
        div { class: styles::ctx_overlay, onclick: move |_| on_close.call(()),
            div {
                class: styles::ctx_sheet,
                onclick: move |e: Event<MouseData>| e.stop_propagation(),
                div { class: styles::ctx_title, "{target_name}" }
                match page() {
                    CtxMenuPage::Main => rsx! {
                        if is_dir {
                            if !is_favorited {
                                button {
                                    class: styles::ctx_item,
                                    onclick: move |_| {
                                        let path_str = target_for_fav.to_string_lossy().into_owned();
                                        let mut fav = favorites();
                                        if !path_str.is_empty() && !fav.dirs.contains(&path_str) {
                                            fav.dirs.push(path_str);
                                            favorites.set(fav);
                                            save_favorites(&favorites());
                                        }
                                        on_close.call(());
                                    },
                                    { i18n_support::translate(&i18n_ctx, "ctx-add-favorite") }
                                }
                            } else {
                                button {
                                    class: styles::ctx_item,
                                    onclick: move |_| {
                                        let path_str = target_for_fav.to_string_lossy().into_owned();
                                        let mut fav = favorites();
                                        fav.dirs.retain(|d| d != &path_str);
                                        favorites.set(fav);
                                        save_favorites(&favorites());
                                        on_close.call(());
                                    },
                                    { i18n_support::translate(&i18n_ctx, "ctx-remove-favorite") }
                                }
                            }
                        }
                        button {
                            class: stylance::classes!(styles::ctx_item, styles::has_submenu),
                            onclick: move |_| page.set(CtxMenuPage::CopyPath),
                            span { { i18n_support::translate(&i18n_ctx, "ctx-copy-path") } }
                            span { class: styles::arrow, "▶" }
                        }
                        button {
                            class: styles::ctx_item,
                            onclick: move |_| {
                                on_close.call(());
                                on_rename.call(target_for_rename.clone());
                            },
                            { i18n_support::translate(&i18n_ctx, "common-rename") }
                        }
                        button {
                            class: "{styles::ctx_item} {styles::ctx_danger}",
                            onclick: move |_| {
                                on_close.call(());
                                on_delete.call(target_for_delete.clone());
                            },
                            { i18n_support::translate(&i18n_ctx, "common-delete") }
                        }
                    },
                    CtxMenuPage::CopyPath => rsx! {
                        button {
                            class: stylance::classes!(styles::ctx_item, styles::ctx_back),
                            onclick: move |_| page.set(CtxMenuPage::Main),
                            { i18n_support::translate(&i18n_ctx, "common-back") }
                        }
                        button {
                            class: styles::ctx_item,
                            onclick: move |_| {
                                let text = target_for_full.to_string_lossy().into_owned();
                                js_api::editor::clipboard_write::<DioxusBridge>(&text);
                                push_toast(toast, toast_copied_full.clone());
                                on_close.call(());
                            },
                            { i18n_support::translate(&i18n_ctx, "ctx-full-path") }
                        }
                        if is_dir {
                            button {
                                class: styles::ctx_item,
                                onclick: move |_| {
                                    js_api::editor::clipboard_write::<DioxusBridge>(&target_name);
                                    push_toast(toast, toast_copied_name.clone());
                                    on_close.call(());
                                },
                                { i18n_support::translate(&i18n_ctx, "ctx-dir-name") }
                            }
                        } else {
                            if let Some(parent) = parent_str {
                                button {
                                    class: styles::ctx_item,
                                    onclick: move |_| {
                                        js_api::editor::clipboard_write::<DioxusBridge>(&parent);
                                        push_toast(toast, toast_copied_dir_path.clone());
                                        on_close.call(());
                                    },
                                    { i18n_support::translate(&i18n_ctx, "ctx-dir") }
                                }
                            }
                            button {
                                class: styles::ctx_item,
                                onclick: move |_| {
                                    js_api::editor::clipboard_write::<DioxusBridge>(&target_name);
                                    push_toast(toast, toast_copied_name.clone());
                                    on_close.call(());
                                },
                                { i18n_support::translate(&i18n_ctx, "ctx-file-name") }
                            }
                        }
                    },
                }
            }
        }
    }
}

fn save_favorites(fav: &Favorites) {
    if let Ok(backend) = AndroidBackend::new() {
        let storage = AndroidStorage::new(backend);
        if let Err(e) = storage.save(fav) {
            tracing::error!("保存收藏夹失败: {e:?}");
        }
    }
}
