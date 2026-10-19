use dioxus::prelude::*;
use mt_note_editor_lsp::types::{DiagnosticSeverity, LspDiagnostic, TextRange};

use super::super::styles;

#[component]
pub fn DiagnosticsLayer(
    line: String,
    line_index: usize,
    diagnostics: Vec<LspDiagnostic>,
) -> Element {
    if line.is_empty() {
        return rsx! { "" };
    }

    let line_diags: Vec<&LspDiagnostic> = diagnostics
        .iter()
        .filter(|d| line_index >= d.range.start.line && line_index <= d.range.end.line)
        .collect();

    if line_diags.is_empty() {
        return rsx! {
            span { "{line}" }
        };
    }

    let mut spans = Vec::new();
    let mut last_end = 0usize;

    let mut sorted_ranges: Vec<(usize, usize, &LspDiagnostic)> = line_diags
        .iter()
        .map(|d| {
            let start = if d.range.start.line == line_index {
                d.range.start.column
            } else {
                0
            };
            let end = if d.range.end.line == line_index {
                d.range.end.column.min(line.len())
            } else {
                line.len()
            };
            (start, end, *d)
        })
        .collect();
    sorted_ranges.sort_by_key(|(s, _, _)| *s);

    for (start, end, diag) in &sorted_ranges {
        let start = *start;
        let end = (*end).min(line.len());

        if start > last_end {
            let plain = &line[last_end..start];
            spans.push(rsx! {
                span { "{plain}" }
            });
        }

        if start < end {
            let text = &line[start..end];
            let class = diag_class(diag.severity);
            let title = diag.message.clone();
            spans.push(rsx! {
                span { class, title, "{text}" }
            });
        }

        last_end = last_end.max(end);
    }

    if last_end < line.len() {
        let plain = &line[last_end..];
        spans.push(rsx! {
            span { "{plain}" }
        });
    }

    rsx! {
        for span in spans {
            {span}
        }

    }
}

#[expect(dead_code, reason = "保留给后续独立诊断渲染场景使用")]
fn diag_class(severity: DiagnosticSeverity) -> &'static str {
    match severity {
        DiagnosticSeverity::Error => styles::diag_error,
        DiagnosticSeverity::Warning => styles::diag_warning,
        DiagnosticSeverity::Information => styles::diag_info,
        DiagnosticSeverity::Hint => styles::diag_hint,
    }
}

#[expect(dead_code, reason = "保留给后续独立诊断渲染场景使用")]
pub fn line_has_diagnostic(line_index: usize, diagnostics: &[LspDiagnostic]) -> bool {
    diagnostics
        .iter()
        .any(|d| line_index >= d.range.start.line && line_index <= d.range.end.line)
}

#[expect(dead_code, reason = "保留给后续独立诊断渲染场景使用")]
pub fn line_diagnostic_message(
    line_index: usize,
    range: TextRange,
    diagnostics: &[LspDiagnostic],
) -> Option<String> {
    diagnostics
        .iter()
        .filter(|d| d.range == range)
        .find(|d| line_index >= d.range.start.line && line_index <= d.range.end.line)
        .map(|d| d.message.clone())
}
