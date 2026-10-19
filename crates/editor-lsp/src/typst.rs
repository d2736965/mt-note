use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tinymist_project::{DynAccessModel, EntryReader, LspUniverse, LspUniverseBuilder};
use tinymist_query::{
    CodeActionRequest, ColorTheme, CompletionFeat, CompletionRequest, DocumentHighlightRequest,
    DocumentSymbolRequest, FoldingRangeRequest, GotoDefinitionRequest, HoverRequest,
    InlayHintRequest, LocalContext, LocalContextGuard, PositionEncoding, ReferencesRequest,
    RenameRequest, SemanticRequest, SemanticTokensFullRequest, SignatureHelpRequest, SyntaxRequest,
    analysis::Analysis,
};
use tinymist_std::ImmutPath;
use tinymist_world::{
    EntryState, ShadowApi, TaskInputs, WorldComputeGraph,
    args::{CompileFontArgs, TaskWhen},
};
use typst::Features;
use typst::World;
use typst::foundations::{Bytes, Dict};
use typst::utils::LazyHash;
use typst_syntax::{DiagSpan, DiagSpanKind, Source};

use crate::EditorLsp;
use crate::types::*;

type ImmutDict = Arc<LazyHash<Dict>>;

pub struct TypstLsp {
    universe: LspUniverse,
    analysis: Arc<Analysis>,
    files: HashMap<PathBuf, String>,
}

impl TypstLsp {
    pub fn new(cache_dir: Option<PathBuf>, font_paths: Vec<PathBuf>) -> Result<Self, String> {
        let entry = EntryState::new_detached();

        let font_args = CompileFontArgs {
            font_paths,
            ignore_system_fonts: false,
        };
        let font_resolver = Arc::new(
            LspUniverseBuilder::resolve_fonts(font_args)
                .map_err(|e| format!("Failed to resolve fonts: {e:?}"))?,
        );

        let package_args = cache_dir.map(|d| tinymist_world::args::CompilePackageArgs {
            package_path: None,
            package_cache_path: Some(d.join("typst").join("packages")),
        });
        let package_registry = LspUniverseBuilder::resolve_package(None, package_args.as_ref());

        let access_model =
            DynAccessModel(Arc::new(tinymist_world::vfs::system::SystemAccessModel {}));

        let universe = LspUniverseBuilder::build(
            entry,
            tinymist_world::args::ExportTarget::Paged,
            Features::default(),
            ImmutDict::default(),
            package_registry,
            font_resolver,
            None,
            access_model,
        );

        let analysis = Arc::new(Analysis {
            position_encoding: PositionEncoding::Utf8,
            allow_overlapping_token: false,
            allow_multiline_token: true,
            remove_html: false,
            support_client_codelens: false,
            extended_code_action: false,
            completion_feat: CompletionFeat::default(),
            color_theme: ColorTheme::Dark,
            lint: TaskWhen::OnType,
            periscope: None,
            workers: Default::default(),
            local_packages: Default::default(),
            tokens_caches: Default::default(),
            caches: Default::default(),
            analysis_rev_cache: Default::default(),
            stats: Default::default(),
        });

        Ok(Self {
            universe,
            analysis,
            files: HashMap::new(),
        })
    }

    fn create_graph(&self, path: &Path) -> Result<tinymist_project::LspComputeGraph, String> {
        let content = self
            .files
            .get(path)
            .ok_or_else(|| format!("file content not found: {}", path.display()))?;
        let immut_path: ImmutPath = Arc::from(path.to_path_buf().into_boxed_path());
        let entry = EntryState::new_rooted_by_parent(immut_path)
            .ok_or_else(|| format!("failed to create entry state for {}", path.display()))?;

        let mut world = self.universe.snapshot_with(Some(TaskInputs {
            entry: Some(entry),
            inputs: None,
        }));

        let main_id = world
            .main_id()
            .ok_or_else(|| "entry file not set".to_string())?;

        world
            .map_shadow_by_id(main_id, Bytes::from_string(content.clone()))
            .map_err(|e| format!("map_shadow_by_id failed: {e:?}"))?;

        let graph = WorldComputeGraph::from_world(world);
        Ok(graph)
    }
}

