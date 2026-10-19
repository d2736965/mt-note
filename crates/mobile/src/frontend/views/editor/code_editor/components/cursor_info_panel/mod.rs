use std::rc::Rc;

use dioxus::logger::tracing;
use dioxus::prelude::*;
use mt_note_editor_lsp::types::{DiagnosticSeverity, HoverInfo, LspDiagnostic};
use serde_json;
use ui::components::icons::ContentCopyIcon;

use crate::frontend::js_bridge::{DioxusBridge, js_api};

use super::super::styles;

fn severity_icon(severity: DiagnosticSeverity) -> &'static str {
    match severity {
        DiagnosticSeverity::Error => "✕",
        DiagnosticSeverity::Warning => "⚠",
        DiagnosticSeverity::Information => "ℹ",
        DiagnosticSeverity::Hint => "💡",
    }
}

fn severity_class(severity: DiagnosticSeverity) -> &'static str {
    match severity {
        DiagnosticSeverity::Error => styles::diag_msg_error,
        DiagnosticSeverity::Warning => styles::diag_msg_warning,
        DiagnosticSeverity::Information => styles::diag_msg_info,
        DiagnosticSeverity::Hint => styles::diag_msg_hint,
    }
}

fn build_copy_text(
    diagnostics: &[LspDiagnostic],
    hover_info: Option<&HoverInfo>,
    show_diag: bool,
    show_hover: bool,
) -> String {
    let mut parts = Vec::new();
    if show_diag {
        for d in diagnostics {
            let label = match d.severity {
                DiagnosticSeverity::Error => "error",
                DiagnosticSeverity::Warning => "warning",
                DiagnosticSeverity::Information => "info",
                DiagnosticSeverity::Hint => "hint",
            };
            parts.push(format!("[{label}] {}", d.message));
        }
    }
    if show_hover && let Some(h) = hover_info {
        parts.push(h.contents.clone());
    }
    parts.join("\n")
}

async fn read_visible_bounds() -> Result<(f64, f64), String> {
    let json_str = js_api::editor::get_visible_bounds::<DioxusBridge>().await;
    let v: serde_json::Value = serde_json::from_str(&json_str).unwrap_or_default();
    if v.is_null() {
        return Err("visualViewport info unavailable".into());
    }
    let visible_top = v["visible_top"].as_f64().ok_or("visible_top")?;
    let visible_bottom = v["visible_bottom"].as_f64().ok_or("visible_bottom")?;
    Ok((visible_top, visible_bottom))
}

#[component]
pub fn CursorInfoPanel(
    x: f64,
    cursor_y: f64,
    line_height: f64,
    hover_info: Option<HoverInfo>,
    diagnostics: Vec<LspDiagnostic>,
    show_hover: bool,
    show_diag: bool,
) -> Element {
    let has_hover = show_hover && hover_info.is_some();
    let has_diag = show_diag && !diagnostics.is_empty();

    if !has_hover && !has_diag {
        return rsx! {};
    }

    let copy_text = build_copy_text(&diagnostics, hover_info.as_ref(), show_diag, show_hover);
    let copied: Signal<bool> = use_signal(|| false);
    let mut cursor_y_sig: Signal<f64> = use_signal(|| cursor_y);
    let mut line_height_sig: Signal<f64> = use_signal(|| line_height);
    if (cursor_y_sig() - cursor_y).abs() > f64::EPSILON {
        cursor_y_sig.set(cursor_y);
    }
    if (line_height_sig() - line_height).abs() > f64::EPSILON {
        line_height_sig.set(line_height);
    }
    let panel_y: Signal<f64> = use_signal(|| cursor_y);
    let mut panel_el: Signal<Option<Rc<MountedData>>> = use_signal(|| None);

    {
        use_effect(move || {
            let cy = cursor_y_sig();
            let lh = line_height_sig();
            let mut panel_y = panel_y;
            let panel_el = panel_el;
            spawn(async move {
                let el = loop {
                    tokio::time::sleep(std::time::Duration::from_millis(16)).await;
                    if let Some(e) = panel_el.cloned() {
                        break e;
                    }
                };
                let panel_height = match el.get_client_rect().await {
                    Ok(rect) => rect.height(),
                    Err(e) => {
                        tracing::error!("get_client_rect failed: {e}");
                        0.0
                    }
                };
                if panel_height <= 0.0 {
                    panel_y.set(cy + lh);
                } else if let Ok((visible_top, visible_bottom)) = read_visible_bounds().await {
                    let cursor_bottom = cy + lh;
                    let space_below = visible_bottom - cursor_bottom;
                    let space_above = cy - visible_top;
                    if space_below >= panel_height {
                        panel_y.set(cy + lh);
                    } else if space_above >= panel_height {
                        panel_y.set(cy - panel_height);
                    } else {
                        let clamped_top = (visible_bottom - panel_height).max(visible_top);
                        panel_y.set(clamped_top);
                    }
                } else {
                    panel_y.set(cy + lh);
                }
            });
        });
    }

    rsx! {
        div {
            class: styles::cursor_info_panel,
            style: "left: {x}px; top: {panel_y()}px;",
            onmounted: move |e: MountedEvent| {
                panel_el.set(Some(e.data()));
            },
            div { class: styles::cursor_info_header,
                button {
                    class: styles::cursor_info_copy_btn,
                    onclick: {
                        let copy_text = copy_text.clone();
                        let mut copied = copied;
                        move |_| {
                            let text = copy_text.clone();
                            js_api::editor::clipboard_write::<DioxusBridge>(&text);
                            spawn(async move {
                                copied.set(true);
                                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                                copied.set(false);
                            });
                        }
                    },
                    if copied() {
                        "✓"
                    } else {
                        ContentCopyIcon {}
                    }
                }
            }
            if has_diag {
                div { class: styles::menu_diag_section,
                    for diag in &diagnostics {
                        div { class: severity_class(diag.severity),
                            span { class: styles::diag_msg_icon, "{severity_icon(diag.severity)}" }
                            "{diag.message}"
                        }
                    }
                }
            }
            if has_hover {
                if let Some(hover) = hover_info {
                    div { class: styles::cursor_hover_preview, "{hover.contents}" }
                }
            }
        }
    }
}
