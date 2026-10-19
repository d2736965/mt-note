//! 基于 div 的代码编辑器组件。
//!
//! 采用两层叠加架构（LSP 语义高亮）：
//! - 底层：高亮渲染层（只读），将 semantic token 渲染为带语义 class 的 `<span>`
//! - 顶层：透明输入层（textarea），接收用户输入
//!
//! 折行统一由 CSS 控制（`white-space` / `overflow-wrap`），两层共用同一套
//! 折行规则，保证 `scrollHeight` 一致。textarea 是唯一滚动源，通过 `onscroll`
//! 单向同步 highlight_layer 的 `scrollTop` / `scrollLeft`。
//!
//! 光标位置变化时自动查询 LSP hover/诊断，通过 CursorInfoPanel 显示。

use std::path::PathBuf;
use std::rc::Rc;

use dioxus::html::geometry::PixelsVector2D;
use dioxus::logger::tracing;
use dioxus::prelude::*;
use dioxus_i18n::prelude::i18n;
use models::{EditorPrefs, WordWrap};
use mt_note_editor_lsp::LspContext;
use mt_note_editor_lsp::types::{DiagnosticSeverity, LspDiagnostic, SemanticToken, TextPosition};
use serde_json;
use ui::hooks::use_app_state::use_app_state;

use crate::frontend::hooks::use_back_interceptor::use_back_interceptor;
use crate::frontend::i18n as i18n_support;
use crate::frontend::js_bridge::{DioxusBridge, js_api};
use crate::frontend::views::editor::ScrollSyncState;

mod components;

pub use components::completion_popup::*;
pub use components::cursor_info_panel::*;
pub use components::touch::*;

stylance::import_style!(styles, "code_editor.module.scss");

