//! 文件浏览器首页。
//!
//! 底部 NavBar 提供 3 个 tab：文件、收藏、更多。
//! 每个 tab 独立管理 AppBar 状态。

use std::path::PathBuf;
use std::sync::Arc;

use dioxus::logger::tracing;
use dioxus::prelude::*;
use dioxus_i18n::prelude::i18n;
use kameo::actor::ActorRef;
use models::Favorites;
use mt_note_fs_engine::{DirEntry, DirLister, create_dir, create_file, remove_path, rename_path};
use tokio::sync::Notify;
use ui::hooks::use_app_state::use_app_state;

use crate::frontend::file_watcher::{self, FsEventKind};
use crate::frontend::hooks::use_back_interceptor::use_back_interceptor;
use crate::frontend::hooks::use_bottom_bar_state::use_bottom_bar_state;
use crate::frontend::hooks::use_top_app_bar_state::use_top_app_bar_state;
use crate::frontend::i18n::Localize;
use crate::frontend::i18n as i18n_support;
use crate::frontend::js_bridge::{DioxusBridge, js_api};
use crate::frontend::store::bottom_bar_state::{NavBarConfig, NavBarItem, NavBarItems};
use crate::frontend::store::file_opener::{FileOpenerActor, OpenResult, ask_open, push_editor};
use crate::frontend::store::storage::android::{AndroidBackend, AndroidStorage};
use crate::frontend::views::home::components::ctx_menu::{CtxMenu, CtxMenuPage};
use crate::frontend::views::home::components::new_item_dialog::{
    NewItemDialog, NewItemKind, NewItemTarget,
};
use crate::frontend::views::home::components::search_dialog::SearchDialog;
use crate::frontend::views::home::components::tab1::Tab1;
use crate::frontend::views::home::components::tab2::Tab2;
use crate::frontend::views::home::components::tab3::Tab3;

stylance::import_style!(styles, "home.module.scss");

pub mod components;

#[derive(Clone, Copy, PartialEq)]
enum HomeTab {
    Files,
    Favorites,
    More,
}

