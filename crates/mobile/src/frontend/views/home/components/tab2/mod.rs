//! 收藏夹 tab。

use std::path::{Path, PathBuf};

use dioxus::prelude::*;
use dioxus_i18n::prelude::i18n;
use models::Favorites;
use mt_note_fs_engine::DirEntry;
use ui::components::icons::{FolderIcon, StarIcon};

use crate::frontend::hooks::use_top_app_bar_state::use_top_app_bar_state;
use crate::frontend::i18n as i18n_support;
use crate::frontend::js_bridge::{DioxusBridge, js_api};
use crate::frontend::store::storage::android::{AndroidBackend, AndroidStorage};
use crate::frontend::store::top_app_bar_state::MenuItem;
use crate::frontend::views::home::components::new_item_dialog::{NewItemKind, NewItemTarget};
use crate::frontend::views::home::components::tab1::components::entry_item::EntryItem;

stylance::import_style!(styles, "tab2.module.scss");

/// 收藏夹 tab 属性。
#[derive(Props, Clone, PartialEq)]
pub struct Tab2Props {
    pub favorites: Signal<Favorites>,
    pub fav_open_dir: ReadSignal<Option<PathBuf>>,
    pub fav_entries: Signal<Vec<DirEntry>>,
    pub show_hidden: Signal<bool>,
    pub fav_refresh_counter: Signal<u64>,
    pub ctx_menu_target: Signal<Option<PathBuf>>,
    pub new_item_target: Signal<Option<NewItemTarget>>,
    pub on_open_dir: EventHandler<Option<PathBuf>>,
    pub search_root: Option<PathBuf>,
    pub on_advanced_search: EventHandler<()>,
}

