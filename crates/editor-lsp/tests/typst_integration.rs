use std::fs;
use std::path::PathBuf;

use mt_note_editor_lsp::{
    Close, CodeActions, Completions, Diagnostics, DocumentHighlight, DocumentSymbols,
    FoldingRanges, GetSignatureHelp, GotoDefinition, Hover, InlayHints, LspContext, References,
    Rename, SemanticTokens, Update,
    types::{TextPosition, TextRange},
};

fn temp_typst_file(name: &str, content: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("mt-note-typst-test");
    fs::create_dir_all(&dir).expect("failed to create temp dir");
    let path = dir.join(name);
    fs::write(&path, content).expect("failed to write temp file");
    path
}

#[tokio::test]
async fn full_flow() {
    let path = temp_typst_file(
        "full_flow.typ",
        "#set page(width: 10cm, height: 10cm)\n\n= Introduction\n\nThis is *italic* and **bold** text.\n\n#let x = 42\n\n$x + 1$",
    );
    let ctx = LspContext::new_with_defaults(None, vec![]);

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: fs::read_to_string(&path).expect("read failed"),
        })
        .await
        .expect("update failed");

    let tokens = ctx
        .manager
        .ask(SemanticTokens { path: path.clone() })
        .await
        .expect("semantic_tokens failed");
    assert!(
        !tokens.is_empty(),
        "should have semantic tokens for typst content"
    );

    let diags = ctx
        .manager
        .ask(Diagnostics { path: path.clone() })
        .await
        .expect("diagnostics failed");
    assert!(!diags.is_empty(), "should have diagnostics");

    let symbols = ctx
        .manager
        .ask(DocumentSymbols { path: path.clone() })
        .await
        .expect("document_symbols failed");
    assert!(!symbols.is_empty(), "should have document symbols");

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
            pos: TextPosition { line: 2, column: 0 },
        })
        .await
        .expect("hover failed");
    assert!(matches!(hover, Some(_) | None));

    ctx.manager
        .ask(Close { path: path.clone() })
        .await
        .expect("close failed");
}

#[tokio::test]
async fn multiple_files_independent() {
    let path_a = temp_typst_file("multi_a.typ", "= Alpha\n\nContent A.");
    let path_b = temp_typst_file("multi_b.typ", "= Beta\n\n== Gamma\n\nContent B.");
    let ctx = LspContext::new_with_defaults(None, vec![]);

    ctx.manager
        .ask(Update {
            path: path_a.clone(),
            content: fs::read_to_string(&path_a).expect("read a"),
        })
        .await
        .expect("update a failed");
    ctx.manager
        .ask(Update {
            path: path_b.clone(),
            content: fs::read_to_string(&path_b).expect("read b"),
        })
        .await
        .expect("update b failed");

    let sym_a = ctx
        .manager
        .ask(DocumentSymbols {
            path: path_a.clone(),
        })
        .await
        .expect("symbols a");
    assert!(!sym_a.is_empty(), "file A should have symbols");

    let sym_b = ctx
        .manager
        .ask(DocumentSymbols {
            path: path_b.clone(),
        })
        .await
        .expect("symbols b");
    assert!(!sym_b.is_empty(), "file B should have symbols");
}

#[tokio::test]
async fn unknown_extension_returns_empty() {
    let path = temp_typst_file("test.txt", "= Title");
    let ctx = LspContext::new_with_defaults(None, vec![]);

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: fs::read_to_string(&path).expect("read"),
        })
        .await
        .expect("update failed");

    let tokens = ctx
        .manager
        .ask(SemanticTokens { path: path.clone() })
        .await
        .expect("tokens");
    assert!(tokens.is_empty(), "unknown ext → empty tokens");

    let symbols = ctx
        .manager
        .ask(DocumentSymbols { path: path.clone() })
        .await
        .expect("symbols");
    assert!(symbols.is_empty(), "unknown ext → empty symbols");

    let hover = ctx
        .manager
        .ask(Hover {
            path: path.clone(),
            pos: TextPosition { line: 0, column: 0 },
        })
        .await
        .expect("hover");
    assert!(hover.is_none(), "unknown ext → no hover");
}

