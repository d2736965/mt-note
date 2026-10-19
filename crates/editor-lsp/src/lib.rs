#![forbid(
    unused_must_use,
    clippy::panic,
    clippy::let_underscore_must_use,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::allow_attributes,
    unfulfilled_lint_expectations
)]
#![deny(unused_variables)]

pub mod highlight;
pub mod markdown;
pub mod types;
pub mod typst;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use kameo::prelude::*;
use types::*;

pub trait EditorLsp: Send + Sync {
    fn language_id(&self) -> &'static str;

    fn update(&mut self, path: &Path, content: &str);
    fn close(&mut self, path: &Path);

    fn semantic_tokens(&self, path: &Path) -> Vec<SemanticToken>;
    fn diagnostics(&self, path: &Path) -> Vec<LspDiagnostic>;
    fn completions(&self, path: &Path, pos: TextPosition) -> Vec<Completion>;
    fn hover(&self, path: &Path, pos: TextPosition) -> Option<HoverInfo>;
    fn goto_definition(&self, path: &Path, pos: TextPosition) -> Option<Location>;
    fn references(&self, path: &Path, pos: TextPosition) -> Vec<Location>;
    fn signature_help(&self, path: &Path, pos: TextPosition)
    -> Option<crate::types::SignatureHelp>;
    fn document_symbols(&self, path: &Path) -> Vec<DocumentSymbol>;
    fn folding_ranges(&self, path: &Path) -> Vec<FoldingRange>;
    fn code_actions(&self, path: &Path, range: TextRange) -> Vec<CodeAction>;
    fn document_highlight(&self, path: &Path, pos: TextPosition) -> Vec<TextRange>;
    fn rename(&self, path: &Path, pos: TextPosition, new_name: &str) -> Vec<TextEdit>;
    fn inlay_hints(&self, path: &Path) -> Vec<InlayHint>;
}

type LspFactory = Arc<dyn Fn() -> Result<Box<dyn EditorLsp>, String> + Send + Sync>;
type LspFactoryEntry = (String, LspFactory);

#[derive(Actor)]
pub struct LspManager {
    engines: HashMap<String, Box<dyn EditorLsp>>,
    factories: Vec<LspFactoryEntry>,
}

impl LspManager {
    fn get_or_create(&mut self, ext: &str) -> Option<&mut Box<dyn EditorLsp>> {
        if !self.engines.contains_key(ext) {
            let factory = self.factories.iter().find(|(e, _)| e == ext)?.clone();
            match (factory.1)() {
                Ok(engine) => {
                    self.engines.insert(ext.to_string(), engine);
                }
                Err(_e) => {
                    return None;
                }
            }
        }
        self.engines.get_mut(ext)
    }
}

#[messages]
impl LspManager {
    #[message]
    pub fn update(&mut self, path: PathBuf, content: String) {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        if let Some(ext) = ext
            && let Some(engine) = self.get_or_create(&ext)
        {
            engine.update(&path, &content);
        }
    }

    #[message]

    pub fn close(&mut self, path: PathBuf) {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        if let Some(ext) = ext
            && let Some(engine) = self.engines.get_mut(&ext)
        {
            engine.close(&path);
        }
    }

    #[message]

