use std::path::PathBuf;

use mt_note_editor_lsp::{
    Close, CodeActions, Completions, Diagnostics, DocumentHighlight, DocumentSymbols,
    FoldingRanges, GetSignatureHelp, GotoDefinition, Hover, InlayHints, LspContext, References,
    Rename, SemanticTokens, Update,
    types::{TextPosition, TextRange},
};

#[tokio::test]
async fn empty_context_all_returns_empty() {
    let ctx = LspContext::new();
    let path = PathBuf::from("test.md");

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: "# Title".to_string(),
        })
        .await
        .expect("update");

    let tokens = ctx
        .manager
        .ask(SemanticTokens { path: path.clone() })
        .await
        .expect("tokens");
    assert!(tokens.is_empty(), "empty context → empty tokens");

    let diags = ctx
        .manager
        .ask(Diagnostics { path: path.clone() })
        .await
        .expect("diags");
    assert!(diags.is_empty(), "empty context → empty diagnostics");

    let symbols = ctx
        .manager
        .ask(DocumentSymbols { path: path.clone() })
        .await
        .expect("symbols");
    assert!(symbols.is_empty(), "empty context → empty symbols");

    let hover = ctx
        .manager
        .ask(Hover {
            path: path.clone(),
            pos: TextPosition { line: 0, column: 0 },
        })
        .await
        .expect("hover");
    assert!(hover.is_none(), "empty context → no hover");

    let folds = ctx
        .manager
        .ask(FoldingRanges { path: path.clone() })
        .await
        .expect("folds");
    assert!(folds.is_empty(), "empty context → empty folds");

    let completions = ctx
        .manager
        .ask(Completions {
            path: path.clone(),
            pos: TextPosition { line: 0, column: 0 },
        })
        .await
        .expect("completions");
    assert!(completions.is_empty(), "empty context → empty completions");

    let def = ctx
        .manager
        .ask(GotoDefinition {
            path: path.clone(),
            pos: TextPosition { line: 0, column: 0 },
        })
        .await
        .expect("goto_definition");
    assert!(def.is_none(), "empty context → no goto definition");

    let refs = ctx
        .manager
        .ask(References {
            path: path.clone(),
            pos: TextPosition { line: 0, column: 0 },
        })
        .await
        .expect("references");
    assert!(refs.is_empty(), "empty context → empty references");

    let sig = ctx
        .manager
        .ask(GetSignatureHelp {
            path: path.clone(),
            pos: TextPosition { line: 0, column: 0 },
        })
        .await
        .expect("signature_help");
    assert!(sig.is_none(), "empty context → no signature help");

    let highlights = ctx
        .manager
        .ask(DocumentHighlight {
            path: path.clone(),
            pos: TextPosition { line: 0, column: 0 },
        })
        .await
        .expect("document_highlight");
    assert!(highlights.is_empty(), "empty context → empty highlights");

    let edits = ctx
        .manager
        .ask(Rename {
            path: path.clone(),
            pos: TextPosition { line: 0, column: 0 },
            new_name: "x".to_string(),
        })
        .await
        .expect("rename");
    assert!(edits.is_empty(), "empty context → empty rename");

    let hints = ctx
        .manager
        .ask(InlayHints { path: path.clone() })
        .await
        .expect("inlay_hints");
    assert!(hints.is_empty(), "empty context → empty inlay hints");

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
    assert!(actions.is_empty(), "empty context → empty code actions");
}

#[tokio::test]
async fn default_context_routes_md_to_markdown() {
    let ctx = LspContext::new_with_defaults(None, vec![]);
    let path = PathBuf::from("test.md");

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: "# Hello\n\nContent.".to_string(),
        })
        .await
        .expect("update");

    let tokens = ctx
        .manager
        .ask(SemanticTokens { path: path.clone() })
        .await
        .expect("tokens");
    assert!(!tokens.is_empty(), "md should have semantic tokens");

    let symbols = ctx
        .manager
        .ask(DocumentSymbols { path: path.clone() })
        .await
        .expect("symbols");
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Hello");
}

#[tokio::test]
async fn default_context_routes_typ_to_typst() {
    let ctx = LspContext::new_with_defaults(None, vec![]);
    let path = temp_typst_file("route_typ.typ", "= Title\n\nContent.");

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: std::fs::read_to_string(&path).expect("read"),
        })
        .await
        .expect("update");

    let symbols = ctx
        .manager
        .ask(DocumentSymbols { path: path.clone() })
        .await
        .expect("symbols");
    assert!(!symbols.is_empty(), "typ should have document symbols");
}

