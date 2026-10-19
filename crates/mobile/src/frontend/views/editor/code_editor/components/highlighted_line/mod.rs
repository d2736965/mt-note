use dioxus::logger::tracing;
use dioxus::prelude::*;
use mt_note_editor_lsp::types::{DiagnosticSeverity, SemanticTokenType};

use super::super::styles;
use super::semantic_highlight::token_class;
use crate::frontend::router::Route;
use crate::frontend::views::editor_error::EditorErrorSignal;

#[component]
pub fn HighlightedLine(
    line: String,
    semantic_tokens: Vec<(usize, usize, SemanticTokenType)>,
    diagnostics: Vec<(usize, usize, DiagnosticSeverity, String)>,
) -> Element {
    if line.is_empty() {
        return rsx! { "" };
    }

    if semantic_tokens.is_empty() && diagnostics.is_empty() {
        return rsx! {
            span { "{line}" }
        };
    }

    let mut all_segments: Vec<(
        usize,
        usize,
        Option<SemanticTokenType>,
        Option<DiagnosticSeverity>,
    )> = Vec::new();

    for (start, end, tt) in &semantic_tokens {
        let end = (*end).min(line.len());
        if *start < end {
            all_segments.push((*start, end, Some(*tt), None));
        }
    }
    for (start, end, sev, _msg) in &diagnostics {
        let end = (*end).min(line.len());
        if *start < end {
            all_segments.push((*start, end, None, Some(*sev)));
        }
    }

    all_segments.sort_by_key(|(s, _, _, _)| *s);

    let mut spans = Vec::new();
    let mut i = 0usize;
    while i < line.len() {
        let mut semantic: Option<SemanticTokenType> = None;
        let mut diag_severity: Option<DiagnosticSeverity> = None;

        for (start, end, tt, sev) in &all_segments {
            if i >= *start && i < *end {
                if tt.is_some() {
                    semantic = *tt;
                }
                if sev.is_some() {
                    diag_severity = *sev;
                }
            }
        }

        let mut next_boundary = line.len();
        for (start, end, _, _) in &all_segments {
            if *start > i && *start < next_boundary {
                next_boundary = *start;
            }
            if *end > i && *end < next_boundary {
                next_boundary = *end;
            }
        }

        let text = match line.get(i..next_boundary) {
            Some(t) => t,
            None => {
                tracing::error!(
                    "HighlightedLine: 无效的 UTF-8 切片范围 line[{}..{}], line.len()={}, line={:?}",
                    i,
                    next_boundary,
                    line.len(),
                    line
                );
                let error_msg = format!(
                    "LSP 位置偏移与文本编码不匹配\n\n行切片范围: [{}..{}]\n行长度: {}\n行内容: {}",
                    i,
                    next_boundary,
                    line.len(),
                    line
                );
                let mut error_signal = use_context::<EditorErrorSignal>();
                error_signal.set(Some(error_msg));
                navigator().push(Route::EditorError {});
                return rsx! {
                    span { "{line}" }
                };
            }
        };
        let class = build_span_class(semantic, diag_severity);

        spans.push(rsx! {
            span { class, "{text}" }
        });

        i = next_boundary;
    }

    rsx! {
        for span in spans {
            {span}
        }
    }
}

fn build_span_class(
    semantic: Option<SemanticTokenType>,
    diag: Option<DiagnosticSeverity>,
) -> String {
    let mut classes = String::new();
    if let Some(tt) = semantic {
        if !classes.is_empty() {
            classes.push(' ');
        }
        classes.push_str(token_class(tt));
    }
    if let Some(sev) = diag {
        if !classes.is_empty() {
            classes.push(' ');
        }
        classes.push_str(diag_class_static(sev));
    }
    classes
}

fn diag_class_static(severity: DiagnosticSeverity) -> &'static str {
    match severity {
        DiagnosticSeverity::Error => styles::diag_error,
        DiagnosticSeverity::Warning => styles::diag_warning,
        DiagnosticSeverity::Information => styles::diag_info,
        DiagnosticSeverity::Hint => styles::diag_hint,
    }
}