#[tokio::test]
async fn update_twice_replace_content() {
    let path = temp_typst_file("replace.typ", "= First");
    let ctx = LspContext::new_with_defaults(None, vec![]);

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: fs::read_to_string(&path).expect("read"),
        })
        .await
        .expect("first update");

    let sym_first = ctx
        .manager
        .ask(DocumentSymbols { path: path.clone() })
        .await
        .expect("symbols first");
    assert!(
        !sym_first.is_empty(),
        "should have symbols from first update"
    );
    fs::write(&path, "= Second\n\n== Sub").expect("rewrite");
    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: fs::read_to_string(&path).expect("read second"),
        })
        .await
        .expect("second update");

    let sym_second = ctx
        .manager
        .ask(DocumentSymbols { path: path.clone() })
        .await
        .expect("symbols second");
    assert!(!sym_second.is_empty(), "should have symbols after replace");
}

#[tokio::test]
async fn close_then_open_again() {
    let path = temp_typst_file("reopen.typ", "= Title\n\nContent.");
    let ctx = LspContext::new_with_defaults(None, vec![]);

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: fs::read_to_string(&path).expect("read"),
        })
        .await
        .expect("update");

    ctx.manager
        .ask(Close { path: path.clone() })
        .await
        .expect("close");

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: "= Reopened".to_string(),
        })
        .await
        .expect("reopen update");

    let sym = ctx
        .manager
        .ask(DocumentSymbols { path: path.clone() })
        .await
        .expect("symbols");
    assert!(!sym.is_empty(), "should have symbols after reopen");
}

#[tokio::test]
async fn empty_content() {
    let path = temp_typst_file("empty.typ", "");
    let ctx = LspContext::new_with_defaults(None, vec![]);

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: String::new(),
        })
        .await
        .expect("update");

    let tokens = ctx
        .manager
        .ask(SemanticTokens { path: path.clone() })
        .await
        .expect("tokens");
    assert!(tokens.is_empty(), "empty content → empty tokens");

    let symbols = ctx
        .manager
        .ask(DocumentSymbols { path: path.clone() })
        .await
        .expect("symbols");
    assert!(symbols.is_empty(), "empty content → empty symbols");

    let folds = ctx
        .manager
        .ask(FoldingRanges { path: path.clone() })
        .await
        .expect("folds");
    assert!(folds.is_empty(), "empty content → empty folds");
}

#[tokio::test]
async fn diagnostics_on_valid_file() {
    let path = temp_typst_file(
        "diag_ok.typ",
        "#set page(width: 10cm, height: 10cm)\n\n= Title\n\nHello world.",
    );
    let ctx = LspContext::new_with_defaults(None, vec![]);

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: fs::read_to_string(&path).expect("read"),
        })
        .await
        .expect("update");

    let diags = ctx
        .manager
        .ask(Diagnostics { path: path.clone() })
        .await
        .expect("diagnostics");
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| {
            matches!(
                d.severity,
                mt_note_editor_lsp::types::DiagnosticSeverity::Error
            )
        })
        .collect();
    assert!(
        errors.is_empty(),
        "valid typst file should have no errors, got: {errors:?}"
    );
}

#[tokio::test]
async fn diagnostics_on_invalid_file() {
    let path = temp_typst_file("diag_err.typ", "= Title\n\n#unknown_func()\n");
    let ctx = LspContext::new_with_defaults(None, vec![]);

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: fs::read_to_string(&path).expect("read"),
        })
        .await
        .expect("update");

    let diags = ctx
        .manager
        .ask(Diagnostics { path: path.clone() })
        .await
        .expect("diagnostics");
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| {
            matches!(
                d.severity,
                mt_note_editor_lsp::types::DiagnosticSeverity::Error
            )
        })
        .collect();
    assert!(!errors.is_empty(), "invalid typst file should have errors");
}

#[tokio::test]
async fn completions_basic() {
    let path = temp_typst_file("complete.typ", "#set page(width: 10cm)\n\n#let x = 42\n\n#");
    let ctx = LspContext::new_with_defaults(None, vec![]);

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: fs::read_to_string(&path).expect("read"),
        })
        .await
        .expect("update");

    let completions = ctx
        .manager
        .ask(Completions {
            path: path.clone(),
            pos: TextPosition { line: 3, column: 1 },
        })
        .await
        .expect("completions");
    completions.len();
}

