use std::path::PathBuf;

use mt_note_editor_lsp::{
    Close, CodeActions, Completions, Diagnostics, DocumentHighlight, DocumentSymbols,
    FoldingRanges, GetSignatureHelp, GotoDefinition, Hover, InlayHints, LspContext, References,
    Rename, SemanticTokens, Update,
    types::{TextPosition, TextRange},
};

#[tokio::test]
async fn full_flow() {
    let ctx = LspContext::new_with_defaults(None, vec![]);
    let path = PathBuf::from("test.md");
    let content = "# Hello\n\nThis is **bold** text.\n\n```rust\nlet x = 1;\n```\n\n[link](https://example.com)";

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: content.to_string(),
        })
        .await
        .expect("update failed");

    let tokens = ctx
        .manager
        .ask(SemanticTokens { path: path.clone() })
        .await
        .expect("semantic_tokens failed");
    assert!(!tokens.is_empty(), "should have semantic tokens");

    let symbols = ctx
        .manager
        .ask(DocumentSymbols { path: path.clone() })
        .await
        .expect("document_symbols failed");
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Hello");

    let folds = ctx
        .manager
        .ask(FoldingRanges { path: path.clone() })
        .await
        .expect("folding_ranges failed");
    assert!(!folds.is_empty(), "should have folding ranges");

    let hover = ctx
        .manager
        .ask(Hover {
            path: path.clone(),
            pos: TextPosition { line: 6, column: 1 },
        })
        .await
        .expect("hover failed");
    assert!(hover.is_some(), "should have hover info on link");

    ctx.manager
        .ask(Close { path: path.clone() })
        .await
        .expect("close failed");
}

#[tokio::test]
async fn multiple_files_independent() {
    let ctx = LspContext::new_with_defaults(None, vec![]);
    let path_a = PathBuf::from("a.md");
    let path_b = PathBuf::from("b.md");

    ctx.manager
        .ask(Update {
            path: path_a.clone(),
            content: "# Alpha\ncontent".to_string(),
        })
        .await
        .expect("update a failed");

    ctx.manager
        .ask(Update {
            path: path_b.clone(),
            content: "# Beta\n## Gamma\ncontent".to_string(),
        })
        .await
        .expect("update b failed");

    let symbols_a = ctx
        .manager
        .ask(DocumentSymbols {
            path: path_a.clone(),
        })
        .await
        .expect("symbols a failed");
    assert_eq!(symbols_a.len(), 1);
    assert_eq!(symbols_a[0].name, "Alpha");

    let symbols_b = ctx
        .manager
        .ask(DocumentSymbols {
            path: path_b.clone(),
        })
        .await
        .expect("symbols b failed");
    assert_eq!(symbols_b.len(), 1);
    assert_eq!(symbols_b[0].name, "Beta");
    assert_eq!(symbols_b[0].children.len(), 1);
    assert_eq!(symbols_b[0].children[0].name, "Gamma");
}

#[tokio::test]
async fn unknown_extension_returns_empty() {
    let ctx = LspContext::new_with_defaults(None, vec![]);
    let path = PathBuf::from("test.txt");

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: "# Title\ncontent".to_string(),
        })
        .await
        .expect("update failed");

    let tokens = ctx
        .manager
        .ask(SemanticTokens { path: path.clone() })
        .await
        .expect("semantic_tokens failed");
    assert!(tokens.is_empty(), "unknown ext should return empty tokens");

    let symbols = ctx
        .manager
        .ask(DocumentSymbols { path: path.clone() })
        .await
        .expect("document_symbols failed");
    assert!(
        symbols.is_empty(),
        "unknown ext should return empty symbols"
    );

    let folds = ctx
        .manager
        .ask(FoldingRanges { path: path.clone() })
        .await
        .expect("folding_ranges failed");
    assert!(folds.is_empty(), "unknown ext should return empty folds");

    let hover = ctx
        .manager
        .ask(Hover {
            path: path.clone(),
            pos: TextPosition { line: 0, column: 0 },
        })
        .await
        .expect("hover failed");
    assert!(hover.is_none(), "unknown ext should return no hover");
}

