use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use dioxus::logger::tracing;
use dioxus::prelude::*;
use dioxus_i18n::prelude::i18n;
use kameo::actor::ActorRef;
use kameo::prelude::*;
use mt_note_fs_engine::{DirEntry, DirLister, SearchOptions, SearchResult};
use ui::components::icons::FolderIcon;
use ui::components::overlay::Overlay;

use self::components::search_result_item::SearchResultItem;

use crate::frontend::hooks::use_back_interceptor::use_back_interceptor;
use crate::frontend::i18n::Localize;
use crate::frontend::i18n as i18n_support;
use crate::frontend::views::home::components::tab1::components::entry_item::EntryItem;

pub mod components;

stylance::import_style!(styles, "search_dialog.module.scss");

/// 搜索结果类型过滤器。
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum SearchResultType {
    Directory,
    File,
}

/// SearchActor：管理搜索状态的 kameo Actor。
#[derive(Actor, Default)]
#[actor(name = "search_actor")]
pub struct SearchActor {
    results: Vec<SearchResult>,
    searching: bool,
    error: Option<String>,
    cancelled: Arc<AtomicBool>,
}

/// 搜索状态快照。
#[derive(Clone, Debug, kameo::Reply)]
pub struct SearchState {
    pub results: Vec<SearchResult>,
    pub searching: bool,
    pub error: Option<String>,
}

/// SearchActor 消息定义。
#[messages]
impl SearchActor {
    /// 启动搜索。
    #[message(ctx)]
    async fn do_search(&mut self, opts: SearchOptions, root: PathBuf, ctx: &mut Context<Self, ()>) {
        self.results.clear();
        self.searching = true;
        self.error = None;
        self.cancelled = Arc::new(AtomicBool::new(false));

        let cancel_flag = self.cancelled.clone();
        let self_ref = ctx.actor_ref().clone();

        // 执行搜索任务
        tokio::task::spawn(async move {
            let lister = DirLister::new(&root);
            let actor_ref = self_ref.clone();
            let cancel_flag_clone = cancel_flag.clone();
            let on_result = move |r: SearchResult| -> bool {
                // 检查是否已取消
                if cancel_flag_clone.load(Ordering::Relaxed) {
                    return false;
                }

                let actor_ref = actor_ref.clone();
                tokio::spawn(async move {
                    if let Err(e) = actor_ref.tell(AddResult { result: r }).await {
                        tracing::error!("Failed to send AddResult message: {:?}", e);
                    }
                });
                true
            };

            if let Err(e) = lister.search_advanced_streaming(opts, on_result).await {
                // actor 侧无 i18n context，只传原始错误；UI 渲染层负责包装翻译
                if let Err(e) = self_ref
                    .tell(SetError {
                        msg: e.to_string(),
                    })
                    .await
                {
                    tracing::error!("Failed to send SetError message: {:?}", e);
                }
            } else if let Err(e) = self_ref.tell(SearchDone).await {
                tracing::error!("Failed to send SearchDone message: {:?}", e);
            }
        });
    }

    /// 追加搜索结果。
    #[message]
    fn add_result(&mut self, result: SearchResult) {
        self.results.push(result);
    }

    /// 设置错误。
    #[message]
    fn set_error(&mut self, msg: String) {
        self.error = Some(msg);
        self.searching = false;
    }

    /// 标记搜索完成。
    #[message]
    fn search_done(&mut self) {
        self.searching = false;
    }

    /// 获取当前状态。
    #[message]
    fn get_state(&self) -> SearchState {
        SearchState {
            results: self.results.clone(),
            searching: self.searching,
            error: self.error.clone(),
        }
    }

    /// 取消搜索。
    #[message]
    fn cancel(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
        self.searching = false;
    }
}

// 消息结构体（由 #[messages] 宏自动生成）：
// - DoSearch { opts, root }
// - AddResult(SearchResult)
// - SetError(String)
// - SearchDone
// - GetState
// - Cancel