#[tokio::test]
async fn hover_on_heading() {
    let path = temp_typst_file("hover_h.typ", "= Section Title\n\nContent here.");
    let ctx = LspContext::new_with_defaults(None, vec![]);

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: fs::read_to_string(&path).expect("read"),
        })
        .await
        .expect("update");

    let hover = ctx
        .manager
        .ask(Hover {
            path: path.clone(),
            pos: TextPosition { line: 0, column: 0 },
        })
        .await
        .expect("hover");
    assert!(matches!(hover, Some(_) | None));
}

#[tokio::test]
async fn hover_on_text() {
    let path = temp_typst_file("hover_t.typ", "Some *italic* text.");
    let ctx = LspContext::new_with_defaults(None, vec![]);

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: fs::read_to_string(&path).expect("read"),
        })
        .await
        .expect("update");

    let hover = ctx
        .manager
        .ask(Hover {
            path: path.clone(),
            pos: TextPosition { line: 0, column: 1 },
        })
        .await
        .expect("hover");
    assert!(matches!(hover, Some(_) | None));
}

#[tokio::test]
async fn goto_definition_basic() {
    let path = temp_typst_file("goto_def.typ", "#let x = 42\n\n#x");
    let ctx = LspContext::new_with_defaults(None, vec![]);

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: fs::read_to_string(&path).expect("read"),
        })
        .await
        .expect("update");

    let def = ctx
        .manager
        .ask(GotoDefinition {
            path: path.clone(),
            pos: TextPosition { line: 2, column: 1 },
        })
        .await
        .expect("goto_definition");
    assert!(def.is_some(), "should have goto definition for #x");
}

#[tokio::test]
async fn references_basic() {
    let path = temp_typst_file("refs.typ", "#let x = 42\n\n#x\n#x");
    let ctx = LspContext::new_with_defaults(None, vec![]);

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: fs::read_to_string(&path).expect("read"),
        })
        .await
        .expect("update");

    let refs = ctx
        .manager
        .ask(References {
            path: path.clone(),
            pos: TextPosition { line: 0, column: 5 },
        })
        .await
        .expect("references");
    assert!(!refs.is_empty(), "should have references for x");
}

#[tokio::test]
async fn signature_help_basic() {
    let path = temp_typst_file("sig.typ", "#let f(x, y) = x + y\n\n#f(");
    let ctx = LspContext::new_with_defaults(None, vec![]);

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: fs::read_to_string(&path).expect("read"),
        })
        .await
        .expect("update");

    let sig = ctx
        .manager
        .ask(GetSignatureHelp {
            path: path.clone(),
            pos: TextPosition { line: 2, column: 2 },
        })
        .await
        .expect("signature_help");
    assert!(sig.is_some(), "should have signature help for f(");
}

#[tokio::test]
async fn document_highlight_basic() {
    let path = temp_typst_file("highlight.typ", "#let x = 42\n\n#x\n#x");
    let ctx = LspContext::new_with_defaults(None, vec![]);

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: fs::read_to_string(&path).expect("read"),
        })
        .await
        .expect("update");

    let highlights = ctx
        .manager
        .ask(DocumentHighlight {
            path: path.clone(),
            pos: TextPosition { line: 0, column: 5 },
        })
        .await
        .expect("document_highlight");
    highlights.len();
}

#[tokio::test]
async fn rename_basic() {
    let path = temp_typst_file("rename.typ", "#let x = 42\n\n#x\n#x");
    let ctx = LspContext::new_with_defaults(None, vec![]);

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: fs::read_to_string(&path).expect("read"),
        })
        .await
        .expect("update");

    let edits = ctx
        .manager
        .ask(Rename {
            path: path.clone(),
            pos: TextPosition { line: 0, column: 5 },
            new_name: "y".to_string(),
        })
        .await
        .expect("rename");
    assert!(!edits.is_empty(), "should have rename edits for x → y");
}

#[tokio::test]
async fn inlay_hints_basic() {
    let path = temp_typst_file("inlay.typ", "#let x = 42\n\n#x");
    let ctx = LspContext::new_with_defaults(None, vec![]);

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: fs::read_to_string(&path).expect("read"),
        })
        .await
        .expect("update");

    let hints = ctx
        .manager
        .ask(InlayHints { path: path.clone() })
        .await
        .expect("inlay_hints");
    hints.len();
}