#[component]
pub fn Home() -> Element {
    // i18n Context（Copy，可入 onclick 闭包；渲染期取词自动订阅语言变化）
    let i18n_ctx = i18n();
    let mut state = match use_app_state() {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Home: AppState 未初始化: {e}");
            return rsx! { { i18n_support::translate(&i18n_ctx, "common-app-state-error") } };
        }
    };
    let mut entries = use_signal(Vec::<DirEntry>::new);
    let mut error_msg = use_signal(|| None::<String>);
    let show_hidden = use_signal(|| false);
    let mut refresh_counter = use_signal(|| 0u64);
    let active_tab = use_memo(move || match state.read().active_tab() {
        1 => HomeTab::Favorites,
        2 => HomeTab::More,
        _ => HomeTab::Files,
    });
    let mut favorites = use_signal(Favorites::default);
    let fav_open_dir = use_memo(move || state.read().fav_open_dir().clone());
    let mut fav_entries = use_signal(Vec::<DirEntry>::new);
    let mut fav_refresh_counter = use_signal(|| 0u64);
    let mut ctx_menu_target = use_signal(|| None::<PathBuf>);
    let mut ctx_menu_page = use_signal(|| CtxMenuPage::Main);
    let mut rename_target = use_signal(|| None::<PathBuf>);
    let mut rename_value = use_signal(String::new);
    let mut new_item_target = use_signal(|| None::<NewItemTarget>);
    let mut new_item_value = use_signal(String::new);
    let mut new_item_error = use_signal(|| None::<String>);

    let has_overlay = ctx_menu_target().is_some()
        || rename_target().is_some()
        || new_item_target().is_some()
        || *state.read().show_search_dialog();
    let can_navigate_up = active_tab() == HomeTab::Files
        && state
            .read()
            .current_dir()
            .parent()
            .filter(|p| *p != *state.read().current_dir())
            .filter(|p| DirLister::new(p).list(false).is_ok())
            .is_some();
    let can_fav_navigate_up = active_tab() == HomeTab::Favorites && fav_open_dir().is_some();
    use_back_interceptor(
        has_overlay || can_navigate_up || can_fav_navigate_up,
        move || {
            if *state.read().show_search_dialog() {
                state.write().set_show_search_dialog(false);
                state.write().set_search_params(None);
            } else if rename_target().is_some() {
                rename_target.set(None);
            } else if new_item_target().is_some() {
                new_item_target.set(None);
                new_item_value.set(String::new());
                new_item_error.set(None);
            } else if ctx_menu_target().is_some() {
                if ctx_menu_page() == CtxMenuPage::CopyPath {
                    ctx_menu_page.set(CtxMenuPage::Main);
                } else {
                    ctx_menu_target.set(None);
                }
            } else if active_tab() == HomeTab::Favorites
                && let Some(current) = fav_open_dir().as_ref().cloned()
            {
                // 收藏夹目录浏览：先尝试退到父目录，已在收藏根目录时才退到入口
                let root = state.read().fav_root_dir().clone();
                if root.as_ref() == Some(&current) {
                    // 已在收藏根目录，退到入口
                    state.write().set_fav_open_dir(None);
                    state.write().set_fav_root_dir(None);
                } else {
                    // 退到父目录
                    let parent = current
                        .parent()
                        .filter(|p| *p != current)
                        .map(PathBuf::from);
                    match parent {
                        Some(p) => {
                            state.write().set_fav_open_dir(Some(p));
                        }
                        None => {
                            // 无父目录（不应发生），退到入口
                            state.write().set_fav_open_dir(None);
                            state.write().set_fav_root_dir(None);
                        }
                    }
                }
            } else if active_tab() == HomeTab::Files {
                let parent = state
                    .read()
                    .current_dir()
                    .parent()
                    .filter(|p| *p != *state.read().current_dir())
                    .filter(|p| DirLister::new(p).list(false).is_ok())
                    .map(PathBuf::from);
                if let Some(parent) = parent {
                    state.write().set_current_dir(parent);
                }
            }
        },
    );

    let mut scroll_restored = use_signal(|| false);
    let mut prev_dir = use_signal(|| state.read().current_dir().clone());

    // 监听当前目录的文件系统变化，自动刷新列表
    use_effect(move || {
        let dir = state.read().current_dir().clone();
        spawn(async move {
            file_watcher::watch_dir(&dir).await;
        });
    });

    use_effect(move || {
        let dir = state.read().current_dir().clone();
        spawn(async move {
            let rx = file_watcher::subscribe().await;
            if let Some(mut rx) = rx {
                while rx.changed().await.is_ok() {
                    let event = rx.borrow().clone();
                    tracing::info!(
                        "home: received event {:?} {}",
                        event.kind,
                        event.path.display()
                    );
                    let is_current_dir = event.path == dir || event.path.starts_with(&dir);
                    if is_current_dir && event.kind == FsEventKind::DirChanged {
                        refresh_counter.set(refresh_counter() + 1);
                    }
                }
            } else {
                tracing::error!("home: subscribe failed");
            }
        });
    });

    // 监听收藏夹目录的文件系统变化，自动刷新
    use_effect(move || {
        if let Some(dir) = fav_open_dir() {
            spawn(async move {
                file_watcher::watch_dir(&dir).await;
            });
        }
    });

    use_effect(move || {
        if let Some(dir) = fav_open_dir() {
            spawn(async move {
                let rx = file_watcher::subscribe().await;
                if let Some(mut rx) = rx {
                    while rx.changed().await.is_ok() {
                        let event = rx.borrow().clone();
                        let is_current_dir = event.path == dir || event.path.starts_with(&dir);
                        if is_current_dir && event.kind == FsEventKind::DirChanged {
                            fav_refresh_counter.set(fav_refresh_counter() + 1);
                        }
                    }
                }
            });
        }
    });

    use_effect(move || {
        refresh_counter();
        let dir = state.read().current_dir().clone();
        match DirLister::new(&dir).list(show_hidden()) {
            Ok(list) => {
                entries.set(list);
                error_msg.set(None);
            }
            Err(e) => {
                error_msg.set(Some(e.localize(&i18n_ctx)));
            }
        }
    });

    // 目录切换时重置滚动位置，避免旧目录的 scrollTop 被复用
    use_effect(move || {
        let current = state.read().current_dir().clone();
        if prev_dir() != current {
            prev_dir.set(current);
            if scroll_restored() {
                js_api::scroll::reset_home_scroll::<DioxusBridge>();
            }
        }
    });

    use_effect(move || {
        if !scroll_restored() && !entries.read().is_empty() {
            scroll_restored.set(true);
            spawn(async move {
                let target = js_api::scroll::get_home_scroll_top::<DioxusBridge>().await;
                if target > 0.0 {
                    js_api::scroll::set_home_scroll_top::<DioxusBridge>(target);
                }
            });
        }
    });

    let mut favorites_loaded = use_signal(|| false);
    use_effect(move || {
        if !favorites_loaded() {
            if let Ok(backend) = AndroidBackend::new() {
                let storage = AndroidStorage::new(backend);
                if let Ok(Some(fav)) = storage.load::<Favorites>() {
                    favorites.set(fav);
                }
            }
            favorites_loaded.set(true);
        }
    });

    use_effect(move || {
        let dir = fav_open_dir();
        fav_refresh_counter();
        if let Some(dir) = dir {
            match DirLister::new(&dir).list(show_hidden()) {
                Ok(list) => {
                    fav_entries.set(list);
                }
                Err(e) => {
                    tracing::error!("收藏夹目录列表失败: {e}");
                    fav_entries.set(Vec::new());
                }
            }
        }
    });

    // 菜单每次打开都从一级页开始（上次可能停在二级复制路径页）。
    // 只读 target 建立依赖——读 page 会把「进二级页」也当触发条件，
    // effect 立即把 page 打回 Main，二级页永远开不出来。
    use_effect(move || {
        if ctx_menu_target().is_some() {
            ctx_menu_page.set(CtxMenuPage::Main);
        }
    });

    let mut search_entries = use_signal(Vec::<DirEntry>::new);
    let mut cancel_token = use_signal(|| None::<Arc<Notify>>);
    let app_bar = use_top_app_bar_state().ok();
    use_effect(move || {
        if let Some(token) = cancel_token.take() {
            token.notify_waiters();
        }

        let is_searching = app_bar.as_ref().is_some_and(|a| *a.read().search_mode());
        let query = app_bar
            .as_ref()
            .map(|a| a.read().search_query().clone())
            .unwrap_or_default();
        refresh_counter();
        fav_refresh_counter();
        show_hidden();
        favorites();
        fav_open_dir();

        if !is_searching || query.is_empty() {
            search_entries.set(Vec::new());
        } else {
            let tab = active_tab();
            let mut search_entries = search_entries;
            let state = state;
            let sh = show_hidden();
            let favs = favorites();
            let fav_open_dir = fav_open_dir;

            let token = Arc::new(Notify::new());
            cancel_token.set(Some(token.clone()));

            spawn(async move {
                tokio::select! {
                    _result = async {
                        let result = match tab {
                            HomeTab::Files => {
                                let dir = state.read().current_dir().clone();
                                DirLister::new(&dir).search(&query, sh).await
                            }
                            HomeTab::Favorites => {
                                let fav_dir = fav_open_dir();
                                if let Some(dir) = fav_dir {
                                    DirLister::new(&dir).search(&query, sh).await
                                } else {
                                    let mut tasks = tokio::task::JoinSet::new();
                                    for dir_str in &favs.dirs {
                                        let dir = dir_str.clone();
                                        let query = query.clone();
                                        tasks.spawn(async move {
                                            DirLister::new(&dir).search(&query, sh).await
                                        });
                                    }
                                    let mut all = Vec::new();
                                    while let Some(task_result) = tasks.join_next().await {
                                        match task_result {
                                            Ok(Ok(list)) => all.extend(list),
                                            Ok(Err(e)) => {
                                                tracing::error!("搜索收藏目录失败: {e}")
                                            }
                                            Err(e) => {
                                                tracing::error!("join error: {e}")
                                            }
                                        }
                                    }
                                    all.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
                                    Ok(all)
                                }
                            }
                            HomeTab::More => Ok(Vec::new()),
                        };
                        match result {
                            Ok(list) => search_entries.set(list),
                            Err(e) => tracing::error!("搜索失败: {e}"),
                        }
                    } => {},
                    _ = token.notified() => {},
                }
            });
        }
    });

    let is_searching = use_memo(move || {
        app_bar
            .as_ref()
            .is_some_and(|a| !a.read().search_query().is_empty() && *a.read().search_mode())
    });

    if let Ok(mut bottom_bar) = use_bottom_bar_state() {
        let tab = active_tab();
        let items = NavBarItems::Three([
            NavBarItem {
                icon: ui::components::icons::Icon::Folder,
                label: i18n_support::translate(&i18n_ctx, "home-tab-files"),
                active: tab == HomeTab::Files,
                on_click: std::rc::Rc::new(std::cell::RefCell::new({
                    let mut state = state;
                    move || {
                        state.write().set_active_tab(0);
                    }
                })),
            },
            NavBarItem {
                icon: ui::components::icons::Icon::Star,
                label: i18n_support::translate(&i18n_ctx, "home-tab-favorites"),
                active: tab == HomeTab::Favorites,
                on_click: std::rc::Rc::new(std::cell::RefCell::new({
                    let mut state = state;
                    move || {
                        state.write().set_active_tab(1);
                    }
                })),
            },
            NavBarItem {
                icon: ui::components::icons::Icon::MoreVert,
                label: i18n_support::translate(&i18n_ctx, "home-tab-more"),
                active: tab == HomeTab::More,
                on_click: std::rc::Rc::new(std::cell::RefCell::new({
                    let mut state = state;
                    move || {
                        state.write().set_active_tab(2);
                    }
                })),
            },
        ]);
        bottom_bar.write().set_nav_bar(NavBarConfig { items });
    }

    let search_root = state.read().current_dir().clone();
    let fav_open_dir_for_root = fav_open_dir();
    let search_root = if let Some(ref dir) = fav_open_dir_for_root {
        match active_tab() {
            HomeTab::Favorites => dir.clone(),
            _ => search_root.clone(),
        }
    } else {
        search_root.clone()
    };
    let saved_search_params = state.read().search_params().clone();

    rsx! {
        div {
            id: "home_view",
            class: styles::home_view,
            onmounted: move |_| {
                js_api::scroll::init_home_scroll::<DioxusBridge>();
            },

            match active_tab() {
                HomeTab::Files => rsx! {
                    Tab1 {
                        entries: if is_searching() { search_entries } else { entries },
                        error_msg,
                        show_hidden,
                        refresh_counter,
                        ctx_menu_target,
                        new_item_target,
                        search_root: if is_searching() { Some(state.read().current_dir().clone()) } else { None },
                        on_advanced_search: move |_| {
                            state.write().set_show_search_dialog(true);
                        },
                    }
                },
                HomeTab::Favorites => rsx! {
                    Tab2 {
                        favorites,
                        fav_open_dir,
                        fav_entries: if is_searching() { search_entries } else { fav_entries },
                        show_hidden,
                        fav_refresh_counter,
                        ctx_menu_target,
                        new_item_target,
                        on_open_dir: move |dir: Option<PathBuf>| {
                            match &dir {
                                Some(d) => {
                                    // 从入口进入时记录收藏根目录
                                    if state.read().fav_root_dir().is_none() {
                                        state.write().set_fav_root_dir(Some(d.clone()));
                                    }
                                    state.write().set_fav_open_dir(dir);
                                }
                                None => {
                                    // 退到入口，清除根目录记录
                                    state.write().set_fav_open_dir(None);
                                    state.write().set_fav_root_dir(None);
                                }
                            }
                        },
                        search_root: if is_searching() { fav_open_dir() } else { None },
                        on_advanced_search: move |_| {
                            state.write().set_show_search_dialog(true);
                        },
                    }
                },
                HomeTab::More => rsx! {
                    Tab3 {}
                },
            }

            // 长按上下文菜单
            if let Some(target) = ctx_menu_target() {
                {
                    rsx! {
                        CtxMenu {
                            target,
                            page: ctx_menu_page,
                            favorites,
                            on_rename: move |t: PathBuf| {
                                rename_value.set(
                                    t.file_name()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or("")
                                        .to_string(),
                                );
                                rename_target.set(Some(t));
                            },
                            on_delete: move |t: PathBuf| {
                                match remove_path(&t) {
                                    Ok(()) => {
                                        if active_tab() == HomeTab::Favorites {
                                            fav_refresh_counter.set(fav_refresh_counter() + 1);
                                        } else {
                                            refresh_counter.set(refresh_counter() + 1);
                                        }
                                    }
                                    Err(e) => {
                                        error_msg.set(Some(i18n_support::translate_err(
                                            &i18n_ctx,
                                            "err-delete",
                                            &e.localize(&i18n_ctx),
                                        )));
                                    }
                                }
                            },
                            on_close: move |_| ctx_menu_target.set(None),
                        }
                    }
                }
            }

            // 重命名对话框
            if let Some(target) = rename_target() {
                {
                    let parent = target.parent().map(|p| p.to_path_buf()).unwrap_or_default();
                    rsx! {
                        div { class: styles::input_dialog, onclick: move |_| rename_target.set(None),
                            div {
                                class: styles::dialog_card,
                                onclick: move |e: Event<MouseData>| e.stop_propagation(),
                                div { class: styles::dialog_title,
                                    { i18n_support::translate(&i18n_ctx, "common-rename") }
                                }
                                input {
                                    class: styles::dialog_input,
                                    r#type: "text",
                                    value: "{rename_value}",
                                    autofocus: true,
                                    oninput: move |e| rename_value.set(e.value()),
                                }
                                div { class: styles::dialog_actions,
                                    button {
                                        class: "{styles::btn} {styles::btn_cancel}",
                                        onclick: move |_| rename_target.set(None),
                                        { i18n_support::translate(&i18n_ctx, "common-cancel") }
                                    }
                                    button {
                                        class: "{styles::btn} {styles::btn_ok}",
                                        onclick: move |_| {
                                            let new_name_val = rename_value();
                                            if !new_name_val.is_empty() {
                                                let to = parent.join(&new_name_val);
                                                match rename_path(&target, &to) {
                                                    Ok(()) => {
                                                        if active_tab() == HomeTab::Favorites {
                                                            fav_refresh_counter.set(fav_refresh_counter() + 1);
                                                        } else {
                                                            refresh_counter.set(refresh_counter() + 1);
                                                        }
                                                    }
                                                    Err(e) => {
                                                        error_msg.set(Some(
                                                            i18n_support::translate_err(
                                                                &i18n_ctx,
                                                                "err-rename",
                                                                &e.localize(&i18n_ctx),
                                                            ),
                                                        ));
                                                    }
                                                }
                                            }
                                            rename_target.set(None);
                                        },
                                        { i18n_support::translate(&i18n_ctx, "common-confirm") }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 新建文件/目录对话框
            if let Some(target) = new_item_target() {
                {
                    let target_for_dialog = target.clone();
                    let target_for_confirm = target.clone();
                    let mut new_item_target = new_item_target;
                    let mut new_item_value = new_item_value;
                    let mut new_item_error = new_item_error;
                    rsx! {
                        NewItemDialog {
                            target: target_for_dialog,
                            value: new_item_value,
                            error: new_item_error,
                            on_input: move |v: String| {
                                new_item_value.set(v);
                                new_item_error.set(None);
                            },
                            on_confirm: move |_| {
                                let name = new_item_value();
                                if name.is_empty() {
                                    new_item_error.set(Some(i18n_support::translate(
                                        &i18n_ctx,
                                        "err-name-empty",
                                    )));
                                } else {
                                    let path = target_for_confirm.dir.join(&name);
                                    let result = match target_for_confirm.kind {
                                        NewItemKind::File => create_file(&path),
                                        NewItemKind::Directory => create_dir(&path),
                                    };
                                    match result {
                                        Ok(()) => {
                                            if target_for_confirm.in_favorites {
                                                fav_refresh_counter.set(fav_refresh_counter() + 1);
                                            } else {
                                                refresh_counter.set(refresh_counter() + 1);
                                            }
                                            new_item_target.set(None);
                                            new_item_value.set(String::new());
                                            new_item_error.set(None);
                                        }
                                        Err(e) => {
                                            new_item_error.set(Some(i18n_support::translate_err(
                                                &i18n_ctx,
                                                "err-create",
                                                &e.localize(&i18n_ctx),
                                            )));
                                        }
                                    }
                                }
                            },
                            on_cancel: move |_| {
                                new_item_target.set(None);
                                new_item_value.set(String::new());
                                new_item_error.set(None);
                            },
                        }
                    }
                }
            }

            // 高级搜索对话框
            if *state.read().show_search_dialog() {
                SearchDialog {
                    root: search_root.clone(),
                    on_close: move |_| {
                        state.write().set_show_search_dialog(false);
                        state.write().set_search_params(None);
                    },
                    on_open_file: {
                        let actor_ctx = use_context::<Signal<Option<ActorRef<FileOpenerActor>>>>();
                        move |path: PathBuf| {
                            // 在 spawn 之前同步保存滚动位置
                            js_api::scroll::save_home_scroll::<DioxusBridge>();
                            spawn(async move {
                                if let Some(OpenResult::Editor(p)) = ask_open(&actor_ctx, path).await {
                                    push_editor(&p);
                                }
                            });
                        }
                    },
                    auto_search: saved_search_params.is_some(),
                    search_params: saved_search_params.clone(),
                    on_save_search_params: {
                        let mut app_state = state;
                        move |(opts, root): (mt_note_fs_engine::SearchOptions, PathBuf)| {
                            let params = ui::store::app_state::SearchParams {
                                query: opts.query,
                                is_regex: opts.is_regex,
                                case_sensitive: opts.case_sensitive,
                                search_content: opts.search_content,
                                only_first_match: opts.only_first_match,
                                show_hidden: opts.show_hidden,
                                root,
                            };
                            app_state.write().set_search_params(Some(params));
                        }
                    },
                }
            }
        }
    }
}