#[tokio::test]
async fn default_context_routes_typst_to_typst() {
    let ctx = LspContext::new_with_defaults(None, vec![]);
    let path = temp_typst_file("route_typst.typst", "= Title\n\nContent.");

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: std::fs::read_to_string(&path).expect("read"),
        })
        .await
        .expect("update");

    let symbols = ctx
        .manager
        .ask(DocumentSymbols { path: path.clone() })
        .await
        .expect("symbols");
    assert!(
        !symbols.is_empty(),
        "typst ext should have document symbols"
    );
}

#[tokio::test]
async fn default_context_unknown_extension_returns_empty() {
    let ctx = LspContext::new_with_defaults(None, vec![]);
    let path = PathBuf::from("test.txt");

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: "# Title".to_string(),
        })
        .await
        .expect("update");

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

    let def = ctx
        .manager
        .ask(GotoDefinition {
            path: path.clone(),
            pos: TextPosition { line: 0, column: 0 },
        })
        .await
        .expect("goto_definition");
    assert!(def.is_none(), "unknown ext → no goto definition");
}

#[tokio::test]
async fn no_extension_returns_empty() {
    let ctx = LspContext::new_with_defaults(None, vec![]);
    let path = PathBuf::from("noextension");

    ctx.manager
        .ask(Update {
            path: path.clone(),
            content: "content".to_string(),
        })
        .await
        .expect("update");

    let tokens = ctx
        .manager
        .ask(SemanticTokens { path: path.clone() })
        .await
        .expect("tokens");
    assert!(tokens.is_empty(), "no extension → empty tokens");

    let symbols = ctx
        .manager
        .ask(DocumentSymbols { path: path.clone() })
        .await
        .expect("symbols");
    assert!(symbols.is_empty(), "no extension → empty symbols");

    let hover = ctx
        .manager
        .ask(Hover {
            path: path.clone(),
            pos: TextPosition { line: 0, column: 0 },
        })
        .await
        .expect("hover");
    assert!(hover.is_none(), "no extension → no hover");
}

#[tokio::test]
async fn md_and_typ_engines_are_independent() {
    let ctx = LspContext::new_with_defaults(None, vec![]);
    let md_path = PathBuf::from("test.md");
    let typ_path = temp_typst_file("indep.typ", "= Typst\n\nContent.");

    ctx.manager
        .ask(Update {
            path: md_path.clone(),
            content: "# Markdown".to_string(),
        })
        .await
        .expect("md update");

    ctx.manager
        .ask(Update {
            path: typ_path.clone(),
            content: std::fs::read_to_string(&typ_path).expect("read"),
        })
        .await
        .expect("typ update");

    let md_symbols = ctx
        .manager
        .ask(DocumentSymbols {
            path: md_path.clone(),
        })
        .await
        .expect("md symbols");
    assert!(!md_symbols.is_empty());
    assert_eq!(md_symbols[0].name, "Markdown");

    let typ_symbols = ctx
        .manager
        .ask(DocumentSymbols {
            path: typ_path.clone(),
        })
        .await
        .expect("typ symbols");
    assert!(!typ_symbols.is_empty());
}

#[tokio::test]
async fn close_on_one_file_does_not_affect_other() {
    let ctx = LspContext::new_with_defaults(None, vec![]);
    let path_a = PathBuf::from("a.md");
    let path_b = PathBuf::from("b.md");

    ctx.manager
        .ask(Update {
            path: path_a.clone(),
            content: "# A".to_string(),
        })
        .await
        .expect("update a");

    ctx.manager
        .ask(Update {
            path: path_b.clone(),
            content: "# B".to_string(),
        })
        .await
        .expect("update b");

    ctx.manager
        .ask(Close {
            path: path_a.clone(),
        })
        .await
        .expect("close a");

    let symbols_b = ctx
        .manager
        .ask(DocumentSymbols {
            path: path_b.clone(),
        })
        .await
        .expect("symbols b");
    assert!(!symbols_b.is_empty());
    assert_eq!(symbols_b[0].name, "B");
}

fn temp_typst_file(name: &str, content: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("mt-note-mgr-test");
    std::fs::create_dir_all(&dir).expect("failed to create temp dir");
    let path = dir.join(name);
    std::fs::write(&path, content).expect("failed to write temp file");
    path
}
