//! 文件浏览 tab。

use std::path::{Path, PathBuf};

use dioxus::prelude::*;
use dioxus_i18n::prelude::i18n;
use mt_note_fs_engine::DirEntry;
use ui::components::icons::{FolderIcon, StorageIcon};
use ui::hooks::use_app_state::use_app_state;
use ui::store::app_state::AppState;

use crate::frontend::hooks::use_top_app_bar_state::use_top_app_bar_state;
use crate::frontend::i18n as i18n_support;
use crate::frontend::js_bridge::{DioxusBridge, js_api};
use crate::frontend::store::top_app_bar_state::MenuItem;
use crate::frontend::views::home::components::new_item_dialog::{NewItemKind, NewItemTarget};
use crate::frontend::views::home::components::tab1::components::entry_item::EntryItem;

stylance::import_style!(styles, "tab1.module.scss");

pub mod components;

/// 文件浏览 tab 属性。
#[derive(Props, Clone, PartialEq)]
pub struct Tab1Props {
    pub entries: Signal<Vec<DirEntry>>,
    pub error_msg: Signal<Option<String>>,
    pub show_hidden: Signal<bool>,
    pub refresh_counter: Signal<u64>,
    pub ctx_menu_target: Signal<Option<PathBuf>>,
    pub new_item_target: Signal<Option<NewItemTarget>>,
    pub search_root: Option<PathBuf>,
    pub on_advanced_search: EventHandler<()>,
}

/// 文件浏览 tab。
///
/// 显示当前目录的文件列表，管理自己的 AppBar 菜单。
#[component]
pub fn Tab1(mut props: Tab1Props) -> Element {
    // i18n Context（Copy，可入菜单闭包；渲染期取词自动订阅语言变化）
    let i18n_ctx = i18n();
    let nav = navigator();
    let mut state = match use_app_state() {
        Ok(s) => s,
        Err(e) => {
            dioxus::logger::tracing::error!("Tab1: AppState 未初始化: {e}");
            return rsx! { { i18n_support::translate(&i18n_ctx, "common-app-state-error") } };
        }
    };
    let mut app_bar = match use_top_app_bar_state() {
        Ok(s) => s,
        Err(e) => {
            dioxus::logger::tracing::error!("Tab1: TopAppBarState 未初始化: {e}");
            return rsx! { { i18n_support::translate(&i18n_ctx, "common-app-state-error") } };
        }
    };

    {
        app_bar.write().reset();
        app_bar.write().set_title("Mt Note");
        let current_dir = state.read().current_dir().clone();
        let mut new_item_target = props.new_item_target;
        let items = vec![
            MenuItem::submenu(
                i18n_support::translate(&i18n_ctx, "common-new"),
                vec![
                    MenuItem::item(i18n_support::translate(&i18n_ctx, "common-new-file"), {
                        let dir = current_dir.clone();
                        move || {
                            new_item_target.set(Some(NewItemTarget {
                                dir: dir.clone(),
                                kind: NewItemKind::File,
                                in_favorites: false,
                            }));
                        }
                    }),
                    MenuItem::item(i18n_support::translate(&i18n_ctx, "common-new-dir"), {
                        let dir = current_dir.clone();
                        move || {
                            new_item_target.set(Some(NewItemTarget {
                                dir: dir.clone(),
                                kind: NewItemKind::Directory,
                                in_favorites: false,
                            }));
                        }
                    }),
                ],
            ),
            MenuItem::item(i18n_support::translate(&i18n_ctx, "common-refresh"), move || {
                props.refresh_counter.set((props.refresh_counter)() + 1);
            }),
            MenuItem::submenu(
                i18n_support::translate(&i18n_ctx, "common-more"),
                vec![
                    MenuItem::item(
                        i18n_support::translate(&i18n_ctx, "common-show-hidden"),
                        move || {
                            props.show_hidden.toggle();
                        },
                    ),
                    MenuItem::item(i18n_support::translate(&i18n_ctx, "common-search"), move || {
                        app_bar.write().enter_search();
                    }),
                    MenuItem::item(
                        i18n_support::translate(&i18n_ctx, "common-advanced-search"),
                        {
                            let on_search = props.on_advanced_search;
                            move || on_search.call(())
                        },
                    ),
                    MenuItem::item(i18n_support::translate(&i18n_ctx, "common-settings"), move || {
                        nav.push("/settings");
                    }),
                    MenuItem::item(i18n_support::translate(&i18n_ctx, "common-about"), move || {
                        nav.push("/about");
                    }),
                ],
            ),
        ];
        app_bar.write().set_menu_items(items);
    }

    let current = state.read().current_dir().clone();
    let breadcrumbs = build_breadcrumbs(&current);

    rsx! {
        if let Some(err) = (props.error_msg)() {
            div { class: styles::error_banner, "{err}" }
        }

        div { class: styles::breadcrumb_bar,
            button {
                class: styles::breadcrumb_root,
                onclick: {
                    let target = state
                        .read()
                        .browser_prefs()
                        .default_dir
                        .clone()
                        .map(PathBuf::from)
                        .unwrap_or_else(AppState::default_dir);
                    move |_| {
                        state.write().set_current_dir(target.clone());
                    }
                },
                StorageIcon {}
            }
            for (i , crumb) in breadcrumbs.iter().enumerate() {
                span { class: styles::breadcrumb_sep, "/" }
                if i < breadcrumbs.len() - 1 {
                    button {
                        class: styles::breadcrumb_btn,
                        onclick: {
                            let target = crumb.path.clone();
                            move |_| {
                                state.write().set_current_dir(target.clone());
                            }
                        },
                        "{crumb.label}"
                    }
                } else {
                    span { class: styles::breadcrumb_current, "{crumb.label}" }
                }
            }
        }

        if props.entries.read().is_empty() {
            div { class: styles::empty_state,
                FolderIcon { class: styles::empty_icon }
                span { class: styles::empty_text, { i18n_support::translate(&i18n_ctx, "common-empty-dir") } }
            }
        } else {
            div {
                class: styles::entry_list,
                id: "entry_list",
                onmounted: move |_| {
                    js_api::scroll::init_home_scroll::<DioxusBridge>();
                },
                for entry in props.entries.read().iter() {
                    {
                        let entry = entry.clone();
                        let subtitle = props
                            .search_root
                            .as_ref()
                            .and_then(|root| {
                                entry
                                    .path()
                                    .strip_prefix(root)
                                    .ok()
                                    .and_then(|p| {
                                        let s = p.to_string_lossy();
                                        if s.is_empty() { None } else { Some(s.to_string()) }
                                    })
                            });
                        rsx! {
                            EntryItem {
                                entry: entry.clone(),
                                subtitle,
                                on_dir_click: {
                                    move |path: PathBuf| {
                                        // 退出搜索模式
                                        app_bar.write().exit_search();
                                        state.write().set_current_dir(path);
                                    }
                                },
                                on_long_press: move |path: PathBuf| {
                                    props.ctx_menu_target.set(Some(path));
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

struct Breadcrumb {
    label: String,
    path: PathBuf,
}

fn build_breadcrumbs(path: &Path) -> Vec<Breadcrumb> {
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
        crumbs.push(Breadcrumb {
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