/// 收藏夹 tab。
///
/// 顶层显示收藏目录列表，点击后进入目录浏览模式（与 tab1 一致的面包屑+文件列表）。
/// 独立管理 AppBar 菜单。
#[component]
pub fn Tab2(props: Tab2Props) -> Element {
    // i18n Context（Copy，可入菜单闭包；渲染期取词自动订阅语言变化）
    let i18n_ctx = i18n();
    let mut favorites = props.favorites;
    let fav_open_dir = props.fav_open_dir;
    let fav_entries = props.fav_entries;
    let on_open_dir = props.on_open_dir;

    if let Some(open_dir) = fav_open_dir() {
        return rsx! {
            FavDirBrowser {
                dir: open_dir,
                entries: fav_entries,
                show_hidden: props.show_hidden,
                fav_refresh_counter: props.fav_refresh_counter,
                on_open_dir,
                ctx_menu_target: props.ctx_menu_target,
                new_item_target: props.new_item_target,
                search_root: props.search_root,
                on_advanced_search: props.on_advanced_search,
            }
        };
    }

    if let Ok(mut app_bar) = use_top_app_bar_state() {
        app_bar.write().reset();
        app_bar.write().set_title(i18n_support::translate(&i18n_ctx, "home-favorites"));
        app_bar
            .write()
            .set_menu_items(vec![MenuItem::item(
                i18n_support::translate(&i18n_ctx, "common-refresh"),
                move || {
                    if let Ok(backend) = AndroidBackend::new() {
                        let storage = AndroidStorage::new(backend);
                        if let Ok(Some(fav)) = storage.load::<Favorites>() {
                            favorites.set(fav);
                        }
                    }
                },
            )]);
    }

    let fav = favorites();
    if fav.dirs.is_empty() {
        return rsx! {
            div { class: styles::empty_state,
                StarIcon { class: styles::empty_icon }
                span { class: styles::empty_text, { i18n_support::translate(&i18n_ctx, "home-no-favorites") } }
                span { class: styles::empty_hint, { i18n_support::translate(&i18n_ctx, "home-no-favorites-hint") } }
            }
        };
    }

    rsx! {
        div { class: styles::fav_list,
            for dir_str in fav.dirs.iter() {
                {
                    let dir_path = PathBuf::from(dir_str);
                    let dir_name = dir_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(dir_str)
                        .to_string();
                    let dir_path_click = dir_path.clone();
                    let dir_str_remove = dir_str.clone();
                    rsx! {
                        div { class: styles::fav_item,
                            button {
                                class: styles::fav_item_content,
                                onclick: {
                                    move |_| {
                                        on_open_dir.call(Some(dir_path_click.clone()));
                                    }
                                },
                                FolderIcon { class: styles::fav_item_icon }
                                div { class: styles::fav_item_info,
                                    span { class: styles::fav_item_name, "{dir_name}" }
                                    span { class: styles::fav_item_path, "{dir_str}" }
                                }
                            }
                            button {
                                class: styles::fav_remove_btn,
                                onclick: {
                                    move |_| {
                                        let mut fav = favorites();
                                        fav.dirs.retain(|d| d != &dir_str_remove);
                                        favorites.set(fav);
                                        save_favorites(&favorites());
                                    }
                                },
                                ui::components::icons::CloseIcon {}
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 收藏夹目录浏览属性。
#[derive(Props, Clone, PartialEq)]
pub struct FavDirBrowserProps {
    pub dir: PathBuf,
    pub entries: Signal<Vec<DirEntry>>,
    pub show_hidden: Signal<bool>,
    pub fav_refresh_counter: Signal<u64>,
    pub on_open_dir: EventHandler<Option<PathBuf>>,
    pub ctx_menu_target: Signal<Option<PathBuf>>,
    pub new_item_target: Signal<Option<NewItemTarget>>,
    pub search_root: Option<PathBuf>,
    pub on_advanced_search: EventHandler<()>,
}

/// 收藏夹目录浏览。
///
/// 与 tab1 一致的面包屑导航 + 文件列表 + TopAppBar（标题、菜单、尾随按钮）。
#[component]
pub fn FavDirBrowser(mut props: FavDirBrowserProps) -> Element {
    // i18n Context（Copy，可入菜单闭包；渲染期取词自动订阅语言变化）
    let i18n_ctx = i18n();
    let nav = navigator();
    let on_open_dir = props.on_open_dir;
    let breadcrumbs = build_breadcrumbs(&props.dir);
    let mut app_bar = match use_top_app_bar_state() {
        Ok(s) => s,
        Err(e) => {
            dioxus::logger::tracing::error!("FavDirBrowser: TopAppBarState 未初始化: {e}");
            return rsx! { { i18n_support::translate(&i18n_ctx, "common-app-state-error") } };
        }
    };

    {
        app_bar.write().reset();
        app_bar.write().set_title("Mt Note");
        let current_dir = props.dir.clone();
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
                                in_favorites: true,
                            }));
                        }
                    }),
                    MenuItem::item(i18n_support::translate(&i18n_ctx, "common-new-dir"), {
                        let dir = current_dir.clone();
                        move || {
                            new_item_target.set(Some(NewItemTarget {
                                dir: dir.clone(),
                                kind: NewItemKind::Directory,
                                in_favorites: true,
                            }));
                        }
                    }),
                ],
            ),
            MenuItem::item(i18n_support::translate(&i18n_ctx, "common-refresh"), move || {
                props
                    .fav_refresh_counter
                    .set((props.fav_refresh_counter)() + 1);
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
                    MenuItem::item(i18n_support::translate(&i18n_ctx, "common-settings"), {
                        move || {
                            nav.push("/settings");
                        }
                    }),
                    MenuItem::item(i18n_support::translate(&i18n_ctx, "common-about"), {
                        move || {
                            nav.push("/about");
                        }
                    }),
                ],
            ),
        ];
        app_bar.write().set_menu_items(items);
        app_bar.write().set_trailing_buttons(vec![]);
    }

    rsx! {
        div { class: styles::breadcrumb_bar,
            button {
                class: styles::breadcrumb_root,
                onclick: move |_| {
                    on_open_dir.call(None);
                },
                StarIcon {}
            }
            for (i , crumb) in breadcrumbs.iter().enumerate() {
                span { class: styles::breadcrumb_sep, "/" }
                if i < breadcrumbs.len() - 1 {
                    button {
                        class: styles::breadcrumb_btn,
                        onclick: {
                            let target = crumb.path.clone();
                            move |_| {
                                on_open_dir.call(Some(target.clone()));
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
                                        on_open_dir.call(Some(path));
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

fn save_favorites(fav: &Favorites) {
    if let Ok(backend) = AndroidBackend::new() {
        let storage = AndroidStorage::new(backend);
        if let Err(e) = storage.save(fav) {
            dioxus::logger::tracing::error!("保存收藏夹失败: {e:?}");
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