#[tokio::test]
async fn update_twice_replace_content() {
    let ctx = LspContext::new_with_defaults(None, vec![]);
    let path = PathBuf::from("test.md");

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: "# First".to_string(),
        })
        .await
        .expect("first update failed");

    let symbols = ctx
        .manager
        .ask(DocumentSymbols { path: path.clone() })
        .await
        .expect("symbols failed");
    assert_eq!(symbols[0].name, "First");

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: "# Second\n## Sub".to_string(),
        })
        .await
        .expect("second update failed");

    let symbols = ctx
        .manager
        .ask(DocumentSymbols { path: path.clone() })
        .await
        .expect("symbols after replace failed");
    assert_eq!(symbols[0].name, "Second");
    assert_eq!(symbols[0].children[0].name, "Sub");
}

#[tokio::test]
async fn close_then_open_again() {
    let ctx = LspContext::new_with_defaults(None, vec![]);
    let path = PathBuf::from("test.md");

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: "# Title\ncontent".to_string(),
        })
        .await
        .expect("update failed");

    ctx.manager
        .ask(Close { path: path.clone() })
        .await
        .expect("close failed");

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: "# Reopened".to_string(),
        })
        .await
        .expect("reopen update failed");

    let symbols = ctx
        .manager
        .ask(DocumentSymbols { path: path.clone() })
        .await
        .expect("symbols after reopen failed");
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Reopened");
}

#[tokio::test]
async fn empty_content() {
    let ctx = LspContext::new_with_defaults(None, vec![]);
    let path = PathBuf::from("test.md");

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: String::new(),
        })
        .await
        .expect("update failed");

    let tokens = ctx
        .manager
        .ask(SemanticTokens { path: path.clone() })
        .await
        .expect("semantic_tokens failed");
    assert!(tokens.is_empty());

    let symbols = ctx
        .manager
        .ask(DocumentSymbols { path: path.clone() })
        .await
        .expect("document_symbols failed");
    assert!(symbols.is_empty());

    let folds = ctx
        .manager
        .ask(FoldingRanges { path: path.clone() })
        .await
        .expect("folding_ranges failed");
    assert!(folds.is_empty());
}

#[tokio::test]
async fn hover_code_block() {
    let ctx = LspContext::new_with_defaults(None, vec![]);
    let path = PathBuf::from("test.md");
    let content = "```rust\nfn main() {}\n```";

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: content.to_string(),
        })
        .await
        .expect("update failed");

    let hover = ctx
        .manager
        .ask(Hover {
            path: path.clone(),
            pos: TextPosition { line: 0, column: 3 },
        })
        .await
        .expect("hover failed");
    assert!(hover.is_some(), "should have hover on code block");
    assert!(hover.unwrap().contents.contains("rust"));
}

#[tokio::test]
async fn hover_heading() {
    let ctx = LspContext::new_with_defaults(None, vec![]);
    let path = PathBuf::from("test.md");
    let content = "## Section Title";

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: content.to_string(),
        })
        .await
        .expect("update failed");

    let hover = ctx
        .manager
        .ask(Hover {
            path: path.clone(),
            pos: TextPosition { line: 0, column: 0 },
        })
        .await
        .expect("hover failed");
    assert!(hover.is_some(), "should have hover on heading");
    assert!(hover.unwrap().contents.contains("Heading"));
}

#[tokio::test]
async fn completions_returns_empty() {
    let ctx = LspContext::new_with_defaults(None, vec![]);
    let path = PathBuf::from("test.md");

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: "# Title".to_string(),
        })
        .await
        .expect("update");

    let completions = ctx
        .manager
        .ask(Completions {
            path: path.clone(),
            pos: TextPosition { line: 0, column: 0 },
        })
        .await
        .expect("completions");
    assert!(completions.is_empty(), "md completions should be empty");
}