/// 基于 div 的代码编辑器。
///
/// 两层叠加架构（LSP 语义高亮）：
/// - 底层：高亮渲染层（semantic token 着色）
/// - 顶层：透明输入层（textarea）接收输入
///
/// 折行由 CSS 统一控制（`wrap_on` / `wrap_off` class），
/// textarea 的 `onscroll` 单向同步 highlight_layer 的滚动位置。
///
/// 光标位置变化时自动查询 LSP hover/诊断信息，
/// 由 AppBar 的 trailing buttons 控制是否显示。
#[component]
pub fn CodeEditor(
    content: Signal<String>,
    file_path: Option<PathBuf>,
    on_change: EventHandler<String>,
) -> Element {
    let i18n_ctx = i18n();
    let prefs = match use_app_state() {
        Ok(state) => state.read().editor_prefs().clone(),
        Err(e) => {
            tracing::error!("CodeEditor: AppState 未初始化: {e}");
            EditorPrefs::default()
        }
    };

    let editor_id = use_hook(|| {
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        format!(
            "editor-{}",
            COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        )
    });
    let show_line_numbers = prefs.show_line_numbers;
    let show_hover = prefs.show_hover_info;
    let show_diag = prefs.show_diagnostics;
    let white_space = match prefs.word_wrap {
        WordWrap::On => "pre-wrap",
        WordWrap::Off => "pre",
    };

    let visual_lines: Memo<Vec<VisualLine>> = use_memo(move || {
        content()
            .split('\n')
            .enumerate()
            .map(|(i, l)| VisualLine {
                text: l.to_string(),
                logical_line: i,
            })
            .collect::<Vec<_>>()
    });

    let lsp_context = use_context::<LspContext>();
    let manager = lsp_context.manager.clone();

    let semantic_tokens: Signal<Vec<SemanticToken>> = use_signal(Vec::new);
    let diagnostics: Signal<Vec<LspDiagnostic>> = use_signal(Vec::new);
    let scroll_top: Signal<f64> = use_signal(|| 0.0);
    let scroll_left: Signal<f64> = use_signal(|| 0.0);

    let mut cursor_hover: Signal<Option<mt_note_editor_lsp::types::HoverInfo>> =
        use_signal(|| None);
    let mut cursor_diagnostics: Signal<Vec<LspDiagnostic>> = use_signal(Vec::new);
    let cursor_pos: Signal<Option<TextPosition>> = use_signal(|| None);
    let cursor_char_column: Signal<usize> = use_signal(|| 0);

    let references_result: Signal<Vec<mt_note_editor_lsp::types::Location>> = use_signal(Vec::new);
    let mut show_references: Signal<bool> = use_signal(|| false);

    let mut rename_active: Signal<bool> = use_signal(|| false);
    let rename_pos: Signal<Option<TextPosition>> = use_signal(|| None);
    let mut rename_input: Signal<String> = use_signal(String::new);
    use_back_interceptor(rename_active(), move || {
        rename_active.set(false);
    });

    let completions: Signal<Vec<mt_note_editor_lsp::types::Completion>> = use_signal(Vec::new);
    let mut show_completions: Signal<bool> = use_signal(|| false);
    let completion_selected: Signal<Option<usize>> = use_signal(|| None);
    let completion_pos: Signal<Option<TextPosition>> = use_signal(|| None);

    let trigger_completion = {
        let manager = manager.clone();
        let file_path = file_path.clone();
        let show_completions = show_completions;
        move || {
            let manager = manager.clone();
            let file_path = file_path.clone();
            let mut completions = completions;
            let mut completion_pos = completion_pos;
            let mut show_completions = show_completions;
            let cursor_pos = cursor_pos;
            spawn(async move {
                if let (Some(pos), Some(path)) = (cursor_pos(), file_path.as_ref()) {
                    let result = manager
                        .ask(mt_note_editor_lsp::Completions {
                            path: path.clone(),
                            pos,
                        })
                        .await;
                    let items = result.unwrap_or_default();
                    if !items.is_empty() {
                        completion_pos.set(Some(pos));
                        completions.set(items);
                        show_completions.set(true);
                    }
                }
            });
        }
    };

    let scroll_sync: ScrollSyncState = use_context();

    let font_metrics = FontMetrics {
        char_width: 14.0 * 0.6,
        line_height: 14.0 * 1.6,
        gutter_width: 40.0,
        padding_x: 8.0,
    };

    let lsp_resource = {
        let manager_lsp = manager.clone();
        let file_path_lsp = file_path.clone();
        let mut semantic_tokens = semantic_tokens;
        let mut diagnostics = diagnostics;
        use_resource(move || {
            let content = content();
            let path = file_path_lsp.clone();
            let manager = manager_lsp.clone();
            async move {
                if let Some(path) = path {
                    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
                    let result = manager
                        .ask(mt_note_editor_lsp::Update {
                            path: path.clone(),
                            content: content.clone(),
                        })
                        .await;
                    if let Err(e) = result {
                        tracing::error!("LSP update failed: {e:?}");
                    }
                    let tokens = manager
                        .ask(mt_note_editor_lsp::SemanticTokens { path: path.clone() })
                        .await
                        .unwrap_or_default();
                    let diags = manager
                        .ask(mt_note_editor_lsp::Diagnostics { path: path.clone() })
                        .await
                        .unwrap_or_default();
                    semantic_tokens.set(tokens);
                    diagnostics.set(diags);
                }
                0u8
            }
        })
    };

    let hover_resource = {
        let manager_refresh = manager.clone();
        let file_path_refresh = file_path.clone();
        let cursor_hover = cursor_hover;
        let cursor_diagnostics = cursor_diagnostics;
        use_resource(move || {
            let diags = diagnostics();
            let pos = cursor_pos();
            let manager = manager_refresh.clone();
            let path = file_path_refresh.clone();
            let mut cursor_hover = cursor_hover;
            let mut cursor_diagnostics = cursor_diagnostics;
            async move {
                if let Some(pos_val) = pos {
                    let pos_diags = diagnostics_at_position(pos_val, &diags);
                    cursor_diagnostics.set(pos_diags);
                    if let Some(p) = path {
                        let hover = manager
                            .ask(mt_note_editor_lsp::Hover {
                                path: p,
                                pos: pos_val,
                            })
                            .await
                            .ok()
                            .flatten();
                        cursor_hover.set(hover);
                    }
                } else {
                    cursor_diagnostics.set(Vec::new());
                    cursor_hover.set(None);
                }
                0u8
            }
        })
    };

    let mut highlight_el: Signal<Option<Rc<MountedData>>> = use_signal(|| None);
    let mut textarea_el: Signal<Option<Rc<MountedData>>> = use_signal(|| None);

    // 光标位置事件驱动监听（CustomEvent 模式，通过 Bridge.waitForEvent）

    {
        let mut last_sync: Signal<u32> = use_signal(|| 0);
        use_effect(move || {
            let current = (scroll_sync.pending_sync)();
            tracing::debug!(
                "CodeEditor use_effect: pending_sync={current}, last_sync={}",
                last_sync()
            );
            if current > last_sync() {
                last_sync.set(current);
                let anchor_line = (scroll_sync.anchor_line)();
                let anchor_ratio = (scroll_sync.anchor_viewport_ratio)();
                let ratio = (scroll_sync.preview_scroll_ratio)();
                tracing::debug!(
                    "CodeEditor sync triggered: anchor_line={anchor_line}, anchor_ratio={anchor_ratio:.3}, ratio={ratio:.3}"
                );
                let ta = textarea_el;
                let hl = highlight_el;
                spawn(async move {
                    if anchor_line > 0 {
                        scroll_editor_to_anchor(anchor_line, anchor_ratio, ta, hl).await;
                    } else {
                        scroll_editor_to_ratio(ratio, ta, hl).await;
                    }
                });
            }
        });
    }

    // 光标位置事件驱动监听（CustomEvent 模式，通过 Bridge.waitForEvent）
    // JS 侧在 input/click/keyup(方向键)/select 事件中计算光标位置并推送，
    // 同时附带 scrollTop/scrollLeft，供面板定位使用
    // 必须用 use_effect 保证只 spawn 一次：createEvents 的 resolvers 是队列，
    // 若每次重渲染都 spawn，旧（已取消）loop 的 resolver 会残留队列，dispatch
    // 被旧 resolver 消费，当前 loop 永远收不到 → cursor_pos 不更新 → hover 失效。
    // 注意：use_hook 内 spawn 在 Dioxus 0.7 中不会调度任务，必须用 use_effect。
    use_effect(move || {
        let mut cursor_pos = cursor_pos;
        let mut cursor_char_column = cursor_char_column;
        let mut scroll_sync = scroll_sync;
        let mut scroll_top = scroll_top;
        let mut scroll_left = scroll_left;
        spawn(async move {
            loop {
                let json = js_api::events::wait_for_event::<DioxusBridge>("cursor-change").await;
                let parsed: serde_json::Value = match serde_json::from_str(&json) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!("cursor-change parse error: {e}");
                        continue;
                    }
                };
                let line = parsed.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let column = parsed.get("column").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let char_column = parsed
                    .get("char_column")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize;
                let st = parsed
                    .get("scrollTop")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let sl = parsed
                    .get("scrollLeft")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                cursor_pos.set(Some(TextPosition { line, column }));
                cursor_char_column.set(char_column);
                scroll_sync.anchor_line.set(line);
                scroll_top.set(st);
                scroll_left.set(sl);
            }
        });
    });

    fn line_semantic_tokens(
        line: usize,
        _line_text: &str,
        all_tokens: &[SemanticToken],
    ) -> Vec<(usize, usize, mt_note_editor_lsp::types::SemanticTokenType)> {
        let mut result = Vec::new();
        let mut current_line = 0;
        let mut current_start = 0;

        for token in all_tokens {
            current_line += token.delta_line as usize;
            if token.delta_line == 0 {
                current_start += token.delta_start as usize;
            } else {
                current_start = token.delta_start as usize;
            }

            if current_line == line {
                let start = current_start;
                let end = start + token.length as usize;
                if !_line_text.is_char_boundary(start) || !_line_text.is_char_boundary(end) {
                    tracing::error!(
                        "line_semantic_tokens: 无效的 UTF-8 边界 start={start}, end={end}, line_len={}, line={:?}",
                        _line_text.len(),
                        _line_text
                    );
                    continue;
                }
                result.push((start, end, token.token_type));
            }
        }

        result
    }

    let has_info =
        (show_hover && cursor_hover().is_some()) || (show_diag && !cursor_diagnostics().is_empty());

    let lsp_loading = lsp_resource.pending();
    let hover_loading = hover_resource.pending();

    rsx! {
        div {
            class: styles::code_editor,
            style: if lsp_loading || hover_loading { "--loading: 1" } else { "--loading: 0" },
            div { class: styles::editor_scroll,
                div { class: styles::editor_content,
                    div {
                        class: styles::highlight_layer,
                        id: "{editor_id}-hl",
                        onmounted: move |e: MountedEvent| {
                            highlight_el.set(Some(e.data()));
                        },
                        {
                            let lines = visual_lines();
                            rsx! {
                                for visual_line in &lines {
                                    div { class: styles::editor_row,
                                        if show_line_numbers {
                                            div { class: styles::line_gutter_cell,
                                                span { class: styles::line_number, "{visual_line.logical_line + 1}" }
                                            }
                                        }
                                        div { class: styles::editor_line, white_space,
                                            {
                                                let line_tokens = line_semantic_tokens(
                                                    visual_line.logical_line,
                                                    &visual_line.text,
                                                    &semantic_tokens(),
                                                );
                                                let line_diags = line_diagnostics(
                                                    visual_line.logical_line,
                                                    &visual_line.text,
                                                    &diagnostics(),
                                                );
                                                rsx! {
                                                    components::highlighted_line::HighlightedLine {
                                                        line: visual_line.text.clone(),
                                                        semantic_tokens: line_tokens,
                                                        diagnostics: line_diags,
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    textarea {
                        class: styles::input_layer,
                        id: "{editor_id}-ta",
                        spellcheck: false,
                        white_space,
                        onmounted: move |e: MountedEvent| {
                            textarea_el.set(Some(e.data()));
                            let ta_id = format!("{editor_id}-ta");
                            let hl_id = format!("{editor_id}-hl");
                            let initial_content = content();
                            spawn(async move {
                                js_api::editor::set_textarea_value::<DioxusBridge>(&initial_content).await;
                            });
                            js_api::editor::init_scroll_sync::<DioxusBridge>(&ta_id, &hl_id);
                            js_api::editor::init_cursor_tracker::<DioxusBridge>(&ta_id);
                        },
                        oninput: {
                            let trigger_completion = trigger_completion.clone();
                            let mut scroll_sync = scroll_sync;
                            move |e: Event<FormData>| {
                                let value = e.data.value();
                                let ends_with_dot = value.ends_with('.');
                                content.set(value);
                                on_change.call(content());
                                scroll_sync
                                    .total_lines
                                    .set(content().split('\n').count());
                                if ends_with_dot {
                                    trigger_completion();
                                }
                            }
                        },
                        onkeydown: {
                            let trigger_completion = trigger_completion.clone();
                            move |e: KeyboardEvent| {
                                if e.data().code() == Code::Space && e.data().modifiers().ctrl() {
                                    e.prevent_default();
                                    trigger_completion();
                                } else if e.key() == Key::Escape {
                                    show_completions.set(false);
                                }
                            }
                        },
                        onclick: {
                            let file_path_click = file_path.clone();
                            let manager_click = manager.clone();
                            move |_e: Event<MouseData>| {
                                cursor_hover.set(None);
                                cursor_diagnostics.set(Vec::new());
                                if let Some(path) = file_path_click.clone() {
                                    let manager = manager_click.clone();
                                    let mut cursor_hover = cursor_hover;
                                    let mut cursor_diagnostics = cursor_diagnostics;
                                    let cursor_pos = cursor_pos;
                                    spawn(async move {
                                        tokio::time::sleep(tokio::time::Duration::from_millis(150))
                                            .await;
                                        if let Some(pos) = cursor_pos() {
                                            let all_diags: Vec<LspDiagnostic> = manager
                                                .ask(mt_note_editor_lsp::Diagnostics {
                                                    path: path.clone(),
                                                })
                                                .await
                                                .unwrap_or_default();
                                            let diags =
                                                diagnostics_at_position(pos, &all_diags);
                                            cursor_diagnostics.set(diags);
                                            let hover = manager
                                                .ask(mt_note_editor_lsp::Hover {
                                                    path,
                                                    pos,
                                                })
                                                .await
                                                .ok()
                                                .flatten();
                                            cursor_hover.set(hover);
                                        }
                                    });
                                }
                            }
                        },
                    }
                    if has_info {
                        if let Some(pos) = cursor_pos() {
                            {
                                let (x, y) = text_position_to_pixel(
                                    pos,
                                    cursor_char_column(),
                                    font_metrics,
                                    scroll_top(),
                                    scroll_left(),
                                );
                                rsx! {
                                    CursorInfoPanel {
                                        x,
                                        cursor_y: y,
                                        line_height: font_metrics.line_height,
                                        hover_info: if show_hover { cursor_hover() } else { None },
                                        diagnostics: if show_diag { cursor_diagnostics() } else { Vec::new() },
                                        show_hover,
                                        show_diag,
                                    }
                                }
                            }
                        }
                    }
                    if show_completions() && !completions().is_empty() {
                        {
                            let pos = completion_pos().unwrap_or(TextPosition { line: 0, column: 0 });
                            let (x, y) = text_position_to_pixel(
                                pos,
                                pos.column,
                                font_metrics,
                                scroll_top(),
                                scroll_left(),
                            );
                            rsx! {
                                CompletionPopup {
                                    x,
                                    y,
                                    items: completions(),
                                    selected: completion_selected(),
                                    on_select: move |completion: mt_note_editor_lsp::types::Completion| {
                                        let insert = completion.insert_text.clone();
                                        let mut current = content();
                                        current.push_str(&insert);
                                        content.set(current);
                                        let content_val = content();
                                        spawn(async move {
                                            js_api::editor::set_textarea_value::<DioxusBridge>(&content_val).await;
                                        });
                                        show_completions.set(false);
                                    },
                                }
                            }
                        }
                    }
                    if show_references() {
                        div { class: styles::references_panel,
                            div { class: styles::references_header,
                                span {
                                    { i18n_support::translate_with(
                                        &i18n_ctx,
                                        "editor-references",
                                        &[("n", references_result().len().to_string().into())],
                                    ) }
                                }
                                button {
                                    class: styles::references_close,
                                    onclick: move |_| show_references.set(false),
                                    "✕"
                                }
                            }
                            div { class: styles::references_list,
                                for ref_loc in references_result() {
                                    div {
                                        class: styles::references_item,
                                        onclick: {
                                            let loc = ref_loc.clone();
                                            let textarea_el = textarea_el;
                                            let highlight_el = highlight_el;
                                            move |_| {
                                                spawn(async move {
                                                    scroll_editor_to_line(
                                                            loc.range.start.line,
                                                            font_metrics,
                                                            textarea_el,
                                                            highlight_el,
                                                        )
                                                        .await;
                                                });
                                                show_references.set(false);
                                            }
                                        },
                                        span { class: styles::references_line,
                                            "L{ref_loc.range.start.line + 1}"
                                        }
                                        span { class: styles::references_path,
                                            "{ref_loc.path.display()}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if rename_active() {
                        div { class: styles::rename_overlay,
                            div { class: styles::rename_dialog,
                                span { class: styles::rename_label, { i18n_support::translate(&i18n_ctx, "editor-rename-label") } }
                                input {
                                    class: styles::rename_input,
                                    value: "{rename_input}",
                                    oninput: move |e: Event<FormData>| {
                                        rename_input.set(e.data.value());
                                    },
                                    onkeydown: {
                                        let manager = manager.clone();
                                        let file_path = file_path.clone();
                                        let content = content;
                                        let mut rename_active = rename_active;
                                        move |e: KeyboardEvent| {
                                            if e.key() == Key::Enter {
                                                if let Some(path) = &file_path
                                                    && let Some(pos) = rename_pos()
                                                {
                                                    let new_name = rename_input();
                                                    let manager = manager.clone();
                                                    let path = path.clone();
                                                    let mut content = content;
                                                    let mut rename_active = rename_active;
                                                    spawn(async move {
                                                        let edits = manager
                                                            .ask(mt_note_editor_lsp::Rename {
                                                                path,
                                                                pos,
                                                                new_name,
                                                            })
                                                            .await;
                                                        if let Ok(edits) = edits {
                                                            if !edits.is_empty() {
                                                                let mut text = content();
                                                                for edit in edits.iter().rev() {
                                                                    let start = position_to_offset(&text, edit.range.start);
                                                                    let end = position_to_offset(&text, edit.range.end);
                                                                    if let (Some(s), Some(e)) = (start, end) {
                                                                        text.replace_range(s..e, &edit.new_text);
                                                                    }
                                                                }
                                                                content.set(text);
                                                                js_api::editor::set_textarea_value::<
                                                                    DioxusBridge,
                                                                >(&content())
                                                                    .await;
                                                            }
                                                            rename_active.set(false);
                                                        }
                                                    });
                                                    rename_active.set(false);
                                                }
                                            } else if e.key() == Key::Escape {
                                                rename_active.set(false);
                                            }
                                        }
                                    },
                                }
                                div { class: styles::rename_actions,
                                    button {
                                        class: styles::rename_cancel,
                                        onclick: move |_| rename_active.set(false),
                                        { i18n_support::translate(&i18n_ctx, "common-cancel") }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn line_diagnostics(
    line_index: usize,
    line_text: &str,
    all_diagnostics: &[LspDiagnostic],
) -> Vec<(usize, usize, DiagnosticSeverity, String)> {
    let mut result = Vec::new();
    for d in all_diagnostics {
        if line_index < d.range.start.line || line_index > d.range.end.line {
            continue;
        }
        let start = if d.range.start.line == line_index {
            d.range.start.column
        } else {
            0
        };
        let end = if d.range.end.line == line_index {
            d.range.end.column.min(line_text.len())
        } else {
            line_text.len()
        };
        if start < end {
            if !line_text.is_char_boundary(start) || !line_text.is_char_boundary(end) {
                tracing::error!(
                    "line_diagnostics: 无效的 UTF-8 边界 start={start}, end={end}, line_len={}, line={:?}",
                    line_text.len(),
                    line_text
                );
                continue;
            }
            result.push((start, end, d.severity, d.message.clone()));
        }
    }
    result.sort_by_key(|(s, _, _, _)| *s);
    result
}

fn diagnostics_at_position(
    pos: TextPosition,
    all_diagnostics: &[LspDiagnostic],
) -> Vec<LspDiagnostic> {
    all_diagnostics
        .iter()
        .filter(|d| {
            pos.line >= d.range.start.line
                && pos.line <= d.range.end.line
                && (pos.line > d.range.start.line || pos.column >= d.range.start.column)
                && (pos.line < d.range.end.line || pos.column <= d.range.end.column)
        })
        .cloned()
        .collect()
}

fn position_to_offset(text: &str, pos: TextPosition) -> Option<usize> {
    let mut offset = 0usize;
    for (line_idx, line_text) in text.split('\n').enumerate() {
        if line_idx == pos.line {
            return Some(offset + pos.column.min(line_text.len()));
        }
        offset += line_text.len() + 1;
    }
    None
}

async fn scroll_editor_to_anchor(
    anchor_line: usize,
    anchor_ratio: f64,
    _textarea_el: Signal<Option<Rc<MountedData>>>,
    _highlight_el: Signal<Option<Rc<MountedData>>>,
) {
    tracing::debug!(
        "scroll_editor_to_anchor: start, anchor_line={anchor_line}, anchor_ratio={anchor_ratio:.3}"
    );
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let json_str = js_api::editor::get_info::<DioxusBridge>().await;
    let result: serde_json::Value = serde_json::from_str(&json_str).unwrap_or_default();
    if !result.is_null() {
        let max_scroll = result["max"].as_f64().unwrap_or(0.0);
        let client_height = result["clientHeight"].as_f64().unwrap_or(1.0);
        tracing::debug!(
            "scroll_editor_to_anchor: max_scroll={max_scroll}, client_height={client_height}"
        );

        if max_scroll > 0.0 {
            let line_top =
                js_api::editor::get_line_position::<DioxusBridge>(anchor_line as f64).await;

            let target_scroll = (line_top - anchor_ratio * client_height)
                .max(0.0)
                .min(max_scroll);
            tracing::debug!(
                "scroll_editor_to_anchor: line_top={line_top}, target_scroll={target_scroll}"
            );

            js_api::editor::scroll_to::<DioxusBridge>(target_scroll);
            tracing::debug!("scroll_editor_to_anchor: scroll command sent");
        } else {
            tracing::debug!("scroll_editor_to_anchor: max_scroll=0, skipping");
        }
    } else {
        tracing::warn!("scroll_editor_to_anchor: 读取编辑器信息返回 null");
    }
}

async fn scroll_editor_to_ratio(
    ratio: f64,
    _textarea_el: Signal<Option<Rc<MountedData>>>,
    _highlight_el: Signal<Option<Rc<MountedData>>>,
) {
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let max_scroll = js_api::editor::get_max_scroll::<DioxusBridge>().await;

    if max_scroll > 0.0 {
        let target_scroll = max_scroll * ratio;
        js_api::editor::scroll_to::<DioxusBridge>(target_scroll);
    }
}

async fn scroll_editor_to_line(
    line: usize,
    metrics: FontMetrics,
    textarea_el: Signal<Option<Rc<MountedData>>>,
    highlight_el: Signal<Option<Rc<MountedData>>>,
) {
    let target_scroll = line as f64 * metrics.line_height;
    tokio::time::sleep(tokio::time::Duration::from_millis(16)).await;

    let ta_el = textarea_el();
    let hl_el = highlight_el();

    if let Some(el) = ta_el
        && let Err(err) = el
            .scroll(
                PixelsVector2D::new(0.0, target_scroll),
                dioxus::html::ScrollBehavior::Instant,
            )
            .await
    {
        tracing::error!("scroll textarea failed: {err}");
    }

    if let Some(el) = hl_el
        && let Err(err) = el
            .scroll(
                PixelsVector2D::new(0.0, target_scroll),
                dioxus::html::ScrollBehavior::Instant,
            )
            .await
    {
        tracing::error!("scroll highlight failed: {err}");
    }
}

#[derive(Clone, PartialEq)]
struct VisualLine {
    text: String,
    logical_line: usize,
}

/// 将 TextPosition 转换为像素坐标。
fn text_position_to_pixel(
    pos: TextPosition,
    char_column: usize,
    metrics: FontMetrics,
    scroll_top: f64,
    scroll_left: f64,
) -> (f64, f64) {
    let x = char_column as f64 * metrics.char_width + metrics.gutter_width + metrics.padding_x
        - scroll_left;
    let y = (pos.line + 1) as f64 * metrics.line_height - scroll_top;
    (x, y)
}