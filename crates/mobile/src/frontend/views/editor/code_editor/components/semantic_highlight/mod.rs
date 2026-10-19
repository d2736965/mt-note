use dioxus::logger::tracing;
use dioxus::prelude::*;
use mt_note_editor_lsp::types::SemanticTokenType;

use super::super::styles;
use crate::frontend::router::Route;
use crate::frontend::views::editor_error::EditorErrorSignal;

#[component]
pub fn SemanticSpans(
    line: String,
    semantic_tokens: Vec<(usize, usize, SemanticTokenType)>,
) -> Element {
    if line.is_empty() {
        return rsx! { "" };
    }

    if semantic_tokens.is_empty() {
        return rsx! {
            span { "{line}" }
        };
    }

    let mut spans = Vec::new();
    let mut last_end = 0usize;

    for (start, end, token_type) in &semantic_tokens {
        let start = *start;
        let end = (*end).min(line.len());

        if start > last_end {
            let plain = match line.get(last_end..start) {
                Some(t) => t,
                None => {
                    tracing::error!(
                        "SemanticSpans: 无效的 UTF-8 切片范围 line[{}..{}], line.len()={}",
                        last_end,
                        start,
                        line.len()
                    );
                    let error_msg = format!(
                        "LSP 位置偏移与文本编码不匹配\n\n行切片范围: [{}..{}]\n行长度: {}\n行内容: {}",
                        last_end,
                        start,
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
            spans.push(rsx! {
                span { "{plain}" }
            });
        }

        if start < end {
            let text = match line.get(start..end) {
                Some(t) => t,
                None => {
                    tracing::error!(
                        "SemanticSpans: 无效的 UTF-8 切片范围 line[{}..{}], line.len()={}",
                        start,
                        end,
                        line.len()
                    );
                    let error_msg = format!(
                        "LSP 位置偏移与文本编码不匹配\n\n行切片范围: [{}..{}]\n行长度: {}\n行内容: {}",
                        start,
                        end,
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
            let class = token_class(*token_type);
            spans.push(rsx! {
                span { class, "{text}" }
            });
        }

        last_end = end;
    }

    if last_end < line.len() {
        let plain = match line.get(last_end..) {
            Some(t) => t,
            None => {
                tracing::error!(
                    "SemanticSpans: 无效的 UTF-8 切片范围 line[{}..], line.len()={}",
                    last_end,
                    line.len()
                );
                let error_msg = format!(
                    "LSP 位置偏移与文本编码不匹配\n\n行切片范围: [{}..]\n行长度: {}\n行内容: {}",
                    last_end,
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

pub fn token_class(token_type: SemanticTokenType) -> &'static str {
    match token_type {
        SemanticTokenType::Namespace => styles::token_namespace,
        SemanticTokenType::Type => styles::token_type,
        SemanticTokenType::Class => styles::token_class,
        SemanticTokenType::Enum => styles::token_enum,
        SemanticTokenType::Interface => styles::token_interface,
        SemanticTokenType::Struct => styles::token_struct,
        SemanticTokenType::TypeParameter => styles::token_type_parameter,
        SemanticTokenType::Parameter => styles::token_parameter,
        SemanticTokenType::Variable => styles::token_variable,
        SemanticTokenType::Property => styles::token_property,
        SemanticTokenType::EnumMember => styles::token_enum_member,
        SemanticTokenType::Function => styles::token_function,
        SemanticTokenType::Method => styles::token_method,
        SemanticTokenType::Macro => styles::token_macro,
        SemanticTokenType::Keyword => styles::token_keyword,
        SemanticTokenType::Modifier => styles::token_modifier,
        SemanticTokenType::Comment => styles::token_comment,
        SemanticTokenType::String => styles::token_string,
        SemanticTokenType::Number => styles::token_number,
        SemanticTokenType::Regexp => styles::token_regexp,
        SemanticTokenType::Operator => styles::token_operator,
        SemanticTokenType::Decorator => styles::token_decorator,
    }
}