impl EditorLsp for TypstLsp {
    fn language_id(&self) -> &'static str {
        "typst"
    }

    fn update(&mut self, path: &Path, content: &str) {
        self.files.insert(path.to_path_buf(), content.to_string());
    }

    fn close(&mut self, path: &Path) {
        self.files.remove(path);
    }

    fn semantic_tokens(&self, path: &Path) -> Vec<SemanticToken> {
        let content = match self.files.get(path) {
            Some(c) => c,
            None => return Vec::new(),
        };
        let source = Source::detached(content);

        let graph = match self.create_graph(path) {
            Ok(g) => g,
            Err(_e) => return Vec::new(),
        };

        let mut guard: LocalContextGuard = self.analysis.enter(graph);
        let ctx: &mut LocalContext = &mut guard;

        let result = SemanticTokensFullRequest::compute(ctx, &source);

        result
            .iter()
            .map(|t| SemanticToken {
                delta_line: t.delta_line,
                delta_start: t.delta_start,
                length: t.length,
                token_type: map_token_type(t.token_type),
                modifiers: map_token_modifiers(t.token_modifiers_bitset),
            })
            .collect()
    }

    fn diagnostics(&self, path: &Path) -> Vec<LspDiagnostic> {
        let content = match self.files.get(path) {
            Some(c) => c,
            None => return Vec::new(),
        };

        let graph = match self.create_graph(path) {
            Ok(g) => g,
            Err(_e) => return Vec::new(),
        };

        let warned = graph.compile();

        let world = graph.world();
        let main_id = world.main();
        let source = match world.source(main_id) {
            Ok(s) => s,
            Err(_e) => Source::detached(content),
        };

        let mut diags = Vec::new();

        if let Err(errors) = &warned.output {
            for error in errors.iter() {
                if let Some(diag) = convert_typst_diagnostic(&source, error) {
                    diags.push(diag);
                }
            }
        }

        for warning in warned.warnings.iter() {
            if let Some(diag) = convert_typst_diagnostic(&source, warning) {
                diags.push(diag);
            }
        }

        diags
    }

    fn completions(&self, path: &Path, pos: TextPosition) -> Vec<Completion> {
        let graph = match self.create_graph(path) {
            Ok(g) => g,
            Err(_e) => return Vec::new(),
        };

        let mut guard: LocalContextGuard = self.analysis.enter(graph);
        let ctx: &mut LocalContext = &mut guard;

        let lsp_pos = lsp_types::Position::new(pos.line as u32, pos.column as u32);
        let request = CompletionRequest {
            path: path.into(),
            position: lsp_pos,
            explicit: true,
            trigger_character: None,
        };

        let result = request.request(ctx);
        match result {
            Some(completion_list) => {
                let json = serde_json::to_value(&completion_list).ok();
                let lsp_list: lsp_types::CompletionList = json
                    .and_then(|v| serde_json::from_value(v).ok())
                    .unwrap_or_default();
                lsp_list
                    .items
                    .into_iter()
                    .map(convert_lsp_completion_item)
                    .collect()
            }
            None => Vec::new(),
        }
    }

    fn hover(&self, path: &Path, pos: TextPosition) -> Option<HoverInfo> {
        let content = self.files.get(path)?;
        let source = Source::detached(content);

        let graph = match self.create_graph(path) {
            Ok(g) => g,
            Err(_e) => return None,
        };

        let mut guard: LocalContextGuard = self.analysis.enter(graph);
        let ctx: &mut LocalContext = &mut guard;

        let lsp_pos = lsp_types::Position::new(pos.line as u32, pos.column as u32);
        let request = HoverRequest {
            path: path.into(),
            position: lsp_pos,
        };

        request
            .request(ctx)
            .map(|hover| convert_hover(hover, &source))
    }

    fn goto_definition(&self, path: &Path, pos: TextPosition) -> Option<Location> {
        let content = self.files.get(path)?;
        let source = Source::detached(content);

        let graph = match self.create_graph(path) {
            Ok(g) => g,
            Err(_e) => return None,
        };

        let mut guard: LocalContextGuard = self.analysis.enter(graph);
        let ctx: &mut LocalContext = &mut guard;

        let lsp_pos = lsp_types::Position::new(pos.line as u32, pos.column as u32);
        let request = GotoDefinitionRequest {
            path: path.into(),
            position: lsp_pos,
        };

        request
            .request(ctx)
            .and_then(|resp| convert_goto_definition_response(resp, &source))
    }

    fn references(&self, path: &Path, pos: TextPosition) -> Vec<Location> {
        let graph = match self.create_graph(path) {
            Ok(g) => g,
            Err(_e) => return Vec::new(),
        };

        let mut guard: LocalContextGuard = self.analysis.enter(graph);
        let ctx: &mut LocalContext = &mut guard;

        let lsp_pos = lsp_types::Position::new(pos.line as u32, pos.column as u32);
        let request = ReferencesRequest {
            path: path.into(),
            position: lsp_pos,
        };

        request
            .request(ctx)
            .map(|locations| {
                locations
                    .into_iter()
                    .filter_map(convert_lsp_location)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn signature_help(
        &self,
        path: &Path,
        pos: TextPosition,
    ) -> Option<crate::types::SignatureHelp> {
        let graph = self.create_graph(path).ok()?;

        let mut guard: LocalContextGuard = self.analysis.enter(graph);
        let ctx: &mut LocalContext = &mut guard;

        let lsp_pos = lsp_types::Position::new(pos.line as u32, pos.column as u32);
        let request = SignatureHelpRequest {
            path: path.into(),
            position: lsp_pos,
        };

        request.request(ctx).map(convert_signature_help)
    }

    fn document_symbols(&self, path: &Path) -> Vec<DocumentSymbol> {
        let content = match self.files.get(path) {
            Some(c) => c,
            None => return Vec::new(),
        };
        let source = Source::detached(content);

        let request = DocumentSymbolRequest { path: path.into() };

        let response = request.request(&source, PositionEncoding::Utf16);
        match response {
            Some(lsp_types::DocumentSymbolResponse::Nested(symbols)) => symbols
                .into_iter()
                .filter_map(|s| convert_document_symbol(s, &source))
                .collect(),
            Some(lsp_types::DocumentSymbolResponse::Flat(symbols)) => symbols
                .into_iter()
                .filter_map(|info| convert_symbol_information(info, &source))
                .collect(),
            None => Vec::new(),
        }
    }

    fn folding_ranges(&self, path: &Path) -> Vec<FoldingRange> {
        let content = match self.files.get(path) {
            Some(c) => c,
            None => return Vec::new(),
        };
        let source = Source::detached(content);

        let request = FoldingRangeRequest {
            path: path.into(),
            line_folding_only: false,
        };

        request
            .request(&source, PositionEncoding::Utf16)
            .map(|ranges| {
                ranges
                    .into_iter()
                    .map(|r| FoldingRange {
                        start_line: r.start_line as usize,
                        end_line: r.end_line as usize,
                        kind: r.kind.map(|k| match k {
                            lsp_types::FoldingRangeKind::Comment => FoldingRangeKind::Comment,
                            lsp_types::FoldingRangeKind::Imports => FoldingRangeKind::Imports,
                            lsp_types::FoldingRangeKind::Region => FoldingRangeKind::Region,
                        }),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn code_actions(&self, path: &Path, range: TextRange) -> Vec<CodeAction> {
        let graph = match self.create_graph(path) {
            Ok(g) => g,
            Err(_e) => return Vec::new(),
        };

        let mut guard: LocalContextGuard = self.analysis.enter(graph);
        let ctx: &mut LocalContext = &mut guard;

        let lsp_range = lsp_types::Range {
            start: lsp_types::Position::new(range.start.line as u32, range.start.column as u32),
            end: lsp_types::Position::new(range.end.line as u32, range.end.column as u32),
        };
        let request = CodeActionRequest {
            path: path.into(),
            range: lsp_range,
            context: lsp_types::CodeActionContext {
                diagnostics: Vec::new(),
                only: None,
                trigger_kind: None,
            },
        };

        request
            .request(ctx)
            .map(|actions| {
                actions
                    .into_iter()
                    .filter_map(|a| {
                        serde_json::to_value(&a)
                            .ok()
                            .and_then(|v| serde_json::from_value::<lsp_types::CodeAction>(v).ok())
                            .and_then(convert_code_action)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn document_highlight(&self, path: &Path, pos: TextPosition) -> Vec<TextRange> {
        let graph = match self.create_graph(path) {
            Ok(g) => g,
            Err(_e) => return Vec::new(),
        };

        let mut guard: LocalContextGuard = self.analysis.enter(graph);
        let ctx: &mut LocalContext = &mut guard;

        let lsp_pos = lsp_types::Position::new(pos.line as u32, pos.column as u32);
        let request = DocumentHighlightRequest {
            path: path.into(),
            position: lsp_pos,
        };

        request
            .request(ctx)
            .map(|highlights| {
                highlights
                    .into_iter()
                    .filter_map(|h| convert_lsp_range(h.range))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn rename(&self, path: &Path, pos: TextPosition, new_name: &str) -> Vec<TextEdit> {
        let graph = match self.create_graph(path) {
            Ok(g) => g,
            Err(_e) => return Vec::new(),
        };

        let mut guard: LocalContextGuard = self.analysis.enter(graph);
        let ctx: &mut LocalContext = &mut guard;

        let lsp_pos = lsp_types::Position::new(pos.line as u32, pos.column as u32);
        let request = RenameRequest {
            path: path.into(),
            position: lsp_pos,
            new_name: new_name.to_string(),
        };

        let result = request.request(ctx);
        match result {
            Some(workspace_edit) => {
                let json = serde_json::to_value(&workspace_edit).ok();
                let lsp_edit: lsp_types::WorkspaceEdit = json
                    .and_then(|v| serde_json::from_value(v).ok())
                    .unwrap_or_default();
                extract_text_edits(lsp_edit)
            }
            None => Vec::new(),
        }
    }

    fn inlay_hints(&self, path: &Path) -> Vec<InlayHint> {
        let graph = match self.create_graph(path) {
            Ok(g) => g,
            Err(_e) => return Vec::new(),
        };

        let mut guard: LocalContextGuard = self.analysis.enter(graph);
        let ctx: &mut LocalContext = &mut guard;

        let request = InlayHintRequest {
            path: path.into(),
            range: lsp_types::Range {
                start: lsp_types::Position::new(0, 0),
                end: lsp_types::Position::new(u32::MAX, u32::MAX),
            },
        };

        request
            .request(ctx)
            .map(|hints| {
                hints
                    .into_iter()
                    .filter_map(convert_lsp_inlay_hint)
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn map_token_type(tt: u32) -> SemanticTokenType {
    match tt {
        0 => SemanticTokenType::Namespace,
        1 => SemanticTokenType::Type,
        2 => SemanticTokenType::Class,
        3 => SemanticTokenType::Enum,
        4 => SemanticTokenType::Interface,
        5 => SemanticTokenType::Struct,
        6 => SemanticTokenType::TypeParameter,
        7 => SemanticTokenType::Parameter,
        8 => SemanticTokenType::Variable,
        9 => SemanticTokenType::Property,
        10 => SemanticTokenType::EnumMember,
        11 => SemanticTokenType::Function,
        12 => SemanticTokenType::Method,
        13 => SemanticTokenType::Macro,
        14 => SemanticTokenType::Keyword,
        15 => SemanticTokenType::Modifier,
        16 => SemanticTokenType::Comment,
        17 => SemanticTokenType::String,
        18 => SemanticTokenType::Number,
        19 => SemanticTokenType::Regexp,
        20 => SemanticTokenType::Operator,
        21 => SemanticTokenType::Decorator,
        _ => SemanticTokenType::Variable,
    }
}

fn map_token_modifiers(bitset: u32) -> Vec<SemanticTokenModifier> {
    let mut modifiers = Vec::new();
    if bitset & (1 << 0) != 0 {
        modifiers.push(SemanticTokenModifier::Declaration);
    }
    if bitset & (1 << 1) != 0 {
        modifiers.push(SemanticTokenModifier::Definition);
    }
    if bitset & (1 << 2) != 0 {
        modifiers.push(SemanticTokenModifier::Readonly);
    }
    if bitset & (1 << 3) != 0 {
        modifiers.push(SemanticTokenModifier::Static);
    }
    if bitset & (1 << 4) != 0 {
        modifiers.push(SemanticTokenModifier::Deprecated);
    }
    if bitset & (1 << 5) != 0 {
        modifiers.push(SemanticTokenModifier::Abstract);
    }
    if bitset & (1 << 6) != 0 {
        modifiers.push(SemanticTokenModifier::Async);
    }
    if bitset & (1 << 7) != 0 {
        modifiers.push(SemanticTokenModifier::Modification);
    }
    if bitset & (1 << 8) != 0 {
        modifiers.push(SemanticTokenModifier::Documentation);
    }
    if bitset & (1 << 9) != 0 {
        modifiers.push(SemanticTokenModifier::DefaultLibrary);
    }
    modifiers
}

fn convert_lsp_completion_item(item: lsp_types::CompletionItem) -> Completion {
    let label = item.label;
    let detail = item.detail;
    let insert_text = item.insert_text.unwrap_or_else(|| label.clone());
    let sort_text = item.sort_text;
    let kind = item.kind.map(|k| match k {
        lsp_types::CompletionItemKind::TEXT => CompletionKind::Text,
        lsp_types::CompletionItemKind::METHOD => CompletionKind::Function,
        lsp_types::CompletionItemKind::FUNCTION => CompletionKind::Function,
        lsp_types::CompletionItemKind::CONSTRUCTOR => CompletionKind::Function,
        lsp_types::CompletionItemKind::FIELD => CompletionKind::Field,
        lsp_types::CompletionItemKind::VARIABLE => CompletionKind::Variable,
        lsp_types::CompletionItemKind::CLASS => CompletionKind::Class,
        lsp_types::CompletionItemKind::INTERFACE => CompletionKind::Class,
        lsp_types::CompletionItemKind::MODULE => CompletionKind::Module,
        lsp_types::CompletionItemKind::PROPERTY => CompletionKind::Field,
        lsp_types::CompletionItemKind::UNIT => CompletionKind::Constant,
        lsp_types::CompletionItemKind::VALUE => CompletionKind::Constant,
        lsp_types::CompletionItemKind::ENUM => CompletionKind::Class,
        lsp_types::CompletionItemKind::KEYWORD => CompletionKind::Text,
        lsp_types::CompletionItemKind::SNIPPET => CompletionKind::Text,
        lsp_types::CompletionItemKind::COLOR => CompletionKind::Constant,
        lsp_types::CompletionItemKind::FILE => CompletionKind::Module,
        lsp_types::CompletionItemKind::REFERENCE => CompletionKind::Reference,
        lsp_types::CompletionItemKind::FOLDER => CompletionKind::Module,
        _ => CompletionKind::Text,
    });
    Completion {
        label,
        detail,
        kind,
        insert_text,
        sort_text,
    }
}

fn convert_typst_diagnostic(
    source: &Source,
    diag: &typst::diag::SourceDiagnostic,
) -> Option<LspDiagnostic> {
    let range = convert_diag_span(source, diag.span)?;
    let severity = match diag.severity {
        typst::diag::Severity::Error => DiagnosticSeverity::Error,
        typst::diag::Severity::Warning => DiagnosticSeverity::Warning,
    };
    Some(LspDiagnostic {
        range,
        severity,
        code: None,
        message: diag.message.to_string(),
    })
}

fn convert_diag_span(source: &Source, diag_span: DiagSpan) -> Option<TextRange> {
    match diag_span.get() {
        DiagSpanKind::Range { range, .. } => byte_range_to_text_range(source, range),
        DiagSpanKind::Number { num, sub_range, .. } => {
            byte_range_to_text_range(source, source.range(num, sub_range)?)
        }
        DiagSpanKind::Detached => None,
    }
}

fn byte_range_to_text_range(source: &Source, range: Range<usize>) -> Option<TextRange> {
    let text = source.text();
    let start = byte_offset_to_position(text, range.start)?;
    let end = byte_offset_to_position(text, range.end)?;
    Some(TextRange { start, end })
}

fn byte_offset_to_position(text: &str, offset: usize) -> Option<TextPosition> {
    if offset > text.len() {
        return None;
    }
    let prefix = &text[..offset];
    let line = prefix.chars().filter(|&c| c == '\n').count();
    let last_newline = prefix.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let column = offset - last_newline;
    Some(TextPosition { line, column })
}

fn convert_hover(hover: lsp_types::Hover, source: &Source) -> HoverInfo {
    let contents = convert_hover_contents(hover.contents);
    let range = hover.range.and_then(|r| lsp_range_to_text_range(r, source));
    HoverInfo { contents, range }
}

fn convert_hover_contents(contents: lsp_types::HoverContents) -> String {
    match contents {
        lsp_types::HoverContents::Scalar(marked) => convert_marked_string(marked),
        lsp_types::HoverContents::Array(markeds) => markeds
            .into_iter()
            .map(convert_marked_string)
            .collect::<Vec<_>>()
            .join("\n\n"),
        lsp_types::HoverContents::Markup(markup) => markup.value,
    }
}

fn convert_marked_string(marked: lsp_types::MarkedString) -> String {
    match marked {
        lsp_types::MarkedString::String(s) => s,
        lsp_types::MarkedString::LanguageString(ls) => ls.value,
    }
}

fn convert_goto_definition_response(
    resp: lsp_types::GotoDefinitionResponse,
    source: &Source,
) -> Option<Location> {
    match resp {
        lsp_types::GotoDefinitionResponse::Scalar(loc) => convert_lsp_location(loc),
        lsp_types::GotoDefinitionResponse::Array(arr) => {
            arr.into_iter().next().and_then(convert_lsp_location)
        }
        lsp_types::GotoDefinitionResponse::Link(links) => links.into_iter().next().map(|link| {
            let range = lsp_range_to_text_range(link.target_range, source).unwrap_or(TextRange {
                start: TextPosition { line: 0, column: 0 },
                end: TextPosition { line: 0, column: 0 },
            });
            Location {
                path: PathBuf::from(link.target_uri.path()),
                range,
            }
        }),
    }
}

fn convert_lsp_location(loc: lsp_types::Location) -> Option<Location> {
    let range = TextRange {
        start: TextPosition {
            line: loc.range.start.line as usize,
            column: loc.range.start.character as usize,
        },
        end: TextPosition {
            line: loc.range.end.line as usize,
            column: loc.range.end.character as usize,
        },
    };
    Some(Location {
        path: PathBuf::from(loc.uri.path()),
        range,
    })
}

fn lsp_range_to_text_range(range: lsp_types::Range, _source: &Source) -> Option<TextRange> {
    Some(TextRange {
        start: TextPosition {
            line: range.start.line as usize,
            column: range.start.character as usize,
        },
        end: TextPosition {
            line: range.end.line as usize,
            column: range.end.character as usize,
        },
    })
}

fn convert_signature_help(sig: lsp_types::SignatureHelp) -> crate::types::SignatureHelp {
    let label = sig
        .signatures
        .first()
        .map(|s| s.label.clone())
        .unwrap_or_default();
    let documentation = sig
        .signatures
        .first()
        .and_then(|s| s.documentation.as_ref())
        .map(|d| match d {
            lsp_types::Documentation::String(s) => s.clone(),
            lsp_types::Documentation::MarkupContent(m) => m.value.clone(),
        });
    let active_parameter = sig
        .active_signature
        .and_then(|i| sig.signatures.get(i as usize))
        .and_then(|s| s.active_parameter)
        .map(|p| p as usize);
    let parameters = sig
        .signatures
        .first()
        .map(|s| {
            s.parameters
                .as_ref()
                .map(|params| {
                    params
                        .iter()
                        .map(|p| ParameterInfo {
                            label: format!("{label:?}", label = p.label),
                            documentation: p.documentation.as_ref().map(|d| match d {
                                lsp_types::Documentation::String(s) => s.clone(),
                                lsp_types::Documentation::MarkupContent(m) => m.value.clone(),
                            }),
                        })
                        .collect()
                })
                .unwrap_or_default()
        })
        .unwrap_or_default();
    crate::types::SignatureHelp {
        label,
        documentation,
        parameters,
        active_parameter,
    }
}

fn convert_document_symbol(
    symbol: lsp_types::DocumentSymbol,
    source: &Source,
) -> Option<DocumentSymbol> {
    let range = lsp_range_to_text_range(symbol.range, source)?;
    let selection_range = lsp_range_to_text_range(symbol.selection_range, source)?;
    let kind = convert_symbol_kind(symbol.kind);
    let children = symbol
        .children
        .unwrap_or_default()
        .into_iter()
        .filter_map(|c| convert_document_symbol(c, source))
        .collect();
    Some(DocumentSymbol {
        name: symbol.name,
        detail: symbol.detail,
        kind,
        range,
        selection_range,
        children,
    })
}

fn convert_symbol_information(
    info: lsp_types::SymbolInformation,
    source: &Source,
) -> Option<DocumentSymbol> {
    let range = lsp_range_to_text_range(info.location.range, source)?;
    Some(DocumentSymbol {
        name: info.name,
        detail: info.container_name,
        kind: convert_symbol_kind(info.kind),
        range,
        selection_range: range,
        children: Vec::new(),
    })
}

fn convert_symbol_kind(kind: lsp_types::SymbolKind) -> SymbolKind {
    if kind == lsp_types::SymbolKind::FILE {
        SymbolKind::File
    } else if kind == lsp_types::SymbolKind::MODULE {
        SymbolKind::Module
    } else if kind == lsp_types::SymbolKind::NAMESPACE {
        SymbolKind::Namespace
    } else if kind == lsp_types::SymbolKind::PACKAGE {
        SymbolKind::Package
    } else if kind == lsp_types::SymbolKind::CLASS {
        SymbolKind::Class
    } else if kind == lsp_types::SymbolKind::METHOD {
        SymbolKind::Method
    } else if kind == lsp_types::SymbolKind::PROPERTY {
        SymbolKind::Property
    } else if kind == lsp_types::SymbolKind::FIELD {
        SymbolKind::Field
    } else if kind == lsp_types::SymbolKind::CONSTRUCTOR {
        SymbolKind::Constructor
    } else if kind == lsp_types::SymbolKind::ENUM {
        SymbolKind::Enum
    } else if kind == lsp_types::SymbolKind::INTERFACE {
        SymbolKind::Interface
    } else if kind == lsp_types::SymbolKind::FUNCTION {
        SymbolKind::Function
    } else if kind == lsp_types::SymbolKind::VARIABLE {
        SymbolKind::Variable
    } else if kind == lsp_types::SymbolKind::CONSTANT {
        SymbolKind::Constant
    } else if kind == lsp_types::SymbolKind::STRING {
        SymbolKind::String
    } else if kind == lsp_types::SymbolKind::NUMBER {
        SymbolKind::Number
    } else if kind == lsp_types::SymbolKind::BOOLEAN {
        SymbolKind::Boolean
    } else if kind == lsp_types::SymbolKind::ARRAY {
        SymbolKind::Array
    } else if kind == lsp_types::SymbolKind::OBJECT {
        SymbolKind::Object
    } else if kind == lsp_types::SymbolKind::KEY {
        SymbolKind::Key
    } else if kind == lsp_types::SymbolKind::NULL {
        SymbolKind::Null
    } else if kind == lsp_types::SymbolKind::ENUM_MEMBER {
        SymbolKind::EnumMember
    } else if kind == lsp_types::SymbolKind::STRUCT {
        SymbolKind::Struct
    } else if kind == lsp_types::SymbolKind::EVENT {
        SymbolKind::Event
    } else if kind == lsp_types::SymbolKind::OPERATOR {
        SymbolKind::Operator
    } else if kind == lsp_types::SymbolKind::TYPE_PARAMETER {
        SymbolKind::TypeParameter
    } else {
        SymbolKind::Variable
    }
}

fn convert_code_action(action: lsp_types::CodeAction) -> Option<CodeAction> {
    let edit = action.edit.and_then(|e| {
        if let Some(changes) = e.changes {
            changes.into_values().next().and_then(|edits| {
                edits.into_iter().next().map(|edit| {
                    let range = lsp_range_to_text_range_simple(edit.range);
                    TextEdit {
                        range,
                        new_text: edit.new_text,
                    }
                })
            })
        } else if let Some(dc) = e.document_changes {
            match dc {
                lsp_types::DocumentChanges::Edits(edits) => {
                    edits.into_iter().next().and_then(|doc_edit| {
                        doc_edit.edits.into_iter().next().map(|edit| {
                            let (range, new_text) = extract_one_of_text_edit(edit);
                            TextEdit { range, new_text }
                        })
                    })
                }
                _ => None,
            }
        } else {
            None
        }
    });
    Some(CodeAction {
        title: action.title,
        kind: action.kind.map(|k| format!("{k:?}")),
        edit,
    })
}

fn lsp_range_to_text_range_simple(range: lsp_types::Range) -> TextRange {
    TextRange {
        start: TextPosition {
            line: range.start.line as usize,
            column: range.start.character as usize,
        },
        end: TextPosition {
            line: range.end.line as usize,
            column: range.end.character as usize,
        },
    }
}

fn extract_one_of_text_edit(
    edit: lsp_types::OneOf<lsp_types::TextEdit, lsp_types::AnnotatedTextEdit>,
) -> (TextRange, String) {
    match edit {
        lsp_types::OneOf::Left(e) => (lsp_range_to_text_range_simple(e.range), e.new_text),
        lsp_types::OneOf::Right(e) => {
            let range = lsp_range_to_text_range_simple(e.text_edit.range);
            (range, e.text_edit.new_text)
        }
    }
}

fn convert_lsp_range(range: lsp_types::Range) -> Option<TextRange> {
    Some(TextRange {
        start: TextPosition {
            line: range.start.line as usize,
            column: range.start.character as usize,
        },
        end: TextPosition {
            line: range.end.line as usize,
            column: range.end.character as usize,
        },
    })
}

fn extract_text_edits(edit: lsp_types::WorkspaceEdit) -> Vec<TextEdit> {
    let mut result = Vec::new();
    if let Some(changes) = edit.changes {
        for (_, edits) in changes {
            for e in edits {
                result.push(TextEdit {
                    range: lsp_range_to_text_range_simple(e.range),
                    new_text: e.new_text,
                });
            }
        }
    }
    if let Some(lsp_types::DocumentChanges::Edits(edits)) = edit.document_changes {
        for doc_edit in edits {
            for e in doc_edit.edits {
                let (range, new_text) = extract_one_of_text_edit(e);
                result.push(TextEdit { range, new_text });
            }
        }
    }
    result
}

fn convert_lsp_inlay_hint(hint: lsp_types::InlayHint) -> Option<InlayHint> {
    let label = match hint.label {
        lsp_types::InlayHintLabel::String(s) => s,
        lsp_types::InlayHintLabel::LabelParts(parts) => {
            parts.into_iter().map(|p| p.value).collect::<String>()
        }
    };
    let kind = hint.kind.map(|k| {
        if k == lsp_types::InlayHintKind::TYPE {
            InlayHintKind::Type
        } else {
            InlayHintKind::Parameter
        }
    });
    Some(InlayHint {
        position: TextPosition {
            line: hint.position.line as usize,
            column: hint.position.character as usize,
        },
        label,
        kind,
        padding_left: hint.padding_left.unwrap_or(false),
        padding_right: hint.padding_right.unwrap_or(false),
    })
}

#[cfg(test)]
#[expect(deprecated, unsafe_code)]
mod tests {
    use super::*;

    #[test]
    fn token_type_namespace() {
        assert_eq!(map_token_type(0), SemanticTokenType::Namespace);
    }

    #[test]
    fn token_type_type() {
        assert_eq!(map_token_type(1), SemanticTokenType::Type);
    }

    #[test]
    fn token_type_class() {
        assert_eq!(map_token_type(2), SemanticTokenType::Class);
    }

    #[test]
    fn token_type_enum() {
        assert_eq!(map_token_type(3), SemanticTokenType::Enum);
    }

    #[test]
    fn token_type_interface() {
        assert_eq!(map_token_type(4), SemanticTokenType::Interface);
    }

    #[test]
    fn token_type_struct() {
        assert_eq!(map_token_type(5), SemanticTokenType::Struct);
    }

    #[test]
    fn token_type_type_parameter() {
        assert_eq!(map_token_type(6), SemanticTokenType::TypeParameter);
    }

    #[test]
    fn token_type_parameter() {
        assert_eq!(map_token_type(7), SemanticTokenType::Parameter);
    }

    #[test]
    fn token_type_variable() {
        assert_eq!(map_token_type(8), SemanticTokenType::Variable);
    }

    #[test]
    fn token_type_property() {
        assert_eq!(map_token_type(9), SemanticTokenType::Property);
    }

    #[test]
    fn token_type_enum_member() {
        assert_eq!(map_token_type(10), SemanticTokenType::EnumMember);
    }

    #[test]
    fn token_type_function() {
        assert_eq!(map_token_type(11), SemanticTokenType::Function);
    }

    #[test]
    fn token_type_method() {
        assert_eq!(map_token_type(12), SemanticTokenType::Method);
    }

    #[test]
    fn token_type_macro() {
        assert_eq!(map_token_type(13), SemanticTokenType::Macro);
    }

    #[test]
    fn token_type_keyword() {
        assert_eq!(map_token_type(14), SemanticTokenType::Keyword);
    }

    #[test]
    fn token_type_modifier() {
        assert_eq!(map_token_type(15), SemanticTokenType::Modifier);
    }

    #[test]
    fn token_type_comment() {
        assert_eq!(map_token_type(16), SemanticTokenType::Comment);
    }

    #[test]
    fn token_type_string() {
        assert_eq!(map_token_type(17), SemanticTokenType::String);
    }

    #[test]
    fn token_type_number() {
        assert_eq!(map_token_type(18), SemanticTokenType::Number);
    }

    #[test]
    fn token_type_regexp() {
        assert_eq!(map_token_type(19), SemanticTokenType::Regexp);
    }

    #[test]
    fn token_type_operator() {
        assert_eq!(map_token_type(20), SemanticTokenType::Operator);
    }

    #[test]
    fn token_type_decorator() {
        assert_eq!(map_token_type(21), SemanticTokenType::Decorator);
    }

    #[test]
    fn token_type_unknown_falls_back_to_variable() {
        assert_eq!(map_token_type(255), SemanticTokenType::Variable);
        assert_eq!(map_token_type(u32::MAX), SemanticTokenType::Variable);
    }

    #[test]
    fn token_modifiers_empty() {
        let modifiers = map_token_modifiers(0);
        assert!(modifiers.is_empty());
    }

    #[test]
    fn token_modifiers_declaration() {
        let modifiers = map_token_modifiers(1 << 0);
        assert!(modifiers.contains(&SemanticTokenModifier::Declaration));
    }

    #[test]
    fn token_modifiers_definition() {
        let modifiers = map_token_modifiers(1 << 1);
        assert!(modifiers.contains(&SemanticTokenModifier::Definition));
    }

    #[test]
    fn token_modifiers_readonly() {
        let modifiers = map_token_modifiers(1 << 2);
        assert!(modifiers.contains(&SemanticTokenModifier::Readonly));
    }

    #[test]
    fn token_modifiers_static() {
        let modifiers = map_token_modifiers(1 << 3);
        assert!(modifiers.contains(&SemanticTokenModifier::Static));
    }

    #[test]
    fn token_modifiers_deprecated() {
        let modifiers = map_token_modifiers(1 << 4);
        assert!(modifiers.contains(&SemanticTokenModifier::Deprecated));
    }

    #[test]
    fn token_modifiers_abstract() {
        let modifiers = map_token_modifiers(1 << 5);
        assert!(modifiers.contains(&SemanticTokenModifier::Abstract));
    }

    #[test]
    fn token_modifiers_async() {
        let modifiers = map_token_modifiers(1 << 6);
        assert!(modifiers.contains(&SemanticTokenModifier::Async));
    }

    #[test]
    fn token_modifiers_modification() {
        let modifiers = map_token_modifiers(1 << 7);
        assert!(modifiers.contains(&SemanticTokenModifier::Modification));
    }

    #[test]
    fn token_modifiers_documentation() {
        let modifiers = map_token_modifiers(1 << 8);
        assert!(modifiers.contains(&SemanticTokenModifier::Documentation));
    }

    #[test]
    fn token_modifiers_default_library() {
        let modifiers = map_token_modifiers(1 << 9);
        assert!(modifiers.contains(&SemanticTokenModifier::DefaultLibrary));
    }

    #[test]
    fn token_modifiers_multiple() {
        let modifiers = map_token_modifiers((1 << 0) | (1 << 4) | (1 << 8));
        assert_eq!(modifiers.len(), 3);
        assert!(modifiers.contains(&SemanticTokenModifier::Declaration));
        assert!(modifiers.contains(&SemanticTokenModifier::Deprecated));
        assert!(modifiers.contains(&SemanticTokenModifier::Documentation));
    }

    #[test]
    fn symbol_kind_file() {
        assert_eq!(
            convert_symbol_kind(lsp_types::SymbolKind::FILE),
            SymbolKind::File
        );
    }

    #[test]
    fn symbol_kind_module() {
        assert_eq!(
            convert_symbol_kind(lsp_types::SymbolKind::MODULE),
            SymbolKind::Module
        );
    }

    #[test]
    fn symbol_kind_namespace() {
        assert_eq!(
            convert_symbol_kind(lsp_types::SymbolKind::NAMESPACE),
            SymbolKind::Namespace
        );
    }

    #[test]
    fn symbol_kind_package() {
        assert_eq!(
            convert_symbol_kind(lsp_types::SymbolKind::PACKAGE),
            SymbolKind::Package
        );
    }

    #[test]
    fn symbol_kind_class() {
        assert_eq!(
            convert_symbol_kind(lsp_types::SymbolKind::CLASS),
            SymbolKind::Class
        );
    }

    #[test]
    fn symbol_kind_method() {
        assert_eq!(
            convert_symbol_kind(lsp_types::SymbolKind::METHOD),
            SymbolKind::Method
        );
    }

    #[test]
    fn symbol_kind_property() {
        assert_eq!(
            convert_symbol_kind(lsp_types::SymbolKind::PROPERTY),
            SymbolKind::Property
        );
    }

    #[test]
    fn symbol_kind_field() {
        assert_eq!(
            convert_symbol_kind(lsp_types::SymbolKind::FIELD),
            SymbolKind::Field
        );
    }

    #[test]
    fn symbol_kind_constructor() {
        assert_eq!(
            convert_symbol_kind(lsp_types::SymbolKind::CONSTRUCTOR),
            SymbolKind::Constructor
        );
    }

    #[test]
    fn symbol_kind_enum() {
        assert_eq!(
            convert_symbol_kind(lsp_types::SymbolKind::ENUM),
            SymbolKind::Enum
        );
    }

    #[test]
    fn symbol_kind_interface() {
        assert_eq!(
            convert_symbol_kind(lsp_types::SymbolKind::INTERFACE),
            SymbolKind::Interface
        );
    }

    #[test]
    fn symbol_kind_function() {
        assert_eq!(
            convert_symbol_kind(lsp_types::SymbolKind::FUNCTION),
            SymbolKind::Function
        );
    }

    #[test]
    fn symbol_kind_variable() {
        assert_eq!(
            convert_symbol_kind(lsp_types::SymbolKind::VARIABLE),
            SymbolKind::Variable
        );
    }

    #[test]
    fn symbol_kind_constant() {
        assert_eq!(
            convert_symbol_kind(lsp_types::SymbolKind::CONSTANT),
            SymbolKind::Constant
        );
    }

    #[test]
    fn symbol_kind_string() {
        assert_eq!(
            convert_symbol_kind(lsp_types::SymbolKind::STRING),
            SymbolKind::String
        );
    }

    #[test]
    fn symbol_kind_number() {
        assert_eq!(
            convert_symbol_kind(lsp_types::SymbolKind::NUMBER),
            SymbolKind::Number
        );
    }

    #[test]
    fn symbol_kind_boolean() {
        assert_eq!(
            convert_symbol_kind(lsp_types::SymbolKind::BOOLEAN),
            SymbolKind::Boolean
        );
    }

    #[test]
    fn symbol_kind_array() {
        assert_eq!(
            convert_symbol_kind(lsp_types::SymbolKind::ARRAY),
            SymbolKind::Array
        );
    }

    #[test]
    fn symbol_kind_object() {
        assert_eq!(
            convert_symbol_kind(lsp_types::SymbolKind::OBJECT),
            SymbolKind::Object
        );
    }

    #[test]
    fn symbol_kind_key() {
        assert_eq!(
            convert_symbol_kind(lsp_types::SymbolKind::KEY),
            SymbolKind::Key
        );
    }

    #[test]
    fn symbol_kind_null() {
        assert_eq!(
            convert_symbol_kind(lsp_types::SymbolKind::NULL),
            SymbolKind::Null
        );
    }

    #[test]
    fn symbol_kind_enum_member() {
        assert_eq!(
            convert_symbol_kind(lsp_types::SymbolKind::ENUM_MEMBER),
            SymbolKind::EnumMember
        );
    }

    #[test]
    fn symbol_kind_struct() {
        assert_eq!(
            convert_symbol_kind(lsp_types::SymbolKind::STRUCT),
            SymbolKind::Struct
        );
    }

    #[test]
    fn symbol_kind_event() {
        assert_eq!(
            convert_symbol_kind(lsp_types::SymbolKind::EVENT),
            SymbolKind::Event
        );
    }

    #[test]
    fn symbol_kind_operator() {
        assert_eq!(
            convert_symbol_kind(lsp_types::SymbolKind::OPERATOR),
            SymbolKind::Operator
        );
    }

    #[test]
    fn symbol_kind_type_parameter() {
        assert_eq!(
            convert_symbol_kind(lsp_types::SymbolKind::TYPE_PARAMETER),
            SymbolKind::TypeParameter
        );
    }

    #[test]
    fn symbol_kind_unknown_falls_back_to_variable() {
        let kind: lsp_types::SymbolKind = unsafe { std::mem::transmute(99u32) };
        assert_eq!(convert_symbol_kind(kind), SymbolKind::Variable);
    }

    #[test]
    fn inlay_hint_string_label() {
        let hint = lsp_types::InlayHint {
            position: lsp_types::Position::new(3, 10),
            label: lsp_types::InlayHintLabel::String("param".to_string()),
            kind: None,
            text_edits: None,
            tooltip: None,
            padding_left: None,
            padding_right: None,
            data: None,
        };
        let result = convert_lsp_inlay_hint(hint).unwrap();
        assert_eq!(result.label, "param");
        assert_eq!(result.position.line, 3);
        assert_eq!(result.position.column, 10);
        assert_eq!(result.kind, None);
        assert!(!result.padding_left);
        assert!(!result.padding_right);
    }

    #[test]
    fn inlay_hint_label_parts() {
        let part = lsp_types::InlayHintLabelPart {
            value: "x".to_string(),
            tooltip: None,
            location: None,
            command: None,
        };
        let hint = lsp_types::InlayHint {
            position: lsp_types::Position::new(0, 0),
            label: lsp_types::InlayHintLabel::LabelParts(vec![
                part.clone(),
                lsp_types::InlayHintLabelPart {
                    value: "y".to_string(),
                    ..part
                },
            ]),
            kind: None,
            text_edits: None,
            tooltip: None,
            padding_left: None,
            padding_right: None,
            data: None,
        };
        let result = convert_lsp_inlay_hint(hint).unwrap();
        assert_eq!(result.label, "xy");
    }

    #[test]
    fn inlay_hint_type_kind() {
        let hint = lsp_types::InlayHint {
            position: lsp_types::Position::new(0, 0),
            label: lsp_types::InlayHintLabel::String("i32".to_string()),
            kind: Some(lsp_types::InlayHintKind::TYPE),
            text_edits: None,
            tooltip: None,
            padding_left: Some(true),
            padding_right: Some(true),
            data: None,
        };
        let result = convert_lsp_inlay_hint(hint).unwrap();
        assert_eq!(result.kind, Some(InlayHintKind::Type));
        assert!(result.padding_left);
        assert!(result.padding_right);
    }

    #[test]
    fn inlay_hint_parameter_kind() {
        let hint = lsp_types::InlayHint {
            position: lsp_types::Position::new(0, 0),
            label: lsp_types::InlayHintLabel::String("name".to_string()),
            kind: Some(lsp_types::InlayHintKind::PARAMETER),
            text_edits: None,
            tooltip: None,
            padding_left: None,
            padding_right: None,
            data: None,
        };
        let result = convert_lsp_inlay_hint(hint).unwrap();
        assert_eq!(result.kind, Some(InlayHintKind::Parameter));
    }

    #[test]
    fn completion_item_all_fields() {
        let item = lsp_types::CompletionItem {
            label: "my_func".to_string(),
            detail: Some("fn my_func(x: int) -> str".to_string()),
            insert_text: Some("my_func($1)".to_string()),
            sort_text: Some("a".to_string()),
            kind: Some(lsp_types::CompletionItemKind::FUNCTION),
            ..Default::default()
        };
        let result = convert_lsp_completion_item(item);
        assert_eq!(result.label, "my_func");
        assert_eq!(result.detail, Some("fn my_func(x: int) -> str".to_string()));
        assert_eq!(result.insert_text, "my_func($1)");
        assert_eq!(result.sort_text, Some("a".to_string()));
        assert_eq!(result.kind, Some(CompletionKind::Function));
    }

    #[test]
    fn completion_item_insert_text_falls_back_to_label() {
        let item = lsp_types::CompletionItem {
            label: "foo".to_string(),
            insert_text: None,
            ..Default::default()
        };
        let result = convert_lsp_completion_item(item);
        assert_eq!(result.insert_text, "foo");
    }

    #[test]
    fn completion_item_kind_method() {
        let item = lsp_types::CompletionItem {
            label: "bar".to_string(),
            kind: Some(lsp_types::CompletionItemKind::METHOD),
            ..Default::default()
        };
        let result = convert_lsp_completion_item(item);
        assert_eq!(result.kind, Some(CompletionKind::Function));
    }

    #[test]
    fn completion_item_kind_variable() {
        let item = lsp_types::CompletionItem {
            label: "x".to_string(),
            kind: Some(lsp_types::CompletionItemKind::VARIABLE),
            ..Default::default()
        };
        let result = convert_lsp_completion_item(item);
        assert_eq!(result.kind, Some(CompletionKind::Variable));
    }

    #[test]
    fn completion_item_kind_field() {
        let item = lsp_types::CompletionItem {
            label: "field".to_string(),
            kind: Some(lsp_types::CompletionItemKind::FIELD),
            ..Default::default()
        };
        let result = convert_lsp_completion_item(item);
        assert_eq!(result.kind, Some(CompletionKind::Field));
    }

    #[test]
    fn completion_item_kind_module() {
        let item = lsp_types::CompletionItem {
            label: "mod".to_string(),
            kind: Some(lsp_types::CompletionItemKind::MODULE),
            ..Default::default()
        };
        let result = convert_lsp_completion_item(item);
        assert_eq!(result.kind, Some(CompletionKind::Module));
    }

    #[test]
    fn completion_item_kind_unknown() {
        let kind: lsp_types::CompletionItemKind = unsafe { std::mem::transmute(99u32) };
        let item = lsp_types::CompletionItem {
            label: "unknown".to_string(),
            kind: Some(kind),
            ..Default::default()
        };
        let result = convert_lsp_completion_item(item);
        assert_eq!(result.kind, Some(CompletionKind::Text));
    }

    #[test]
    fn hover_contents_scalar() {
        let contents = lsp_types::HoverContents::Scalar(lsp_types::MarkedString::String(
            "hover text".to_string(),
        ));
        let result = convert_hover_contents(contents);
        assert_eq!(result, "hover text");
    }

    #[test]
    fn hover_contents_array() {
        let contents = lsp_types::HoverContents::Array(vec![
            lsp_types::MarkedString::String("first".to_string()),
            lsp_types::MarkedString::String("second".to_string()),
        ]);
        let result = convert_hover_contents(contents);
        assert_eq!(result, "first\n\nsecond");
    }

    #[test]
    fn hover_contents_markup() {
        let markup = lsp_types::MarkupContent {
            kind: lsp_types::MarkupKind::Markdown,
            value: "**bold**".to_string(),
        };
        let contents = lsp_types::HoverContents::Markup(markup);
        let result = convert_hover_contents(contents);
        assert_eq!(result, "**bold**");
    }

    #[test]
    fn marked_string_simple() {
        let result = convert_marked_string(lsp_types::MarkedString::String("hello".to_string()));
        assert_eq!(result, "hello");
    }

    #[test]
    fn marked_string_language() {
        let ls = lsp_types::LanguageString {
            language: "rust".to_string(),
            value: "fn main() {}".to_string(),
        };
        let result = convert_marked_string(lsp_types::MarkedString::LanguageString(ls));
        assert_eq!(result, "fn main() {}");
    }

    #[test]
    fn convert_lsp_range_basic() {
        let range = lsp_types::Range {
            start: lsp_types::Position::new(2, 5),
            end: lsp_types::Position::new(2, 10),
        };
        let result = convert_lsp_range(range).unwrap();
        assert_eq!(result.start.line, 2);
        assert_eq!(result.start.column, 5);
        assert_eq!(result.end.line, 2);
        assert_eq!(result.end.column, 10);
    }

    #[test]
    fn lsp_range_to_text_range_simple_basic() {
        let range = lsp_types::Range {
            start: lsp_types::Position::new(1, 3),
            end: lsp_types::Position::new(4, 7),
        };
        let result = lsp_range_to_text_range_simple(range);
        assert_eq!(result.start.line, 1);
        assert_eq!(result.start.column, 3);
        assert_eq!(result.end.line, 4);
        assert_eq!(result.end.column, 7);
    }

    #[test]
    fn convert_lsp_location_basic() {
        let loc = lsp_types::Location {
            uri: lsp_types::Url::parse("file:///test.typ").unwrap(),
            range: lsp_types::Range {
                start: lsp_types::Position::new(5, 0),
                end: lsp_types::Position::new(5, 10),
            },
        };
        let result = convert_lsp_location(loc).unwrap();
        assert_eq!(result.path, PathBuf::from("/test.typ"));
        assert_eq!(result.range.start.line, 5);
        assert_eq!(result.range.start.column, 0);
        assert_eq!(result.range.end.line, 5);
        assert_eq!(result.range.end.column, 10);
    }

    #[test]
    fn lsp_range_to_text_range_basic() {
        let source = Source::detached("test");
        let range = lsp_types::Range {
            start: lsp_types::Position::new(3, 4),
            end: lsp_types::Position::new(3, 8),
        };
        let result = lsp_range_to_text_range(range, &source).unwrap();
        assert_eq!(result.start.line, 3);
        assert_eq!(result.start.column, 4);
        assert_eq!(result.end.line, 3);
        assert_eq!(result.end.column, 8);
    }

    #[test]
    fn hover_basic() {
        let source = Source::detached("content");
        let hover = lsp_types::Hover {
            contents: lsp_types::HoverContents::Scalar(lsp_types::MarkedString::String(
                "info".to_string(),
            )),
            range: None,
        };
        let result = convert_hover(hover, &source);
        assert_eq!(result.contents, "info");
        assert_eq!(result.range, None);
    }

    #[test]
    fn hover_with_range() {
        let source = Source::detached("content");
        let hover = lsp_types::Hover {
            contents: lsp_types::HoverContents::Scalar(lsp_types::MarkedString::String(
                "info".to_string(),
            )),
            range: Some(lsp_types::Range {
                start: lsp_types::Position::new(0, 0),
                end: lsp_types::Position::new(0, 5),
            }),
        };
        let result = convert_hover(hover, &source);
        assert!(result.range.is_some());
    }

    #[test]
    fn goto_definition_scalar() {
        let source = Source::detached("content");
        let resp = lsp_types::GotoDefinitionResponse::Scalar(lsp_types::Location {
            uri: lsp_types::Url::parse("file:///target.typ").unwrap(),
            range: lsp_types::Range {
                start: lsp_types::Position::new(1, 0),
                end: lsp_types::Position::new(1, 5),
            },
        });
        let result = convert_goto_definition_response(resp, &source).unwrap();
        assert_eq!(result.path, PathBuf::from("/target.typ"));
        assert_eq!(result.range.start.line, 1);
    }

    #[test]
    fn goto_definition_array() {
        let source = Source::detached("content");
        let resp = lsp_types::GotoDefinitionResponse::Array(vec![lsp_types::Location {
            uri: lsp_types::Url::parse("file:///first.typ").unwrap(),
            range: lsp_types::Range {
                start: lsp_types::Position::new(0, 0),
                end: lsp_types::Position::new(0, 0),
            },
        }]);
        let result = convert_goto_definition_response(resp, &source).unwrap();
        assert_eq!(result.path, PathBuf::from("/first.typ"));
    }

    #[test]
    fn goto_definition_link() {
        let source = Source::detached("content");
        let resp = lsp_types::GotoDefinitionResponse::Link(vec![lsp_types::LocationLink {
            target_uri: lsp_types::Url::parse("file:///linked.typ").unwrap(),
            target_range: lsp_types::Range {
                start: lsp_types::Position::new(2, 0),
                end: lsp_types::Position::new(2, 10),
            },
            target_selection_range: lsp_types::Range {
                start: lsp_types::Position::new(2, 0),
                end: lsp_types::Position::new(2, 10),
            },
            origin_selection_range: None,
        }]);
        let result = convert_goto_definition_response(resp, &source).unwrap();
        assert_eq!(result.path, PathBuf::from("/linked.typ"));
        assert_eq!(result.range.start.line, 2);
    }

    #[test]
    fn convert_document_symbol_basic() {
        let source = Source::detached("content");
        let symbol = lsp_types::DocumentSymbol {
            name: "MyFunc".to_string(),
            detail: Some("fn my_func()".to_string()),
            kind: lsp_types::SymbolKind::FUNCTION,
            range: lsp_types::Range {
                start: lsp_types::Position::new(0, 0),
                end: lsp_types::Position::new(0, 10),
            },
            selection_range: lsp_types::Range {
                start: lsp_types::Position::new(0, 0),
                end: lsp_types::Position::new(0, 6),
            },
            children: None,
            deprecated: None,
            tags: None,
        };
        let result = convert_document_symbol(symbol, &source).unwrap();
        assert_eq!(result.name, "MyFunc");
        assert_eq!(result.detail, Some("fn my_func()".to_string()));
        assert_eq!(result.kind, SymbolKind::Function);
        assert!(result.children.is_empty());
    }

    #[test]
    fn convert_document_symbol_with_children() {
        let source = Source::detached("content");
        let child = lsp_types::DocumentSymbol {
            name: "x".to_string(),
            detail: None,
            kind: lsp_types::SymbolKind::VARIABLE,
            range: lsp_types::Range {
                start: lsp_types::Position::new(1, 2),
                end: lsp_types::Position::new(1, 3),
            },
            selection_range: lsp_types::Range {
                start: lsp_types::Position::new(1, 2),
                end: lsp_types::Position::new(1, 3),
            },
            children: None,
            deprecated: None,
            tags: None,
        };
        let parent = lsp_types::DocumentSymbol {
            name: "Parent".to_string(),
            detail: None,
            kind: lsp_types::SymbolKind::NAMESPACE,
            range: lsp_types::Range {
                start: lsp_types::Position::new(0, 0),
                end: lsp_types::Position::new(5, 0),
            },
            selection_range: lsp_types::Range {
                start: lsp_types::Position::new(0, 0),
                end: lsp_types::Position::new(0, 6),
            },
            children: Some(vec![child]),
            deprecated: None,
            tags: None,
        };
        let result = convert_document_symbol(parent, &source).unwrap();
        assert_eq!(result.children.len(), 1);
        assert_eq!(result.children[0].name, "x");
    }

    #[test]
    fn convert_symbol_information_basic() {
        let source = Source::detached("content");
        let info = lsp_types::SymbolInformation {
            name: "SymbolName".to_string(),
            kind: lsp_types::SymbolKind::CONSTANT,
            container_name: Some("Container".to_string()),
            location: lsp_types::Location {
                uri: lsp_types::Url::parse("file:///test.typ").unwrap(),
                range: lsp_types::Range {
                    start: lsp_types::Position::new(0, 0),
                    end: lsp_types::Position::new(0, 8),
                },
            },
            deprecated: None,
            tags: None,
        };
        let result = convert_symbol_information(info, &source).unwrap();
        assert_eq!(result.name, "SymbolName");
        assert_eq!(result.detail, Some("Container".to_string()));
        assert_eq!(result.kind, SymbolKind::Constant);
    }

    #[test]
    fn signature_help_basic() {
        let sig = lsp_types::SignatureHelp {
            signatures: vec![lsp_types::SignatureInformation {
                label: "fn foo(x: int)".to_string(),
                documentation: Some(lsp_types::Documentation::String("does foo".to_string())),
                parameters: Some(vec![lsp_types::ParameterInformation {
                    label: lsp_types::ParameterLabel::Simple("x".to_string()),
                    documentation: Some(lsp_types::Documentation::String("the param".to_string())),
                }]),
                active_parameter: Some(0),
            }],
            active_signature: Some(0),
            active_parameter: None,
        };
        let result = convert_signature_help(sig);
        assert_eq!(result.label, "fn foo(x: int)");
        assert_eq!(result.documentation, Some("does foo".to_string()));
        assert_eq!(result.parameters.len(), 1);
        assert_eq!(result.active_parameter, Some(0));
    }

    #[test]
    fn signature_help_no_active_signature() {
        let sig = lsp_types::SignatureHelp {
            signatures: vec![lsp_types::SignatureInformation {
                label: "fn bar()".to_string(),
                documentation: None,
                parameters: None,
                active_parameter: None,
            }],
            active_signature: None,
            active_parameter: None,
        };
        let result = convert_signature_help(sig);
        assert_eq!(result.label, "fn bar()");
        assert_eq!(result.active_parameter, None);
        assert!(result.parameters.is_empty());
    }

    #[test]
    fn signature_help_markup_documentation() {
        let sig = lsp_types::SignatureHelp {
            signatures: vec![lsp_types::SignatureInformation {
                label: "fn baz()".to_string(),
                documentation: Some(lsp_types::Documentation::MarkupContent(
                    lsp_types::MarkupContent {
                        kind: lsp_types::MarkupKind::Markdown,
                        value: "**important**".to_string(),
                    },
                )),
                parameters: None,
                active_parameter: None,
            }],
            active_signature: None,
            active_parameter: None,
        };
        let result = convert_signature_help(sig);
        assert_eq!(result.documentation, Some("**important**".to_string()));
    }

    #[test]
    fn code_action_basic() {
        let action = lsp_types::CodeAction {
            title: "Fix it".to_string(),
            kind: Some(lsp_types::CodeActionKind::QUICKFIX),
            edit: None,
            ..Default::default()
        };
        let result = convert_code_action(action).unwrap();
        assert_eq!(result.title, "Fix it");
        assert!(result.edit.is_none());
    }

    #[test]
    fn code_action_with_text_edit() {
        let mut changes = HashMap::new();
        changes.insert(
            lsp_types::Url::parse("file:///test.typ").unwrap(),
            vec![lsp_types::TextEdit {
                range: lsp_types::Range {
                    start: lsp_types::Position::new(0, 0),
                    end: lsp_types::Position::new(0, 5),
                },
                new_text: "replaced".to_string(),
            }],
        );
        let action = lsp_types::CodeAction {
            title: "Replace".to_string(),
            kind: None,
            edit: Some(lsp_types::WorkspaceEdit {
                changes: Some(changes),
                ..Default::default()
            }),
            ..Default::default()
        };
        let result = convert_code_action(action).unwrap();
        let edit = result.edit.unwrap();
        assert_eq!(edit.new_text, "replaced");
    }

    #[test]
    fn extract_text_edits_from_changes() {
        let mut changes = HashMap::new();
        changes.insert(
            lsp_types::Url::parse("file:///test.typ").unwrap(),
            vec![lsp_types::TextEdit {
                range: lsp_types::Range {
                    start: lsp_types::Position::new(0, 0),
                    end: lsp_types::Position::new(0, 3),
                },
                new_text: "abc".to_string(),
            }],
        );
        let edit = lsp_types::WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
        };
        let result = extract_text_edits(edit);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].new_text, "abc");
    }

    #[test]
    fn extract_text_edits_from_document_changes() {
        let edit = lsp_types::WorkspaceEdit {
            changes: None,
            document_changes: Some(lsp_types::DocumentChanges::Edits(vec![
                lsp_types::TextDocumentEdit {
                    text_document: lsp_types::OptionalVersionedTextDocumentIdentifier {
                        uri: lsp_types::Url::parse("file:///test.typ").unwrap(),
                        version: None,
                    },
                    edits: vec![lsp_types::OneOf::Left(lsp_types::TextEdit {
                        range: lsp_types::Range {
                            start: lsp_types::Position::new(1, 0),
                            end: lsp_types::Position::new(1, 4),
                        },
                        new_text: "xyz".to_string(),
                    })],
                },
            ])),
            ..Default::default()
        };
        let result = extract_text_edits(edit);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].new_text, "xyz");
    }

    #[test]
    fn extract_one_of_text_edit_left() {
        let edit = lsp_types::OneOf::Left(lsp_types::TextEdit {
            range: lsp_types::Range {
                start: lsp_types::Position::new(0, 0),
                end: lsp_types::Position::new(0, 3),
            },
            new_text: "new".to_string(),
        });
        let (range, text) = extract_one_of_text_edit(edit);
        assert_eq!(text, "new");
        assert_eq!(range.start.line, 0);
    }

    #[test]
    fn extract_one_of_text_edit_right() {
        let edit = lsp_types::OneOf::Right(lsp_types::AnnotatedTextEdit {
            text_edit: lsp_types::TextEdit {
                range: lsp_types::Range {
                    start: lsp_types::Position::new(2, 0),
                    end: lsp_types::Position::new(2, 5),
                },
                new_text: "annotated".to_string(),
            },
            annotation_id: "test".to_string(),
        });
        let (range, text) = extract_one_of_text_edit(edit);
        assert_eq!(text, "annotated");
        assert_eq!(range.start.line, 2);
    }

    #[test]
    fn typst_lsp_language_id() {
        let lsp = TypstLsp::new(None, vec![]).unwrap();
        assert_eq!(lsp.language_id(), "typst");
    }

    #[test]
    fn typst_lsp_update_and_close() {
        let mut lsp = TypstLsp::new(None, vec![]).unwrap();
        let path = Path::new("test.typ");
        lsp.update(path, "#set page()");
        lsp.close(path);
        assert!(lsp.files.is_empty());
    }
}