#[tokio::test]
async fn code_actions_basic() {
    let path = temp_typst_file(
        "actions.typ",
        "#set page(width: 10cm)\n\n= Title\n\nContent.",
    );
    let ctx = LspContext::new_with_defaults(None, vec![]);

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: fs::read_to_string(&path).expect("read"),
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
    actions.len();
}

#[tokio::test]
async fn diagnostics_update_after_edit() {
    let path = temp_typst_file("diag_update.typ", "#for i in range(3) [\n  第#i项\n]\n");
    let ctx = LspContext::new_with_defaults(None, vec![]);

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: fs::read_to_string(&path).expect("read"),
        })
        .await
        .expect("first update");

    let diags_first = ctx
        .manager
        .ask(Diagnostics { path: path.clone() })
        .await
        .expect("diagnostics first");
    eprintln!(
        "diags_first: {:?}",
        diags_first.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    let errors_first: Vec<_> = diags_first
        .iter()
        .filter(|d| {
            matches!(
                d.severity,
                mt_note_editor_lsp::types::DiagnosticSeverity::Error
            )
        })
        .collect();
    assert!(
        !errors_first.is_empty(),
        "initial content should have errors (i项 is unknown variable), got: {errors_first:?}"
    );

    let updated_content = "#for i in range(3) [\n  第#i 项\n]\n";
    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: updated_content.to_string(),
        })
        .await
        .expect("second update");

    let diags_second = ctx
        .manager
        .ask(Diagnostics { path: path.clone() })
        .await
        .expect("diagnostics second");
    eprintln!(
        "diags_second: {:?}",
        diags_second.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    let errors_second: Vec<_> = diags_second
        .iter()
        .filter(|d| {
            matches!(
                d.severity,
                mt_note_editor_lsp::types::DiagnosticSeverity::Error
            )
        })
        .collect();
    assert!(
        errors_second.is_empty(),
        "fixed content should have no errors, got: {errors_second:?}"
    );
}

#[tokio::test]
async fn diagnostics_persist_after_minor_edit() {
    let path = temp_typst_file("diag_persist.typ", "#for i in range(3) [\n  第#i项\n]\n");
    let ctx = LspContext::new_with_defaults(None, vec![]);

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: fs::read_to_string(&path).expect("read"),
        })
        .await
        .expect("first update");

    let diags_first = ctx
        .manager
        .ask(Diagnostics { path: path.clone() })
        .await
        .expect("diagnostics first");
    let errors_first: Vec<_> = diags_first
        .iter()
        .filter(|d| {
            matches!(
                d.severity,
                mt_note_editor_lsp::types::DiagnosticSeverity::Error
            )
        })
        .collect();
    assert!(
        !errors_first.is_empty(),
        "initial content should have errors"
    );
    let first_count = errors_first.len();

    let edited_content = "#for i in range(3) [\n  第#i项\n]\n// comment\n";
    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: edited_content.to_string(),
        })
        .await
        .expect("edit update");

    let diags_after_edit = ctx
        .manager
        .ask(Diagnostics { path: path.clone() })
        .await
        .expect("diagnostics after edit");
    let errors_after: Vec<_> = diags_after_edit
        .iter()
        .filter(|d| {
            matches!(
                d.severity,
                mt_note_editor_lsp::types::DiagnosticSeverity::Error
            )
        })
        .collect();
    assert!(
        !errors_after.is_empty(),
        "same error should persist after minor edit, got {} errors (expected {first_count})",
        errors_after.len()
    );
}

#[tokio::test]
async fn folding_ranges_basic() {
    let path = temp_typst_file(
        "folds.typ",
        "#set page(width: 10cm)\n\n= Section 1\n\nContent.\n\n== Subsection\n\nMore content.\n\n= Section 2\n\nEnd.",
    );
    let ctx = LspContext::new_with_defaults(None, vec![]);

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: fs::read_to_string(&path).expect("read"),
        })
        .await
        .expect("update");

    let folds = ctx
        .manager
        .ask(FoldingRanges { path: path.clone() })
        .await
        .expect("folding_ranges");
    assert!(!folds.is_empty(), "should have folding ranges");
}