/// 在 App 层提供 SearchActor 的引用容器。
///
/// Actor 生命周期独立于 SearchDialog 组件：跳转编辑器时组件卸载，
/// 但 Actor 继续在后台搜索。只有关闭对话框时才清除 Actor。
/// 必须在 App（layout 组件，跨路由不卸载）中调用。
pub fn provide_search_actor() -> Signal<Option<ActorRef<SearchActor>>> {
    let actor: Signal<Option<ActorRef<SearchActor>>> = Signal::new(None);
    use_context_provider(|| actor)
}

/// 搜索对话框属性。
#[derive(Props, Clone, PartialEq)]
pub struct SearchDialogProps {
    /// 搜索根目录。
    pub root: PathBuf,
    /// 关闭对话框回调。
    pub on_close: EventHandler<()>,
    /// 打开文件回调。
    pub on_open_file: EventHandler<PathBuf>,
    /// 是否自动重新搜索（从编辑器返回时）。
    pub auto_search: bool,
    /// 上次搜索参数（从编辑器返回时恢复）。
    pub search_params: Option<ui::store::app_state::SearchParams>,
    /// 保存搜索参数回调。
    pub on_save_search_params: EventHandler<(SearchOptions, PathBuf)>,
}

/// 高级搜索对话框。
#[component]
pub fn SearchDialog(props: SearchDialogProps) -> Element {
    // i18n Context（渲染期取词自动订阅语言变化）
    let i18n_ctx = i18n();
    let mut query = use_signal(String::new);
    let mut is_regex = use_signal(|| false);
    let mut case_sensitive = use_signal(|| false);
    let mut search_content = use_signal(|| false);
    let mut only_first = use_signal(|| false);
    let mut show_hidden = use_signal(|| false);
    let mut history: Signal<Vec<String>> = use_signal(Vec::new);
    let results: Signal<Vec<SearchResult>> = use_signal(Vec::new);
    let searching = use_signal(|| false);
    let error_msg = use_signal(|| None::<String>);
    let mut filter_text = use_signal(String::new);
    let mut expanded: Signal<HashSet<usize>> = use_signal(HashSet::new);
    let mut search_trigger = use_signal(|| 0u64);
    let mut auto_searched = use_signal(|| false);

    // 目录浏览导航栈：栈顶为当前浏览目录，空 = 搜索结果模式
    let mut browse_stack: Signal<Vec<PathBuf>> = use_signal(Vec::new);
    let mut browsing_entries = use_signal(Vec::<DirEntry>::new);
    let mut browsing_error = use_signal(|| None::<String>);

    // 搜索结果类型过滤器：默认目录+文件全选
    let mut filter_types: Signal<HashSet<SearchResultType>> = use_signal(|| {
        let mut set = HashSet::new();
        set.insert(SearchResultType::Directory);
        set.insert(SearchResultType::File);
        set
    });

    // 从 context 获取 Actor 引用（跨组件存活），首次使用时创建
    let mut actor_ctx = use_context::<Signal<Option<ActorRef<SearchActor>>>>();
    if actor_ctx.read().is_none() {
        actor_ctx.set(Some(SearchActor::spawn(SearchActor::default())));
    }

    // 目录浏览时接管硬件返回键：弹栈，栈空则交给上层关闭对话框
    use_back_interceptor(!browse_stack().is_empty(), move || {
        let mut stack = browse_stack();
        if stack.pop().is_some() {
            browse_stack.set(stack.clone());
            match stack.last() {
                Some(top) => {
                    match DirLister::new(top).list(show_hidden()) {
                        Ok(list) => {
                            browsing_entries.set(list);
                            browsing_error.set(None);
                        }
                        Err(e) => {
                            browsing_error.set(Some(e.localize(&i18n_ctx)));
                            browsing_entries.set(Vec::new());
                        }
                    }
                }
                None => {
                    browsing_entries.set(Vec::new());
                    browsing_error.set(None);
                }
            }
        }
    });

    // 处理自动重新搜索（从编辑器返回时），恢复搜索参数
    if let Some(ref params) = props.search_params
        && !auto_searched()
    {
        auto_searched.set(true);
        query.set(params.query().clone());
        is_regex.set(*params.is_regex());
        case_sensitive.set(*params.case_sensitive());
        search_content.set(*params.search_content());
        only_first.set(*params.only_first_match());
        show_hidden.set(*params.show_hidden());
        search_trigger.set(search_trigger() + 1);
    }

    // 对话框关闭：取消搜索 + 清除 Actor（搜索历史终点）
    let mut on_close_with_cancel = move |_| {
        if let Some(actor_ref) = actor_ctx.take() {
            spawn(async move {
                if let Err(e) = actor_ref.tell(Cancel).await {
                    tracing::error!("Failed to send Cancel message: {:?}", e);
                }
            });
        }
        actor_ctx.set(None);
        props.on_close.call(());
    };

    let root = props.root.clone();

    let trigger_search = move |_| {
        let q = query().trim().to_string();
        if !q.is_empty() {
            let mut hist = history();
            if let Some(pos) = hist.iter().position(|h| h == &q) {
                hist.remove(pos);
            }
            hist.insert(0, q.clone());
            if hist.len() > 20 {
                hist.truncate(20);
            }
            history.set(hist);
            search_trigger.set(search_trigger() + 1);
        }
    };

    // 触发搜索（当 search_trigger 变化时）
    use_effect(move || {
        let trigger = search_trigger();
        if trigger != 0 {
            let opts = SearchOptions {
                query: query().trim().to_string(),
                is_regex: is_regex(),
                case_sensitive: case_sensitive(),
                search_content: search_content(),
                only_first_match: only_first(),
                show_hidden: show_hidden(),
                max_depth: 16,
            };

            // 保存搜索参数到 AppState（通过回调通知父组件）
            props
                .on_save_search_params
                .call((opts.clone(), root.clone()));

            if let Some(actor_ref) = actor_ctx.read().clone() {
                let root = root.clone();
                spawn(async move {
                    if let Err(e) = actor_ref.tell(DoSearch { opts, root }).await {
                        tracing::error!("Failed to send DoSearch message: {:?}", e);
                    }
                });
            }
        }
    });

    // 轮询 Actor 状态（当搜索开始时）
    use_effect(move || {
        let trigger = search_trigger();
        if let Some(actor_ref) = actor_ctx.read().clone()
            && trigger != 0
        {
            let mut results = results;
            let mut searching = searching;
            let mut error_msg = error_msg;

            spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    if let Ok(state) = actor_ref.ask(GetState).await {
                        results.set(state.results);
                        searching.set(state.searching);
                        error_msg.set(state.error);
                        if !state.searching {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            });
        }
    });

    let filtered_results: Vec<SearchResult> = {
        let ft = filter_text().to_lowercase();
        let types = filter_types();
        let all = results();
        all.into_iter()
            .filter(|r| {
                let text_match = ft.is_empty() || r.entry.name().to_lowercase().contains(&ft);
                let type_match = (types.contains(&SearchResultType::Directory) && r.entry.is_dir())
                    || (types.contains(&SearchResultType::File) && !r.entry.is_dir());
                text_match && type_match
            })
            .collect()
    };

    let expanded_current = expanded();
    let root_clone = props.root.clone();
    let on_open = props.on_open_file;
    let on_open_dir = EventHandler::new(move |path: PathBuf| {
        let mut stack = browse_stack();
        stack.push(path.clone());
        browse_stack.set(stack);
        // 同步加载（use_effect 有调度延迟，首次会闪"空目录"）
        match DirLister::new(&path).list(show_hidden()) {
            Ok(list) => {
                browsing_entries.set(list);
                browsing_error.set(None);
            }
            Err(e) => {
                browsing_error.set(Some(e.localize(&i18n_ctx)));
                browsing_entries.set(Vec::new());
            }
        }
    });
    let browsing = browse_stack().last().cloned();
    // 面包屑直接基于导航栈构建，每层对应栈中一个元素
    let breadcrumbs: Vec<String> = browse_stack()
        .iter()
        .map(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string()
        })
        .collect();

    rsx! {
        Overlay { on_close: move |_| on_close_with_cancel(()) }
        div {
            class: styles::search_dialog,
            onclick: move |e: Event<MouseData>| e.stop_propagation(),

            div { class: styles::dialog_card,

                div { class: styles::header,
                    if browsing.is_some() {
                        button {
                            class: styles::back_btn,
                            onclick: move |_| {
                                let mut stack = browse_stack();
                                stack.pop();
                                browse_stack.set(stack.clone());
                                // 同步加载新栈顶（或清空）
                                match stack.last() {
                                    Some(top) => {
                                        match DirLister::new(top).list(show_hidden()) {
                                            Ok(list) => {
                                                browsing_entries.set(list);
                                                browsing_error.set(None);
                                            }
                                            Err(e) => {
                                                browsing_error.set(Some(e.localize(&i18n_ctx)));
                                                browsing_entries.set(Vec::new());
                                            }
                                        }
                                    }
                                    None => {
                                        browsing_entries.set(Vec::new());
                                        browsing_error.set(None);
                                    }
                                }
                            },
                            "←"
                        }
                        span { class: styles::title,
                            { browsing
                                .as_ref()
                                .and_then(|d| d.file_name())
                                .and_then(|n| n.to_str())
                                .map(str::to_string)
                                .unwrap_or_else(|| i18n_support::translate(&i18n_ctx, "search-browse")) }
                        }
                    } else {
                        span { class: styles::title, { i18n_support::translate(&i18n_ctx, "common-advanced-search") } }
                    }
                    button {
                        class: styles::close_btn,
                        onclick: move |_| on_close_with_cancel(()),
                        "✕"
                    }
                }

                if browsing.is_some() {
                    // 目录浏览模式
                    div { class: styles::browse_breadcrumb_bar,
                        for (i, label) in breadcrumbs.iter().enumerate() {
                            if i > 0 {
                                span { class: styles::browse_breadcrumb_sep, "/" }
                            }
                            if i < breadcrumbs.len() - 1 {
                                button {
                                    class: styles::browse_breadcrumb_btn,
                                    onclick: {
                                        let target_len = i + 1;
                                        move |_| {
                                            let mut stack = browse_stack();
                                            stack.truncate(target_len);
                                            browse_stack.set(stack.clone());
                                            // 同步加载截断后的栈顶
                                            if let Some(top) = stack.last() {
                                                match DirLister::new(top).list(show_hidden()) {
                                                    Ok(list) => {
                                                        browsing_entries.set(list);
                                                        browsing_error.set(None);
                                                    }
                                                    Err(e) => {
                                                        browsing_error.set(Some(e.localize(&i18n_ctx)));
                                                        browsing_entries.set(Vec::new());
                                                    }
                                                }
                                            }
                                        }
                                    },
                                    "{label}"
                                }
                            } else {
                                span { class: styles::browse_breadcrumb_current, "{label}" }
                            }
                        }
                    }

                    if let Some(ref err) = browsing_error() {
                        div { class: "{styles::status} {styles::error}",
                            { i18n_support::translate_err(&i18n_ctx, "err-browse", err) }
                        }
                    } else if browsing_entries().is_empty() {
                        div { class: styles::browse_empty,
                            FolderIcon {}
                            span { { i18n_support::translate(&i18n_ctx, "common-empty-dir") } }
                        }
                    } else {
                        div { class: styles::browse_entry_list,
                            for entry in browsing_entries().iter() {
                                {
                                    let entry = entry.clone();
                                    rsx! {
                                        EntryItem {
                                            entry: entry.clone(),
                                            subtitle: None::<String>,
                                            on_dir_click: move |path: PathBuf| {
                                                let mut stack = browse_stack();
                                                stack.push(path.clone());
                                                browse_stack.set(stack);
                                                match DirLister::new(&path).list(show_hidden()) {
                                                    Ok(list) => {
                                                        browsing_entries.set(list);
                                                        browsing_error.set(None);
                                                    }
                                                    Err(e) => {
                                                        browsing_error.set(Some(e.localize(&i18n_ctx)));
                                                        browsing_entries.set(Vec::new());
                                                    }
                                                }
                                            },
                                            on_long_press: move |_| {},
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    // 搜索模式
                    div { class: styles::search_bar,
                        div { class: styles::query_row,
                            input {
                                class: styles::query_input,
                                r#type: "text",
                                placeholder: i18n_support::translate(&i18n_ctx, "search-query-placeholder"),
                                value: "{query}",
                                autofocus: true,
                                oninput: move |e| query.set(e.value()),
                                onkeydown: {
                                    let mut search = trigger_search;
                                    move |e| {
                                        if e.key() == Key::Enter {
                                            search(());
                                        }
                                    }
                                },
                            }
                            button {
                                class: styles::search_btn,
                                onclick: {
                                    let mut search = trigger_search;
                                    move |_| search(())
                                },
                                disabled: searching() || query().trim().is_empty(),
                                { i18n_support::translate(&i18n_ctx, "common-search") }
                            }
                        }

                        if !history().is_empty() {
                            div { class: styles::history_section,
                                select {
                                    class: styles::history_select,
                                    value: "",
                                    onchange: move |e| {
                                        let v = e.value();
                                        if !v.is_empty() {
                                            query.set(v);
                                        }
                                    },
                                    option { value: "", { i18n_support::translate(&i18n_ctx, "search-history") } }
                                    for h in history() {
                                        option { value: "{h}", "{h}" }
                                    }
                                }
                            }
                        }

                        div { class: styles::options,
                            div {
                                class: styles::option,
                                onclick: move |_| {
                                    is_regex.set(!is_regex());
                                },
                                input {
                                    r#type: "checkbox",
                                    checked: is_regex(),
                                    onmousedown: move |e: Event<MouseData>| {
                                        e.prevent_default();
                                        is_regex.set(!is_regex());
                                    },
                                    onclick: move |e: Event<MouseData>| {
                                        e.stop_propagation();
                                    },
                                    oninput: move |e| is_regex.set(e.checked()),
                                }
                                span { { i18n_support::translate(&i18n_ctx, "search-regex") } }
                            }
                            div {
                                class: styles::option,
                                onclick: move |_| {
                                    case_sensitive.set(!case_sensitive());
                                },
                                input {
                                    r#type: "checkbox",
                                    checked: case_sensitive(),
                                    onmousedown: move |e: Event<MouseData>| {
                                        e.prevent_default();
                                        case_sensitive.set(!case_sensitive());
                                    },
                                    onclick: move |e: Event<MouseData>| {
                                        e.stop_propagation();
                                    },
                                    oninput: move |e| case_sensitive.set(e.checked()),
                                }
                                span { { i18n_support::translate(&i18n_ctx, "search-case-sensitive") } }
                            }
                            div {
                                class: styles::option,
                                onclick: move |_| {
                                    search_content.set(!search_content());
                                },
                                input {
                                    r#type: "checkbox",
                                    checked: search_content(),
                                    onmousedown: move |e: Event<MouseData>| {
                                        e.prevent_default();
                                        search_content.set(!search_content());
                                    },
                                    onclick: move |e: Event<MouseData>| {
                                        e.stop_propagation();
                                    },
                                    oninput: move |e| search_content.set(e.checked()),
                                }
                                span { { i18n_support::translate(&i18n_ctx, "search-content") } }
                            }
                            if search_content() {
                                div {
                                    class: "{styles::option} {styles::sub_option}",
                                    onclick: move |_| {
                                        only_first.set(!only_first());
                                    },
                                    input {
                                        r#type: "checkbox",
                                        checked: only_first(),
                                        onmousedown: move |e: Event<MouseData>| {
                                            e.prevent_default();
                                            only_first.set(!only_first());
                                        },
                                        onclick: move |e: Event<MouseData>| {
                                            e.stop_propagation();
                                        },
                                        oninput: move |e| only_first.set(e.checked()),
                                    }
                                    span { { i18n_support::translate(&i18n_ctx, "search-only-first") } }
                                }
                            }
                            div {
                                class: styles::option,
                                onclick: move |_| {
                                    show_hidden.set(!show_hidden());
                                },
                                input {
                                    r#type: "checkbox",
                                    checked: show_hidden(),
                                    onmousedown: move |e: Event<MouseData>| {
                                        e.prevent_default();
                                        show_hidden.set(!show_hidden());
                                    },
                                    onclick: move |e: Event<MouseData>| {
                                        e.stop_propagation();
                                    },
                                    oninput: move |e| show_hidden.set(e.checked()),
                                }
                                span { { i18n_support::translate(&i18n_ctx, "common-show-hidden") } }
                            }
                        }
                    }

                    if searching() {
                        div { class: styles::status,
                            span { class: styles::spinner }
                            span { { i18n_support::translate(&i18n_ctx, "search-in-progress") } }
                        }
                    } else if let Some(ref err) = error_msg() {
                        div { class: "{styles::status} {styles::error}",
                            { i18n_support::translate_err(&i18n_ctx, "err-search", err) }
                        }
                    } else if !results().is_empty() {
                        div { class: styles::filter_bar,
                            input {
                                class: styles::filter_input,
                                r#type: "text",
                                placeholder: i18n_support::translate(&i18n_ctx, "search-filter-placeholder"),
                                value: "{filter_text}",
                                oninput: move |e| filter_text.set(e.value()),
                            }
                            div { class: styles::filter_types,
                                button {
                                    class: if filter_types().contains(&SearchResultType::Directory) {
                                        format!("{} {}", styles::filter_type_btn, styles::active)
                                    } else {
                                        styles::filter_type_btn.to_string()
                                    },
                                    onclick: move |_| {
                                        let mut types = filter_types();
                                        if types.contains(&SearchResultType::Directory) {
                                            // 最少保留一个
                                            if types.len() > 1 {
                                                types.remove(&SearchResultType::Directory);
                                            }
                                        } else {
                                            types.insert(SearchResultType::Directory);
                                        }
                                        filter_types.set(types);
                                    },
                                    { i18n_support::translate(&i18n_ctx, "search-type-dir") }
                                }
                                button {
                                    class: if filter_types().contains(&SearchResultType::File) {
                                        format!("{} {}", styles::filter_type_btn, styles::active)
                                    } else {
                                        styles::filter_type_btn.to_string()
                                    },
                                    onclick: move |_| {
                                        let mut types = filter_types();
                                        if types.contains(&SearchResultType::File) {
                                            if types.len() > 1 {
                                                types.remove(&SearchResultType::File);
                                            }
                                        } else {
                                            types.insert(SearchResultType::File);
                                        }
                                        filter_types.set(types);
                                    },
                                    { i18n_support::translate(&i18n_ctx, "search-type-file") }
                                }
                            }
                            span { class: styles::result_count,
                                { i18n_support::translate_with(
                                    &i18n_ctx,
                                    "search-result-count",
                                    &[
                                        ("done", filtered_results.len().to_string().into()),
                                        ("total", results().len().to_string().into()),
                                    ],
                                ) }
                            }
                        }
                    }

                    div { class: styles::result_list,
                        if filtered_results.is_empty() && !searching() && error_msg().is_none() {
                            if results().is_empty() {
                                div { class: styles::empty_hint, { i18n_support::translate(&i18n_ctx, "search-empty-hint") } }
                            } else {
                                div { class: styles::empty_hint, { i18n_support::translate(&i18n_ctx, "search-no-match") } }
                            }
                        }
                        for (idx , result) in filtered_results.iter().enumerate() {
                            SearchResultItem {
                                result: result.clone(),
                                root: root_clone.clone(),
                                on_open,
                                on_open_dir,
                                is_expanded: expanded_current.contains(&idx),
                                on_toggle_expand: EventHandler::new(move |_| {
                                    let mut set = expanded();
                                    if set.contains(&idx) {
                                        set.remove(&idx);
                                    } else {
                                        set.insert(idx);
                                    }
                                    expanded.set(set);
                                }),
                            }
                        }
                    }
                }
            }
        }
    }
}