#[tokio::test]
async fn goto_definition_returns_none() {
    let ctx = LspContext::new_with_defaults(None, vec![]);
    let path = PathBuf::from("test.md");

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: "# Title".to_string(),
        })
        .await
        .expect("update");

    let def = ctx
        .manager
        .ask(GotoDefinition {
            path: path.clone(),
            pos: TextPosition { line: 0, column: 0 },
        })
        .await
        .expect("goto_definition");
    assert!(def.is_none(), "md goto_definition should be None");
}

#[tokio::test]
async fn references_returns_empty() {
    let ctx = LspContext::new_with_defaults(None, vec![]);
    let path = PathBuf::from("test.md");

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: "# Title".to_string(),
        })
        .await
        .expect("update");

    let refs = ctx
        .manager
        .ask(References {
            path: path.clone(),
            pos: TextPosition { line: 0, column: 0 },
        })
        .await
        .expect("references");
    assert!(refs.is_empty(), "md references should be empty");
}

#[tokio::test]
async fn signature_help_returns_none() {
    let ctx = LspContext::new_with_defaults(None, vec![]);
    let path = PathBuf::from("test.md");

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: "# Title".to_string(),
        })
        .await
        .expect("update");

    let sig = ctx
        .manager
        .ask(GetSignatureHelp {
            path: path.clone(),
            pos: TextPosition { line: 0, column: 0 },
        })
        .await
        .expect("signature_help");
    assert!(sig.is_none(), "md signature_help should be None");
}

#[tokio::test]
async fn code_actions_returns_empty() {
    let ctx = LspContext::new_with_defaults(None, vec![]);
    let path = PathBuf::from("test.md");

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: "# Title".to_string(),
        })
        .await
        .expect("update");

    let actions = ctx
        .manager
        .ask(CodeActions {
            path: path.clone(),
            range: TextRange {
                start: TextPosition { line: 0, column: 0 },
                end: TextPosition { line: 0, column: 5 },
            },
        })
        .await
        .expect("code_actions");
    assert!(actions.is_empty(), "md code_actions should be empty");
}

#[tokio::test]
async fn document_highlight_returns_empty() {
    let ctx = LspContext::new_with_defaults(None, vec![]);
    let path = PathBuf::from("test.md");

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: "# Title".to_string(),
        })
        .await
        .expect("update");

    let highlights = ctx
        .manager
        .ask(DocumentHighlight {
            path: path.clone(),
            pos: TextPosition { line: 0, column: 0 },
        })
        .await
        .expect("document_highlight");
    assert!(
        highlights.is_empty(),
        "md document_highlight should be empty"
    );
}

#[tokio::test]
async fn rename_returns_empty() {
    let ctx = LspContext::new_with_defaults(None, vec![]);
    let path = PathBuf::from("test.md");

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: "# Title".to_string(),
        })
        .await
        .expect("update");

    let edits = ctx
        .manager
        .ask(Rename {
            path: path.clone(),
            pos: TextPosition { line: 0, column: 0 },
            new_name: "x".to_string(),
        })
        .await
        .expect("rename");
    assert!(edits.is_empty(), "md rename should be empty");
}

#[tokio::test]
async fn inlay_hints_returns_empty() {
    let ctx = LspContext::new_with_defaults(None, vec![]);
    let path = PathBuf::from("test.md");

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: "# Title".to_string(),
        })
        .await
        .expect("update");

    let hints = ctx
        .manager
        .ask(InlayHints { path: path.clone() })
        .await
        .expect("inlay_hints");
    assert!(hints.is_empty(), "md inlay_hints should be empty");
}

#[tokio::test]
async fn diagnostics_on_valid_md_returns_empty() {
    let ctx = LspContext::new_with_defaults(None, vec![]);
    let path = PathBuf::from("test.md");

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: "# Hello\n\nThis is **bold** text.".to_string(),
        })
        .await
        .expect("update");

    let diags = ctx
        .manager
        .ask(Diagnostics { path: path.clone() })
        .await
        .expect("diagnostics");
    assert!(diags.is_empty(), "valid md should have no diagnostics");
}
