use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::EditorLsp;
use crate::types::*;
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

struct MarkdownDoc {
    text: String,
    semantic_tokens: Vec<SemanticToken>,
    diagnostics: Vec<LspDiagnostic>,
    document_symbols: Vec<DocumentSymbol>,
    folding_ranges: Vec<FoldingRange>,
}

pub struct MarkdownLsp {
    docs: HashMap<PathBuf, MarkdownDoc>,
}

impl MarkdownLsp {
    pub fn new() -> Self {
        Self {
            docs: HashMap::new(),
        }
    }
}

impl Default for MarkdownLsp {
    fn default() -> Self {
        Self::new()
    }
}

impl EditorLsp for MarkdownLsp {
    fn language_id(&self) -> &'static str {
        "markdown"
    }

    fn update(&mut self, path: &Path, content: &str) {
        let text = content.to_string();

        let events: Vec<(Event<'_>, std::ops::Range<usize>)> =
            Parser::new_ext(&text, Options::all())
                .into_offset_iter()
                .collect();

        let ((semantic_tokens, diagnostics), (document_symbols, folding_ranges)) = rayon::join(
            || {
                rayon::join(
                    || compute_semantic_tokens(&text, &events),
                    || compute_diagnostics(&text, &events),
                )
            },
            || {
                rayon::join(
                    || compute_document_symbols(&text, &events),
                    || compute_folding_ranges(&text, &events),
                )
            },
        );

        self.docs.insert(
            path.to_path_buf(),
            MarkdownDoc {
                text,
                semantic_tokens,
                diagnostics,
                document_symbols,
                folding_ranges,
            },
        );
    }

    fn close(&mut self, path: &Path) {
        self.docs.remove(path);
    }

    fn semantic_tokens(&self, path: &Path) -> Vec<SemanticToken> {
        match self.docs.get(path) {
            Some(doc) => doc.semantic_tokens.clone(),
            None => Vec::new(),
        }
    }

    fn diagnostics(&self, path: &Path) -> Vec<LspDiagnostic> {
        match self.docs.get(path) {
            Some(doc) => doc.diagnostics.clone(),
            None => Vec::new(),
        }
    }

    fn completions(&self, _path: &Path, _pos: TextPosition) -> Vec<Completion> {
        Vec::new()
    }

    fn hover(&self, path: &Path, pos: TextPosition) -> Option<HoverInfo> {
        let doc = self.docs.get(path)?;
        let offset = line_col_to_offset(&doc.text, pos.line, pos.column)?;
        let events: Vec<(Event<'_>, std::ops::Range<usize>)> =
            Parser::new_ext(&doc.text, Options::all())
                .into_offset_iter()
                .collect();

        find_hover(&events, offset)
    }

    fn goto_definition(&self, _path: &Path, _pos: TextPosition) -> Option<Location> {
        None
    }

    fn references(&self, _path: &Path, _pos: TextPosition) -> Vec<Location> {
        Vec::new()
    }

    fn signature_help(&self, _path: &Path, _pos: TextPosition) -> Option<SignatureHelp> {
        None
    }

    fn document_symbols(&self, path: &Path) -> Vec<DocumentSymbol> {
        match self.docs.get(path) {
            Some(doc) => doc.document_symbols.clone(),
            None => Vec::new(),
        }
    }

    fn folding_ranges(&self, path: &Path) -> Vec<FoldingRange> {
        match self.docs.get(path) {
            Some(doc) => doc.folding_ranges.clone(),
            None => Vec::new(),
        }
    }

    fn code_actions(&self, _path: &Path, _range: TextRange) -> Vec<CodeAction> {
        Vec::new()
    }

    fn document_highlight(&self, _path: &Path, _pos: TextPosition) -> Vec<TextRange> {
        Vec::new()
    }

    fn rename(&self, _path: &Path, _pos: TextPosition, _new_name: &str) -> Vec<TextEdit> {
        Vec::new()
    }

    fn inlay_hints(&self, _path: &Path) -> Vec<InlayHint> {
        Vec::new()
    }
}

fn byte_offset_to_position(text: &str, offset: usize) -> Option<TextPosition> {
    if offset > text.len() {
        return None;
    }
    let line = text[..offset].chars().filter(|&c| c == '\n').count();
    let last_newline = match text[..offset].rfind('\n') {
        Some(i) => i + 1,
        None => 0,
    };
    let column = offset - last_newline;
    Some(TextPosition { line, column })
}

fn line_col_to_offset(text: &str, line: usize, column: usize) -> Option<usize> {
    let mut current_line = 0;
    let mut line_start = 0;
    for (i, ch) in text.char_indices() {
        if current_line == line {
            let offset = line_start + column;
            if offset <= text.len() {
                return Some(offset);
            }
            return None;
        }
        if ch == '\n' {
            current_line += 1;
            line_start = i + 1;
        }
    }
    if current_line == line {
        let offset = line_start + column;
        if offset <= text.len() {
            return Some(offset);
        }
    }
    None
}

fn compute_semantic_tokens(
    text: &str,
    events: &[(Event<'_>, std::ops::Range<usize>)],
) -> Vec<SemanticToken> {
    #[derive(Clone)]
    struct RawToken {
        line: usize,
        column: usize,
        length: usize,
        token_type: SemanticTokenType,
        modifiers: Vec<SemanticTokenModifier>,
    }

    let mut raw_tokens: Vec<RawToken> = Vec::new();
    let mut tag_stack: Vec<Tag<'_>> = Vec::new();

    for (event, range) in events {
        match event {
            Event::Start(tag) => {
                tag_stack.push(tag.clone());
            }
            Event::End(_) => {
                tag_stack.pop();
            }
            Event::Text(_s) | Event::Code(_s) => {
                let token_type = current_token_type(&tag_stack);
                let modifiers = current_modifiers(&tag_stack);
                let pos = match byte_offset_to_position(text, range.start) {
                    Some(p) => p,
                    None => continue,
                };
                let len = range.end - range.start;
                raw_tokens.push(RawToken {
                    line: pos.line,
                    column: pos.column,
                    length: len,
                    token_type,
                    modifiers,
                });
            }
            _ => continue,
        }
    }

    raw_tokens.sort_by(|a, b| a.line.cmp(&b.line).then(a.column.cmp(&b.column)));

    let mut prev_line = 0u32;
    let mut prev_col = 0u32;

    raw_tokens
        .into_iter()
        .map(|t| {
            let line = t.line as u32;
            let col = t.column as u32;
            let delta_line = line.saturating_sub(prev_line);
            let delta_start = if delta_line == 0 {
                col.saturating_sub(prev_col)
            } else {
                col
            };
            prev_line = line;
            prev_col = col;
            SemanticToken {
                delta_line,
                delta_start,
                length: t.length as u32,
                token_type: t.token_type,
                modifiers: t.modifiers,
            }
        })
        .collect()
}

fn current_token_type(tag_stack: &[Tag<'_>]) -> SemanticTokenType {
    for tag in tag_stack.iter().rev() {
        match tag {
            Tag::Heading { .. } => return SemanticTokenType::Keyword,
            Tag::CodeBlock(_) => return SemanticTokenType::String,
            Tag::Link { .. } => return SemanticTokenType::Function,
            Tag::Image { .. } => return SemanticTokenType::Function,
            Tag::BlockQuote(_) => return SemanticTokenType::Comment,
            Tag::MetadataBlock(_) => return SemanticTokenType::Comment,
            _ => continue,
        }
    }
    SemanticTokenType::Variable
}

fn current_modifiers(tag_stack: &[Tag<'_>]) -> Vec<SemanticTokenModifier> {
    let mut mods = Vec::new();
    for tag in tag_stack.iter() {
        match tag {
            Tag::Emphasis => mods.push(SemanticTokenModifier::Modification),
            Tag::Strong => mods.push(SemanticTokenModifier::Declaration),
            Tag::Strikethrough => mods.push(SemanticTokenModifier::Deprecated),
            _ => continue,
        }
    }
    mods
}

fn compute_diagnostics(
    _text: &str,
    _events: &[(Event<'_>, std::ops::Range<usize>)],
) -> Vec<LspDiagnostic> {
    Vec::new()
}

#[derive(Debug)]
struct HeadingEntry {
    name: String,
    level: usize,
    start_line: usize,
    end_line: usize,
}

fn compute_document_symbols(
    text: &str,
    events: &[(Event<'_>, std::ops::Range<usize>)],
) -> Vec<DocumentSymbol> {
    let mut headings: Vec<HeadingEntry> = Vec::new();
    let mut tag_stack: Vec<Tag<'_>> = Vec::new();
    let mut current_heading_name = String::new();
    let mut in_heading = false;

    for (event, range) in events {
        match event {
            Event::Start(tag) => {
                if let Tag::Heading { level, .. } = tag {
                    in_heading = true;
                    current_heading_name.clear();
                    let pos = match byte_offset_to_position(text, range.start) {
                        Some(p) => p,
                        None => {
                            tag_stack.push(tag.clone());
                            continue;
                        }
                    };
                    let level = match level {
                        HeadingLevel::H1 => 1,
                        HeadingLevel::H2 => 2,
                        HeadingLevel::H3 => 3,
                        HeadingLevel::H4 => 4,
                        HeadingLevel::H5 => 5,
                        HeadingLevel::H6 => 6,
                    };
                    headings.push(HeadingEntry {
                        name: String::new(),
                        level,
                        start_line: pos.line,
                        end_line: pos.line,
                    });
                }
                tag_stack.push(tag.clone());
            }
            Event::End(_tag) => {
                if in_heading && matches!(tag_stack.last(), Some(Tag::Heading { .. })) {
                    in_heading = false;
                    if let Some(last) = headings.last_mut() {
                        last.name = std::mem::take(&mut current_heading_name);
                    }
                }
                tag_stack.pop();
            }
            Event::Text(s) | Event::Code(s) if in_heading => {
                current_heading_name.push_str(s);
            }
            _ => continue,
        }
    }

    if headings.is_empty() {
        return Vec::new();
    }

    for i in 0..headings.len().saturating_sub(1) {
        headings[i].end_line = headings[i + 1].start_line.saturating_sub(1);
    }
    if let Some(last) = headings.last_mut() {
        let total_lines = text.chars().filter(|&c| c == '\n').count();
        last.end_line = total_lines;
    }

    let mut all_symbols = Vec::new();
    let mut idx = 0;
    while idx < headings.len() {
        if let Some((symbols, consumed)) = build_symbol_tree(&headings, idx) {
            all_symbols.extend(symbols);
            idx += consumed;
        } else {
            idx += 1;
        }
    }
    all_symbols
}

fn build_symbol_tree(
    headings: &[HeadingEntry],
    index: usize,
) -> Option<(Vec<DocumentSymbol>, usize)> {
    if index >= headings.len() {
        return None;
    }

    let mut children = Vec::new();
    let mut i = index;
    let current = &headings[i];

    if current.end_line < current.start_line {
        return None;
    }

    i += 1;
    while i < headings.len() {
        let next = &headings[i];
        if next.level <= current.level {
            break;
        }
        if let Some((sub_children, consumed)) = build_symbol_tree(headings, i) {
            children.extend(sub_children);
            i += consumed;
        } else {
            i += 1;
        }
    }

    let symbol = DocumentSymbol {
        name: current.name.clone(),
        detail: None,
        kind: SymbolKind::String,
        range: TextRange {
            start: TextPosition {
                line: current.start_line,
                column: 0,
            },
            end: TextPosition {
                line: current.end_line,
                column: 0,
            },
        },
        selection_range: TextRange {
            start: TextPosition {
                line: current.start_line,
                column: 0,
            },
            end: TextPosition {
                line: current.start_line,
                column: 0,
            },
        },
        children,
    };

    Some((vec![symbol], i - index))
}

fn compute_folding_ranges(
    text: &str,
    events: &[(Event<'_>, std::ops::Range<usize>)],
) -> Vec<FoldingRange> {
    let mut ranges = Vec::new();
    let mut heading_stack: Vec<(usize, usize)> = Vec::new();

    for (event, range) in events {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                let pos = match byte_offset_to_position(text, range.start) {
                    Some(p) => p,
                    None => continue,
                };
                let lvl = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };

                while let Some(&(top_lvl, top_line)) = heading_stack.last() {
                    if top_lvl >= lvl {
                        let end = pos.line.saturating_sub(1);
                        if end > top_line {
                            ranges.push(FoldingRange {
                                start_line: top_line,
                                end_line: end,
                                kind: Some(FoldingRangeKind::Region),
                            });
                        }
                        heading_stack.pop();
                    } else {
                        break;
                    }
                }

                heading_stack.push((lvl, pos.line));
            }
            Event::Start(Tag::CodeBlock(_)) => {
                let pos = match byte_offset_to_position(text, range.start) {
                    Some(p) => p,
                    None => continue,
                };
                heading_stack.push((0, pos.line));
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(&(0, start_line)) = heading_stack.last() {
                    let pos = match byte_offset_to_position(text, range.end) {
                        Some(p) => p,
                        None => continue,
                    };
                    let end_line = pos.line;
                    if end_line > start_line {
                        ranges.push(FoldingRange {
                            start_line,
                            end_line,
                            kind: Some(FoldingRangeKind::Region),
                        });
                    }
                    heading_stack.pop();
                }
            }
            _ => continue,
        }
    }

    let total_lines = text.chars().filter(|&c| c == '\n').count();
    while let Some(&(_lvl, start_line)) = heading_stack.last() {
        if total_lines > start_line {
            ranges.push(FoldingRange {
                start_line,
                end_line: total_lines,
                kind: Some(FoldingRangeKind::Region),
            });
        }
        heading_stack.pop();
    }

    ranges
}

fn find_hover(events: &[(Event<'_>, std::ops::Range<usize>)], offset: usize) -> Option<HoverInfo> {
    let mut tag_stack: Vec<Tag<'_>> = Vec::new();

    for (event, range) in events {
        if let Event::Start(tag) = event {
            tag_stack.push(tag.clone());
        }
        if let Event::End(_) = event {
            tag_stack.pop();
        }

        if !range.contains(&offset) {
            continue;
        }

        for tag in tag_stack.iter().rev() {
            match tag {
                Tag::Link {
                    dest_url, title, ..
                } => {
                    let mut contents = dest_url.to_string();
                    if !title.is_empty() {
                        contents.push_str(&format!("\n\n{title}"));
                    }
                    return Some(HoverInfo {
                        contents,
                        range: None,
                    });
                }
                Tag::Image {
                    dest_url, title, ..
                } => {
                    let mut contents = format!("Image: {dest_url}");
                    if !title.is_empty() {
                        contents.push_str(&format!("\n\n{title}"));
                    }
                    return Some(HoverInfo {
                        contents,
                        range: None,
                    });
                }
                Tag::CodeBlock(kind) => {
                    let lang = match kind {
                        pulldown_cmark::CodeBlockKind::Fenced(lang) if !lang.is_empty() => {
                            format!("Language: {lang}")
                        }
                        _ => "Code Block".to_string(),
                    };
                    return Some(HoverInfo {
                        contents: lang,
                        range: None,
                    });
                }
                Tag::Heading { level, id, .. } => {
                    let level_str = match level {
                        HeadingLevel::H1 => "H1",
                        HeadingLevel::H2 => "H2",
                        HeadingLevel::H3 => "H3",
                        HeadingLevel::H4 => "H4",
                        HeadingLevel::H5 => "H5",
                        HeadingLevel::H6 => "H6",
                    };
                    let mut contents = format!("Heading {level_str}");
                    if let Some(ref id) = *id {
                        contents.push_str(&format!("\nID: #{id}"));
                    }
                    return Some(HoverInfo {
                        contents,
                        range: None,
                    });
                }
                _ => continue,
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_md(text: &str) -> Vec<(Event<'_>, std::ops::Range<usize>)> {
        Parser::new_ext(text, Options::all())
            .into_offset_iter()
            .collect()
    }

    fn decode_tokens(tokens: &[SemanticToken]) -> Vec<(usize, usize, usize, SemanticTokenType)> {
        let mut result = Vec::new();
        let mut line = 0u32;
        let mut col = 0u32;
        for t in tokens {
            if t.delta_line > 0 {
                line += t.delta_line;
                col = t.delta_start;
            } else {
                col += t.delta_start;
            }
            result.push((line as usize, col as usize, t.length as usize, t.token_type));
        }
        result
    }

    #[test]
    fn offset_pos_single_line() {
        let pos = byte_offset_to_position("hello", 2).unwrap();
        assert_eq!(pos.line, 0);
        assert_eq!(pos.column, 2);
    }

    #[test]
    fn offset_pos_multi_line() {
        let pos = byte_offset_to_position("a\nbc\ndef", 5).unwrap();
        assert_eq!(pos.line, 2);
        assert_eq!(pos.column, 0);
    }

    #[test]
    fn offset_pos_out_of_range() {
        assert!(byte_offset_to_position("ab", 3).is_none());
    }

    #[test]
    fn offset_pos_at_end() {
        let pos = byte_offset_to_position("ab", 2).unwrap();
        assert_eq!(pos.line, 0);
        assert_eq!(pos.column, 2);
    }

    #[test]
    fn offset_pos_multi_byte_chars() {
        let pos = byte_offset_to_position("你好世界", 6).unwrap();
        assert_eq!(pos.line, 0);
        assert_eq!(pos.column, 2);
    }

    #[test]
    fn lc_offset_roundtrip() {
        let text = "line1\nline2\nline3";
        for offset in 0..=text.len() {
            let pos = byte_offset_to_position(text, offset).unwrap();
            let back = line_col_to_offset(text, pos.line, pos.column).unwrap();
            assert_eq!(back, offset, "roundtrip failed at offset {offset}");
        }
    }

    #[test]
    fn semantic_heading() {
        let text = "# Hello";
        let events = parse_md(text);
        let tokens = compute_semantic_tokens(text, &events);
        let decoded = decode_tokens(&tokens);
        let heading_token = decoded
            .iter()
            .find(|t| t.3 == SemanticTokenType::Keyword)
            .unwrap();
        assert_eq!(heading_token.0, 0);
        assert_eq!(heading_token.1, 2);
        assert_eq!(heading_token.2, 5);
    }

    #[test]
    fn semantic_code_block() {
        let text = "```rust\nlet x = 1;\n```";
        let events = parse_md(text);
        let tokens = compute_semantic_tokens(text, &events);
        let decoded = decode_tokens(&tokens);
        let code_tokens: Vec<_> = decoded
            .iter()
            .filter(|t| t.3 == SemanticTokenType::String)
            .collect();
        assert!(!code_tokens.is_empty());
    }

    #[test]
    fn semantic_plain_text() {
        let text = "just plain text";
        let events = parse_md(text);
        let tokens = compute_semantic_tokens(text, &events);
        let decoded = decode_tokens(&tokens);
        assert!(decoded.iter().all(|t| t.3 == SemanticTokenType::Variable));
    }

    #[test]
    fn semantic_emphasis() {
        let text = "*italic*";
        let events = parse_md(text);
        let tokens = compute_semantic_tokens(text, &events);
        let italic = tokens
            .iter()
            .find(|t| t.modifiers.contains(&SemanticTokenModifier::Modification))
            .unwrap();
        assert_eq!(italic.length, 6);
    }

    #[test]
    fn semantic_strong() {
        let text = "**bold**";
        let events = parse_md(text);
        let tokens = compute_semantic_tokens(text, &events);
        let bold = tokens
            .iter()
            .find(|t| t.modifiers.contains(&SemanticTokenModifier::Declaration))
            .unwrap();
        assert_eq!(bold.length, 4);
    }

    #[test]
    fn symbols_single_h1() {
        let text = "# Title\ncontent";
        let events = parse_md(text);
        let symbols = compute_document_symbols(text, &events);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "Title");
    }

    #[test]
    fn symbols_nested() {
        let text = "# H1\n## H2\n### H3\n## H2b\n# H1b";
        let events = parse_md(text);
        let symbols = compute_document_symbols(text, &events);
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "H1");
        assert_eq!(symbols[0].children.len(), 2);
        assert_eq!(symbols[0].children[0].name, "H2");
        assert_eq!(symbols[0].children[0].children[0].name, "H3");
        assert_eq!(symbols[1].name, "H1b");
    }

    #[test]
    fn folding_headings() {
        let text = "# H1\ncontent\n## H2\nmore\n# H3\nend";
        let events = parse_md(text);
        let ranges = compute_folding_ranges(text, &events);
        assert!(!ranges.is_empty());
    }

    #[test]
    fn folding_code_block() {
        let text = "text\n```rust\nlet x = 1;\nlet y = 2;\n```\nmore";
        let events = parse_md(text);
        let ranges = compute_folding_ranges(text, &events);
        let code_fold = ranges.iter().find(|r| r.start_line == 1).unwrap();
        assert_eq!(code_fold.end_line, 4);
    }

    #[test]
    fn hover_link() {
        let text = "[click](https://example.com)";
        let events = parse_md(text);
        let hover = find_hover(&events, 3).unwrap();
        assert!(hover.contents.contains("https://example.com"));
    }

    #[test]
    fn hover_code_block() {
        let text = "```rust\nlet x = 1;\n```";
        let events = parse_md(text);
        let hover = find_hover(&events, 10).unwrap();
        assert!(hover.contents.contains("rust"));
    }

    #[test]
    fn hover_heading() {
        let text = "# Title";
        let events = parse_md(text);
        let hover = find_hover(&events, 2).unwrap();
        assert!(hover.contents.contains("H1"));
    }

    #[test]
    fn lsp_update_and_semantic_tokens() {
        let mut lsp = MarkdownLsp::new();
        let path = Path::new("test.md");
        lsp.update(path, "# Hello\n\nworld");
        let tokens = lsp.semantic_tokens(path);
        assert!(!tokens.is_empty());
        let decoded = decode_tokens(&tokens);
        assert!(decoded.iter().any(|t| t.3 == SemanticTokenType::Keyword));
    }

    #[test]
    fn lsp_update_and_document_symbols() {
        let mut lsp = MarkdownLsp::new();
        let path = Path::new("test.md");
        lsp.update(path, "# H1\n## H2\ncontent");
        let symbols = lsp.document_symbols(path);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "H1");
        assert_eq!(symbols[0].children[0].name, "H2");
    }

    #[test]
    fn lsp_update_and_hover() {
        let mut lsp = MarkdownLsp::new();
        let path = Path::new("test.md");
        lsp.update(path, "[link](https://example.com)");
        let hover = lsp
            .hover(path, TextPosition { line: 0, column: 3 })
            .unwrap();
        assert!(hover.contents.contains("https://example.com"));
    }

    #[test]
    fn lsp_close_removes_doc() {
        let mut lsp = MarkdownLsp::new();
        let path = Path::new("test.md");
        lsp.update(path, "# Hello");
        assert!(!lsp.semantic_tokens(path).is_empty());
        lsp.close(path);
        assert!(lsp.semantic_tokens(path).is_empty());
    }

    #[test]
    fn lsp_language_id() {
        let lsp = MarkdownLsp::new();
        assert_eq!(lsp.language_id(), "markdown");
    }

    #[test]
    fn lsp_unknown_file_returns_empty() {
        let lsp = MarkdownLsp::new();
        let path = Path::new("nonexistent.md");
        assert!(lsp.semantic_tokens(path).is_empty());
        assert!(lsp.document_symbols(path).is_empty());
        assert!(lsp.folding_ranges(path).is_empty());
        assert!(
            lsp.hover(path, TextPosition { line: 0, column: 0 })
                .is_none()
        );
    }

    #[test]
    fn semantic_link() {
        let text = "[click](https://example.com)";
        let events = parse_md(text);
        let tokens = compute_semantic_tokens(text, &events);
        let decoded = decode_tokens(&tokens);
        let link_token = decoded
            .iter()
            .find(|t| t.3 == SemanticTokenType::Function)
            .unwrap();
        assert_eq!(link_token.2, 5);
    }

    #[test]
    fn semantic_image() {
        let text = "![alt](img.png)";
        let events = parse_md(text);
        let tokens = compute_semantic_tokens(text, &events);
        let decoded = decode_tokens(&tokens);
        let img_token = decoded
            .iter()
            .find(|t| t.3 == SemanticTokenType::Function)
            .unwrap();
        assert_eq!(img_token.2, 3);
    }

    #[test]
    fn semantic_blockquote() {
        let text = "> quoted";
        let events = parse_md(text);
        let tokens = compute_semantic_tokens(text, &events);
        let decoded = decode_tokens(&tokens);
        let quote_token = decoded
            .iter()
            .find(|t| t.3 == SemanticTokenType::Comment)
            .unwrap();
        assert_eq!(quote_token.2, 6);
    }

    #[test]
    fn semantic_strikethrough() {
        let text = "~~deleted~~";
        let events = parse_md(text);
        let tokens = compute_semantic_tokens(text, &events);
        let strike = tokens
            .iter()
            .find(|t| t.modifiers.contains(&SemanticTokenModifier::Deprecated))
            .unwrap();
        assert_eq!(strike.length, 7);
    }

    #[test]
    fn symbols_empty() {
        let text = "";
        let events = parse_md(text);
        let symbols = compute_document_symbols(text, &events);
        assert!(symbols.is_empty());
    }

    #[test]
    fn symbols_no_heading() {
        let text = "just text\nmore text";
        let events = parse_md(text);
        let symbols = compute_document_symbols(text, &events);
        assert!(symbols.is_empty());
    }

    #[test]
    fn symbols_h4_h5_h6() {
        let text = "#### H4\n##### H5\n###### H6";
        let events = parse_md(text);
        let symbols = compute_document_symbols(text, &events);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "H4");
        assert_eq!(symbols[0].children.len(), 1);
        assert_eq!(symbols[0].children[0].name, "H5");
        assert_eq!(symbols[0].children[0].children[0].name, "H6");
    }

    #[test]
    fn symbols_heading_with_bold() {
        let text = "## **bold title**";
        let events = parse_md(text);
        let symbols = compute_document_symbols(text, &events);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "bold title");
    }

    #[test]
    fn symbols_heading_with_code() {
        let text = "## `code` title";
        let events = parse_md(text);
        let symbols = compute_document_symbols(text, &events);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "code title");
    }

    #[test]
    fn folding_empty() {
        let text = "";
        let events = parse_md(text);
        let ranges = compute_folding_ranges(text, &events);
        assert!(ranges.is_empty());
    }

    #[test]
    fn folding_nested_headings() {
        let text = "# H1\ncontent\n## H2\nmore\n### H3\ndeep\n## H2b\nmore\n# H1b\nend";
        let events = parse_md(text);
        let ranges = compute_folding_ranges(text, &events);
        assert!(!ranges.is_empty());
        let h1_fold = ranges.iter().find(|r| r.start_line == 0).unwrap();
        assert!(h1_fold.end_line >= 7);
    }

    #[test]
    fn folding_trailing_heading() {
        let text = "# H1\ncontent\n## H2\nmore";
        let events = parse_md(text);
        let ranges = compute_folding_ranges(text, &events);
        assert!(!ranges.is_empty());
    }

    #[test]
    fn hover_image() {
        let text = "![alt](img.png)";
        let events = parse_md(text);
        let hover = find_hover(&events, 3).unwrap();
        assert!(hover.contents.contains("img.png"));
    }

    #[test]
    fn hover_none() {
        let text = "plain text";
        let events = parse_md(text);
        let hover = find_hover(&events, 3);
        assert!(hover.is_none());
    }

    #[test]
    fn hover_at_boundary() {
        let text = "just plain text";
        let events = parse_md(text);
        let hover = find_hover(&events, 0);
        assert!(hover.is_none());
    }

    #[test]
    fn lc_offset_col_out_of_range() {
        let text = "abc\ndef";
        let result = line_col_to_offset(text, 0, 10);
        assert!(result.is_none());
    }

    #[test]
    fn lc_offset_line_out_of_range() {
        let text = "abc\ndef";
        let result = line_col_to_offset(text, 5, 0);
        assert!(result.is_none());
    }

    #[test]
    fn lsp_update_twice() {
        let mut lsp = MarkdownLsp::new();
        let path = Path::new("test.md");
        lsp.update(path, "# First");
        assert_eq!(lsp.document_symbols(path)[0].name, "First");
        lsp.update(path, "# Second");
        assert_eq!(lsp.document_symbols(path)[0].name, "Second");
    }

    #[test]
    fn lsp_diagnostics() {
        let mut lsp = MarkdownLsp::new();
        let path = Path::new("test.md");
        lsp.update(path, "# Hello");
        let diags = lsp.diagnostics(path);
        assert!(diags.is_empty());
    }
}