    pub fn semantic_tokens(&mut self, path: PathBuf) -> Vec<SemanticToken> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        match ext.and_then(|e| self.get_or_create(&e)) {
            Some(engine) => engine.semantic_tokens(&path),
            None => Vec::new(),
        }
    }

    #[message]

    pub fn diagnostics(&mut self, path: PathBuf) -> Vec<LspDiagnostic> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        match ext.and_then(|e| self.get_or_create(&e)) {
            Some(engine) => engine.diagnostics(&path),
            None => Vec::new(),
        }
    }

    #[message]

    pub fn completions(&mut self, path: PathBuf, pos: TextPosition) -> Vec<Completion> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        match ext.and_then(|e| self.get_or_create(&e)) {
            Some(engine) => engine.completions(&path, pos),
            None => Vec::new(),
        }
    }

    #[message]

    pub fn hover(&mut self, path: PathBuf, pos: TextPosition) -> Option<HoverInfo> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        ext.and_then(|e| self.get_or_create(&e))
            .and_then(|engine| engine.hover(&path, pos))
    }

    #[message]

    pub fn goto_definition(&mut self, path: PathBuf, pos: TextPosition) -> Option<Location> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        ext.and_then(|e| self.get_or_create(&e))
            .and_then(|engine| engine.goto_definition(&path, pos))
    }

    #[message]

    pub fn references(&mut self, path: PathBuf, pos: TextPosition) -> Vec<Location> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        match ext.and_then(|e| self.get_or_create(&e)) {
            Some(engine) => engine.references(&path, pos),
            None => Vec::new(),
        }
    }

    #[message]

    pub fn get_signature_help(
        &mut self,
        path: PathBuf,
        pos: TextPosition,
    ) -> Option<SignatureHelp> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        ext.and_then(|e| self.get_or_create(&e))
            .and_then(|engine| engine.signature_help(&path, pos))
    }

    #[message]

    pub fn document_symbols(&mut self, path: PathBuf) -> Vec<DocumentSymbol> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        match ext.and_then(|e| self.get_or_create(&e)) {
            Some(engine) => engine.document_symbols(&path),
            None => Vec::new(),
        }
    }

    #[message]

    pub fn folding_ranges(&mut self, path: PathBuf) -> Vec<FoldingRange> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        match ext.and_then(|e| self.get_or_create(&e)) {
            Some(engine) => engine.folding_ranges(&path),
            None => Vec::new(),
        }
    }

    #[message]

    pub fn code_actions(&mut self, path: PathBuf, range: TextRange) -> Vec<CodeAction> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        match ext.and_then(|e| self.get_or_create(&e)) {
            Some(engine) => engine.code_actions(&path, range),
            None => Vec::new(),
        }
    }

    #[message]

    pub fn document_highlight(&mut self, path: PathBuf, pos: TextPosition) -> Vec<TextRange> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        match ext.and_then(|e| self.get_or_create(&e)) {
            Some(engine) => engine.document_highlight(&path, pos),
            None => Vec::new(),
        }
    }

    #[message]

    pub fn rename(&mut self, path: PathBuf, pos: TextPosition, new_name: String) -> Vec<TextEdit> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        match ext.and_then(|e| self.get_or_create(&e)) {
            Some(engine) => engine.rename(&path, pos, &new_name),
            None => Vec::new(),
        }
    }

    #[message]

    pub fn inlay_hints(&mut self, path: PathBuf) -> Vec<InlayHint> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        match ext.and_then(|e| self.get_or_create(&e)) {
            Some(engine) => engine.inlay_hints(&path),
            None => Vec::new(),
        }
    }
}

pub struct LspContext {
    pub manager: ActorRef<LspManager>,
}

impl Clone for LspContext {
    fn clone(&self) -> Self {
        Self {
            manager: self.manager.clone(),
        }
    }
}

impl LspContext {
    pub fn new() -> Self {
        let manager = LspManager {
            engines: HashMap::new(),
            factories: Vec::new(),
        };
        let actor_ref = LspManager::spawn(manager);
        Self { manager: actor_ref }
    }

    pub fn new_with_defaults(cache_dir: Option<PathBuf>, font_paths: Vec<PathBuf>) -> Self {
        let cache_dir_for_typst = cache_dir.clone();
        let font_paths_for_typst = font_paths.clone();
        let mut factories: Vec<LspFactoryEntry> = Vec::new();
        let typst_factory: LspFactory = Arc::new(move || {
            typst::TypstLsp::new(cache_dir_for_typst.clone(), font_paths_for_typst.clone())
                .map(|lsp| Box::new(lsp) as Box<dyn EditorLsp>)
                .map_err(|e| format!("Failed to create TypstLsp: {e}"))
        });
        factories.push(("typ".to_string(), Arc::clone(&typst_factory)));
        factories.push(("typst".to_string(), typst_factory));

        let md_factory: LspFactory =
            Arc::new(|| Ok(Box::new(markdown::MarkdownLsp::new()) as Box<dyn EditorLsp>));
        factories.push(("md".to_string(), md_factory));

        let manager = LspManager {
            engines: HashMap::new(),
            factories,
        };
        let actor_ref = LspManager::spawn(manager);
        Self { manager: actor_ref }
    }
}

impl Default for LspContext {
    fn default() -> Self {
        Self::new()
    }
}
