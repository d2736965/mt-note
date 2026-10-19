use oxc_allocator::Allocator;
use oxc_ast::ast::{
    BindingPattern, Declaration, ExportDefaultDeclarationKind, Expression, FormalParameter,
    Program, Statement, TSEnumDeclaration, TSEnumMemberName, TSFunctionType, TSGlobalDeclaration,
    TSInterfaceDeclaration, TSMethodSignature, TSModuleDeclaration, TSModuleDeclarationBody,
    TSModuleDeclarationName, TSSignature, TSTupleElement, TSType, TSTypeName,
};
use oxc_parser::Parser;
use oxc_span::SourceType;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::error::Error;
use crate::types::*;

/// 从源码文本中提取 JSDoc 描述文本与声明名称的映射。
///
/// 提取 `/** ... */` 注释中的描述文本（去除 `*` 前缀和 `@` 标签行），
/// 关联到紧随其后的声明名称。若注释仅含 `@` 标签而无描述，不生成映射。
fn extract_doc_comment_map(source: &str) -> HashMap<String, String> {
    static TOP_LEVEL_RE: std::sync::OnceLock<Option<regex::Regex>> = std::sync::OnceLock::new();
    static METHOD_RE: std::sync::OnceLock<Option<regex::Regex>> = std::sync::OnceLock::new();

    let top_level_re = TOP_LEVEL_RE.get_or_init(|| {
        regex::Regex::new(
            r"(?m)/\*\*((?:[^*]|\*[^/])*)\*/\s*(?:export\s+)?(?:default\s+)?(?:function|interface|type|enum|const|let|var|class)\s+(\w+)"
        ).ok()
    });
    let method_re = METHOD_RE
        .get_or_init(|| regex::Regex::new(r"(?m)/\*\*((?:[^*]|\*[^/])*)\*/\s*(\w+)[\(:]").ok());

    let mut map = HashMap::new();

    let mut insert_if_valid = |name: &str, raw: &str| {
        let cleaned = clean_doc_comment(raw);
        if let Some(doc) = cleaned {
            map.insert(name.to_string(), doc);
        }
    };

    if let Some(re) = top_level_re {
        for cap in re.captures_iter(source) {
            if let (Some(raw), Some(name)) = (cap.get(1), cap.get(2)) {
                insert_if_valid(name.as_str(), raw.as_str());
            }
        }
    }

    if let Some(re) = method_re {
        for cap in re.captures_iter(source) {
            if let (Some(raw), Some(name)) = (cap.get(1), cap.get(2)) {
                insert_if_valid(name.as_str(), raw.as_str());
            }
        }
    }

    map
}

/// 清洗 JSDoc 原始内容为纯描述文本：
/// - 去除每行开头的 `*` 和空白
/// - 过滤 `@` 开头的标签行（如 `@rust-lint`、`@param`）
/// - 从非 `@` 开头的行内移除 `@` 标签及其参数（如 `@rust-lint expect(dead_code)`）
/// - 返回 `None` 如果清洗后为空
fn clean_doc_comment(raw: &str) -> Option<String> {
    static AT_TAG_RE: std::sync::OnceLock<Option<regex::Regex>> = std::sync::OnceLock::new();
    let at_tag_re = AT_TAG_RE.get_or_init(|| regex::Regex::new(r"@\S+(\s+\S+)*\s*").ok());

    let cleaned: String = raw
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            let line = trimmed.strip_prefix('*').unwrap_or(trimmed).trim();
            if line.starts_with('@') {
                String::new()
            } else if let Some(re) = at_tag_re {
                re.replace_all(line, "").trim().to_string()
            } else {
                line.to_string()
            }
        })
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

/// 从源码中提取 `@rust-lint` 注解，返回 name → lint 属性列表的映射
///
/// 支持两种模式：
/// 1. 顶层声明：`/** @rust-lint expect(dead_code) */ export interface Foo`
/// 2. 成员声明：`/** @rust-lint expect(dead_code) */ method(): void`
///
/// 一行可包含多个 `@rust-lint`，如：
/// ```ts
/// /**
///  * @rust-lint expect(dead_code, reason = "a")
///  * @rust-lint allow(clippy::too_many_arguments)
///  */
/// ```
fn extract_rust_lint_attrs_map(source: &str) -> HashMap<String, Vec<RustLintAttr>> {
    static RE: std::sync::OnceLock<Option<regex::Regex>> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(
            r"(?m)/\*\*((?:[^*]|\*[^/])*)\*/\s*\n?\s*(?:export\s+(?:function|interface|type|enum)\s+)?(\w+)\s*[\(:\{]",
        )
        .ok()
    });

    static LINT_RE: std::sync::OnceLock<Option<regex::Regex>> = std::sync::OnceLock::new();
    let lint_re = LINT_RE
        .get_or_init(|| regex::Regex::new(r"@rust-lint\s+([^@*]+?)(?:\s*$|\s*\n|\s*\*)").ok());

    let mut map: HashMap<String, Vec<RustLintAttr>> = HashMap::new();

    if let Some(re) = re {
        for cap in re.captures_iter(source) {
            let body = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let name = cap
                .get(2)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();

            let attrs: Vec<RustLintAttr> = if let Some(lint_re) = lint_re {
                lint_re
                    .captures_iter(body)
                    .filter_map(|c| {
                        c.get(1).map(|m| RustLintAttr {
                            content: m.as_str().trim().to_string(),
                        })
                    })
                    .collect()
            } else {
                Vec::new()
            };

            if !attrs.is_empty() {
                map.insert(name, attrs);
            }
        }
    }

    map
}

/// 从内存字符串解析 TS 源码，不依赖文件系统。
///
/// `file_path` 仅用于推断 `SourceType`（.ts / .tsx），
/// 文件不需要实际存在。
pub fn parse_from_str(source_text: &str, file_path: &Path) -> Result<ParsedModule, Error> {
    let doc_map = extract_doc_comment_map(source_text);
    let rust_lint_map = extract_rust_lint_attrs_map(source_text);

    let allocator = Allocator::default();
    let source_type = SourceType::from_path(file_path)
        .unwrap_or_default()
        .with_typescript(true);
    let result = Parser::new(&allocator, source_text, source_type).parse();

    if !result.diagnostics.is_empty() {
        let errors: Vec<String> = result.diagnostics.iter().map(|e| format!("{e}")).collect();
        return Err(Error::ParseFailed(errors.join("\n")));
    }

    let program = result.program;
    Ok(extract_module(
        &program,
        &doc_map,
        &rust_lint_map,
        source_text,
    ))
}

/// 从文件系统读取 TS 源码并解析，递归处理 import 引用的子模块。
pub fn parse(config: &Config) -> Result<ParsedModule, Error> {
    let mut visited: HashSet<PathBuf> = HashSet::new();
    let mut merged = ParsedModule {
        factory_functions: Vec::new(),
        interfaces: Vec::new(),
        enums: Vec::new(),
        type_aliases: Vec::new(),
        global_decl: None,
    };
    parse_recursive(&config.entry, &mut visited, &mut merged)?;
    Ok(merged)
}

fn parse_recursive(
    entry: &Path,
    visited: &mut HashSet<PathBuf>,
    merged: &mut ParsedModule,
) -> Result<(), Error> {
    let canonical = entry.canonicalize().map_err(|e| Error::Io {
        path: entry.to_path_buf(),
        source: e,
    })?;
    if !visited.insert(canonical.clone()) {
        return Ok(());
    }

    let source_text = std::fs::read_to_string(entry).map_err(|e| Error::Io {
        path: entry.to_path_buf(),
        source: e,
    })?;

    let doc_map = extract_doc_comment_map(&source_text);
    let rust_lint_map = extract_rust_lint_attrs_map(&source_text);

    let allocator = Allocator::default();
    let source_type = SourceType::from_path(entry)
        .unwrap_or_default()
        .with_typescript(true);
    let result = Parser::new(&allocator, &source_text, source_type).parse();

    if !result.diagnostics.is_empty() {
        let errors: Vec<String> = result.diagnostics.iter().map(|e| format!("{e}")).collect();
        return Err(Error::ParseFailed(errors.join("\n")));
    }

    let program = result.program;
    let module = extract_module(&program, &doc_map, &rust_lint_map, &source_text);

    let import_paths = collect_relative_imports(&program, entry);

    merged.factory_functions.extend(module.factory_functions);
    merged.interfaces.extend(module.interfaces);
    merged.enums.extend(module.enums);
    merged.type_aliases.extend(module.type_aliases);
    if let Some(gd) = module.global_decl {
        if let Some(ref mut merged_gd) = merged.global_decl {
            merged_gd.factory_methods.extend(gd.factory_methods);
            merged_gd.instance_vars.extend(gd.instance_vars);
        } else {
            merged.global_decl = Some(gd);
        }
    }

    for import_path in import_paths {
        parse_recursive(&import_path, visited, merged)?;
    }

    Ok(())
}

fn collect_relative_imports(program: &Program, from_file: &Path) -> Vec<PathBuf> {
    let base_dir = from_file.parent().unwrap_or(Path::new("."));
    collect_relative_import_specs(program)
        .into_iter()
        .filter_map(|spec| resolve_import_path_from_spec(&spec, base_dir))
        .collect()
}

fn collect_relative_import_specs(program: &Program) -> Vec<String> {
    let mut specs = Vec::new();

    for stmt in &program.body {
        if let Statement::ImportDeclaration(import) = stmt {
            let spec = import.source.value.as_str();
            if spec.starts_with('.') {
                specs.push(spec.to_string());
            }
        } else if let Statement::ExportNamedDeclaration(export) = stmt {
            if let Some(source) = &export.source {
                let spec = source.value.as_str();
                if spec.starts_with('.') {
                    specs.push(spec.to_string());
                }
            }
        } else if let Statement::ExportAllDeclaration(export_all) = stmt {
            let spec = export_all.source.value.as_str();
            if spec.starts_with('.') {
                specs.push(spec.to_string());
            }
        }
    }

    specs
}

fn resolve_import_path_from_spec(spec: &str, base_dir: &Path) -> Option<PathBuf> {
    if !spec.starts_with('.') {
        return None;
    }
    let candidate = base_dir.join(spec);

    if candidate.is_file() {
        return Some(candidate);
    }

    for ext in ["ts", "tsx", "js", "jsx"] {
        let with_ext = candidate.with_extension(ext);
        if with_ext.is_file() {
            return Some(with_ext);
        }
    }

    if candidate.is_dir() {
        for index_name in ["index.ts", "index.tsx", "index.js", "index.jsx"] {
            let index = candidate.join(index_name);
            if index.is_file() {
                return Some(index);
            }
        }
    }

    None
}

fn extract_module(
    program: &Program,
    doc_map: &HashMap<String, String>,
    rust_lint_map: &HashMap<String, Vec<RustLintAttr>>,
    source_text: &str,
) -> ParsedModule {
    let mut extractor = ModuleExtractor {
        factory_functions: Vec::new(),
        interfaces: Vec::new(),
        enums: Vec::new(),
        type_aliases: Vec::new(),
        global_decl: None,
        doc_map,
        rust_lint_map,
        source_text,
    };

    for stmt in &program.body {
        extractor.visit_statement(stmt);
    }

    ParsedModule {
        factory_functions: extractor.factory_functions,
        interfaces: extractor.interfaces,
        enums: extractor.enums,
        type_aliases: extractor.type_aliases,
        global_decl: extractor.global_decl,
    }
}

struct ModuleExtractor<'a> {
    factory_functions: Vec<FactoryFunction>,
    interfaces: Vec<InterfaceInfo>,
    enums: Vec<EnumInfo>,
    type_aliases: Vec<TypeAliasInfo>,
    global_decl: Option<GlobalDeclaration>,
    doc_map: &'a HashMap<String, String>,
    rust_lint_map: &'a HashMap<String, Vec<RustLintAttr>>,
    source_text: &'a str,
}

impl<'a> ModuleExtractor<'a> {
    fn visit_statement(&mut self, stmt: &Statement) {
        if let Statement::ExportNamedDeclaration(export) = stmt {
            if let Some(decl) = &export.declaration {
                self.visit_declaration(decl);
            }
        } else if let Statement::ExportDefaultDeclaration(export) = stmt {
            if let ExportDefaultDeclarationKind::FunctionDeclaration(func) = &export.declaration {
                if let Some(id) = &func.id {
                    let name = id.name.to_string();
                    self.factory_functions.push(FactoryFunction {
                        doc_comment: self.doc_map.get(&name).cloned(),
                        rust_lint_attrs: self.rust_lint_map.get(&name).cloned().unwrap_or_default(),
                        name,
                        params: extract_function_params(func),
                        return_type: extract_return_type(func),
                        is_default_export: true,
                    });
                }
            } else if let ExportDefaultDeclarationKind::TSInterfaceDeclaration(iface) =
                &export.declaration
            {
                self.interfaces.push(extract_interface(
                    iface,
                    self.doc_map,
                    self.rust_lint_map,
                    self.source_text,
                ));
            }
        } else if let Statement::TSModuleDeclaration(mod_decl) = stmt {
            self.visit_ts_module_declaration(mod_decl);
        } else if let Statement::TSGlobalDeclaration(global_decl) = stmt {
            self.visit_ts_global_declaration(global_decl);
        }
    }

    fn visit_ts_global_declaration(&mut self, global_decl: &TSGlobalDeclaration) {
        let mut factory_methods: Vec<FactoryMethodDecl> = Vec::new();
        let mut instance_vars: Vec<InstanceVarDecl> = Vec::new();

        for stmt in &global_decl.body.body {
            if let Statement::TSInterfaceDeclaration(iface) = stmt
                && iface.id.name == "Window"
            {
                extract_window_extensions(
                    iface,
                    &mut factory_methods,
                    &mut instance_vars,
                    self.doc_map,
                    self.rust_lint_map,
                );
            }
        }

        self.global_decl = Some(GlobalDeclaration {
            factory_methods,
            instance_vars,
        });
    }

    fn visit_declaration(&mut self, decl: &Declaration) {
        if let Declaration::FunctionDeclaration(func) = decl {
            if let Some(id) = &func.id {
                let name = id.name.to_string();
                self.factory_functions.push(FactoryFunction {
                    doc_comment: self.doc_map.get(&name).cloned(),
                    rust_lint_attrs: self.rust_lint_map.get(&name).cloned().unwrap_or_default(),
                    name,
                    params: extract_function_params(func),
                    return_type: extract_return_type(func),
                    is_default_export: false,
                });
            }
        } else if let Declaration::TSInterfaceDeclaration(iface) = decl {
            self.interfaces.push(extract_interface(
                iface,
                self.doc_map,
                self.rust_lint_map,
                self.source_text,
            ));
        } else if let Declaration::TSEnumDeclaration(enum_decl) = decl {
            self.enums.push(extract_enum(enum_decl));
        } else if let Declaration::TSTypeAliasDeclaration(type_alias) = decl {
            self.type_aliases.push(TypeAliasInfo {
                name: type_alias.id.name.to_string(),
            });
        } else if let Declaration::TSModuleDeclaration(mod_decl) = decl {
            self.visit_ts_module_declaration(mod_decl);
        }
    }

    fn visit_ts_module_declaration(&mut self, mod_decl: &TSModuleDeclaration) {
        let is_global = match &mod_decl.id {
            TSModuleDeclarationName::Identifier(id) => id.name == "global",
            TSModuleDeclarationName::StringLiteral(lit) => lit.value == "global",
        };

        if is_global
            && mod_decl.declare
            && let Some(TSModuleDeclarationBody::TSModuleBlock(block)) = &mod_decl.body
        {
            let mut factory_methods: Vec<FactoryMethodDecl> = Vec::new();
            let mut instance_vars: Vec<InstanceVarDecl> = Vec::new();

            for stmt in &block.body {
                if let Statement::TSInterfaceDeclaration(iface) = stmt
                    && iface.id.name == "Window"
                {
                    extract_window_extensions(
                        iface,
                        &mut factory_methods,
                        &mut instance_vars,
                        self.doc_map,
                        self.rust_lint_map,
                    );
                }
            }

            self.global_decl = Some(GlobalDeclaration {
                factory_methods,
                instance_vars,
            });
        }
    }
}

fn extract_window_extensions(
    iface: &TSInterfaceDeclaration,
    factory_methods: &mut Vec<FactoryMethodDecl>,
    instance_vars: &mut Vec<InstanceVarDecl>,
    doc_map: &HashMap<String, String>,
    rust_lint_map: &HashMap<String, Vec<RustLintAttr>>,
) {
    for sig in &iface.body.body {
        if let TSSignature::TSPropertySignature(prop) = sig {
            let key_name = property_key_name(&prop.key);
            if let Some(type_ann) = &prop.type_annotation {
                let ts_type = &type_ann.type_annotation;
                if is_object_type_with_methods(ts_type) {
                    factory_methods.push(FactoryMethodDecl {
                        name: key_name,
                        methods: extract_object_methods(ts_type, doc_map, rust_lint_map),
                    });
                } else {
                    instance_vars.push(InstanceVarDecl {
                        name: key_name,
                        type_name: extract_type_name(ts_type),
                        optional: prop.optional,
                    });
                }
            }
        } else if let TSSignature::TSMethodSignature(method) = sig {
            let key_name = property_key_name(&method.key);
            factory_methods.push(FactoryMethodDecl {
                name: key_name,
                methods: vec![extract_method_sig(method, doc_map, rust_lint_map)],
            });
        }
    }
}

fn is_object_type_with_methods(ts_type: &TSType) -> bool {
    matches!(ts_type, TSType::TSTypeLiteral(_))
}

fn extract_object_methods(
    ts_type: &TSType,
    doc_map: &HashMap<String, String>,
    rust_lint_map: &HashMap<String, Vec<RustLintAttr>>,
) -> Vec<MethodSignature> {
    let TSType::TSTypeLiteral(lit) = ts_type else {
        return Vec::new();
    };
    let mut methods = Vec::new();
    for member in &lit.members {
        if let TSSignature::TSMethodSignature(method) = member {
            methods.push(extract_method_sig(method, doc_map, rust_lint_map));
        } else if let TSSignature::TSPropertySignature(prop) = member
            && let Some(type_ann) = &prop.type_annotation
        {
            let inner = &type_ann.type_annotation;
            if is_function_type(inner) {
                let name = property_key_name(&prop.key);
                methods.push(MethodSignature {
                    doc_comment: doc_map.get(&name).cloned(),
                    name: name.clone(),
                    params: vec![],
                    return_type: extract_ts_return_type(inner),
                    rust_lint_attrs: rust_lint_map.get(&name).cloned().unwrap_or_default(),
                    path: Vec::new(),
                });
            }
        }
    }
    methods
}

fn is_function_type(ts_type: &TSType) -> bool {
    matches!(ts_type, TSType::TSFunctionType(_))
}

/// 从对象字面量类型中递归提取方法，并附带嵌套路径前缀。
///
/// 用于 interface 内「对象字面量属性」（如 `render: { setScale(): void }`）：
/// 将内层方法提取为带 `path` 的 `MethodSignature`，使 codegen 生成
/// `instance.render.setScale()` 形式的嵌套访问。深层嵌套（如
/// `render.query.pageInfos`）通过递归追加路径段支持。
fn extract_object_methods_with_path(
    ts_type: &TSType,
    doc_map: &HashMap<String, String>,
    rust_lint_map: &HashMap<String, Vec<RustLintAttr>>,
    source_text: &str,
    path_prefix: &[String],
) -> Vec<MethodSignature> {
    let TSType::TSTypeLiteral(lit) = ts_type else {
        return Vec::new();
    };
    let mut methods = Vec::new();
    for member in &lit.members {
        let span_start = match member {
            TSSignature::TSMethodSignature(m) => m.span.start,
            TSSignature::TSPropertySignature(p) => p.span.start,
            _ => 0,
        };
        let local_rust_lint =
            extract_rust_lint_from_source_before(source_text, span_start as usize);

        match member {
            TSSignature::TSMethodSignature(method) => {
                let mut sig = extract_method_sig(method, doc_map, rust_lint_map);
                sig.rust_lint_attrs = local_rust_lint;
                sig.path = path_prefix.to_vec();
                methods.push(sig);
            }
            TSSignature::TSPropertySignature(prop) => {
                let Some(type_ann) = &prop.type_annotation else {
                    continue;
                };
                let inner = &type_ann.type_annotation;
                let key = property_key_name(&prop.key);
                if is_function_type(inner) {
                    let mut sig = extract_property_as_method(&key, inner, doc_map, rust_lint_map);
                    sig.rust_lint_attrs = local_rust_lint;
                    sig.path = path_prefix.to_vec();
                    methods.push(sig);
                } else if is_object_type_with_methods(inner) {
                    let mut deeper = path_prefix.to_vec();
                    deeper.push(key);
                    methods.extend(extract_object_methods_with_path(
                        inner,
                        doc_map,
                        rust_lint_map,
                        source_text,
                        &deeper,
                    ));
                }
            }
            _ => continue,
        }
    }
    methods
}

fn extract_property_as_method(
    name: &str,
    ts_type: &TSType,
    doc_map: &HashMap<String, String>,
    rust_lint_map: &HashMap<String, Vec<RustLintAttr>>,
) -> MethodSignature {
    let TSType::TSFunctionType(func) = ts_type else {
        return MethodSignature {
            doc_comment: doc_map.get(name).cloned(),
            name: name.to_string(),
            params: vec![],
            return_type: RustType::Unknown,
            rust_lint_attrs: rust_lint_map.get(name).cloned().unwrap_or_default(),
            path: Vec::new(),
        };
    };

    let params = extract_func_type_params(func);
    let return_type = ts_type_to_rust_type(&func.return_type.type_annotation);

    MethodSignature {
        doc_comment: doc_map.get(name).cloned(),
        name: name.to_string(),
        params,
        return_type,
        rust_lint_attrs: rust_lint_map.get(name).cloned().unwrap_or_default(),
        path: Vec::new(),
    }
}

fn extract_func_type_params(func: &TSFunctionType) -> Vec<ParamInfo> {
    let mut params = Vec::new();
    for param in &func.params.items {
        if let Some(name) = param_name(param) {
            let ts_type = param
                .type_annotation
                .as_ref()
                .map(|ann| ts_type_to_rust_type(&ann.type_annotation))
                .unwrap_or(RustType::Unknown);
            params.push(ParamInfo {
                name,
                ts_type,
                optional: param.optional,
            });
        }
    }
    params
}

fn extract_method_sig(
    method: &TSMethodSignature,
    doc_map: &HashMap<String, String>,
    rust_lint_map: &HashMap<String, Vec<RustLintAttr>>,
) -> MethodSignature {
    let name = property_key_name(&method.key);
    let params = extract_method_params(method);
    let return_type = method
        .return_type
        .as_ref()
        .map(|ann| ts_type_to_rust_type(&ann.type_annotation))
        .unwrap_or(RustType::Void);
    MethodSignature {
        doc_comment: doc_map.get(&name).cloned(),
        rust_lint_attrs: rust_lint_map.get(&name).cloned().unwrap_or_default(),
        name,
        params,
        return_type,
        path: Vec::new(),
    }
}

fn extract_method_params(method: &TSMethodSignature) -> Vec<ParamInfo> {
    let mut params = Vec::new();
    for param in &method.params.items {
        if let Some(name) = param_name(param) {
            let ts_type = param
                .type_annotation
                .as_ref()
                .map(|ann| ts_type_to_rust_type(&ann.type_annotation))
                .unwrap_or(RustType::Unknown);
            params.push(ParamInfo {
                name,
                ts_type,
                optional: param.optional,
            });
        }
    }
    params
}

fn extract_function_params(func: &oxc_ast::ast::Function) -> Vec<ParamInfo> {
    let mut params = Vec::new();
    for param in &func.params.items {
        if let Some(name) = param_name(param) {
            let ts_type = param
                .type_annotation
                .as_ref()
                .map(|ann| ts_type_to_rust_type(&ann.type_annotation))
                .unwrap_or(RustType::Unknown);
            params.push(ParamInfo {
                name,
                ts_type,
                optional: param.optional,
            });
        }
    }
    params
}

fn param_name(param: &FormalParameter) -> Option<String> {
    match &param.pattern {
        BindingPattern::BindingIdentifier(id) => Some(id.name.to_string()),
        BindingPattern::ObjectPattern(obj) => {
            if let Some(first_prop) = obj.properties.first() {
                match &first_prop.value {
                    BindingPattern::BindingIdentifier(id) => Some(id.name.to_string()),
                    _ => None,
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

fn extract_return_type(func: &oxc_ast::ast::Function) -> RustType {
    func.return_type
        .as_ref()
        .map(|ann| ts_type_to_rust_type(&ann.type_annotation))
        .unwrap_or(RustType::Void)
}

fn extract_rust_lint_from_source_before(source_text: &str, position: usize) -> Vec<RustLintAttr> {
    static JS_DOC_RE: std::sync::OnceLock<Option<regex::Regex>> = std::sync::OnceLock::new();
    static LINT_RE: std::sync::OnceLock<Option<regex::Regex>> = std::sync::OnceLock::new();

    let js_doc_re = JS_DOC_RE
        .get_or_init(|| regex::Regex::new(r"/\*\*((?:[^*]|\*[^/])*)\*/\s*(?:export\s+)?$").ok());
    let lint_re = LINT_RE
        .get_or_init(|| regex::Regex::new(r"@rust-lint\s+([^@*]+?)(?:\s*$|\s*\n|\s*\*)").ok());

    let before = &source_text[..position.min(source_text.len())];
    if let Some(re) = js_doc_re
        && let Some(cap) = re.captures(before)
    {
        let body = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        if let Some(lint_re) = lint_re {
            return lint_re
                .captures_iter(body)
                .filter_map(|c| {
                    c.get(1).map(|m| RustLintAttr {
                        content: m.as_str().trim().to_string(),
                    })
                })
                .collect();
        }
    }
    Vec::new()
}

fn extract_interface(
    iface: &TSInterfaceDeclaration,
    doc_map: &HashMap<String, String>,
    rust_lint_map: &HashMap<String, Vec<RustLintAttr>>,
    source_text: &str,
) -> InterfaceInfo {
    let iface_name = iface.id.name.to_string();
    let mut methods = Vec::new();
    let mut properties = Vec::new();

    for sig in &iface.body.body {
        let span_start = match sig {
            TSSignature::TSMethodSignature(m) => m.span.start,
            TSSignature::TSPropertySignature(p) => p.span.start,
            TSSignature::TSCallSignatureDeclaration(c) => c.span.start,
            _ => 0,
        };
        let local_rust_lint =
            extract_rust_lint_from_source_before(source_text, span_start as usize);

        if let TSSignature::TSMethodSignature(method) = sig {
            let mut sig = extract_method_sig(method, doc_map, rust_lint_map);
            sig.rust_lint_attrs = local_rust_lint;
            methods.push(sig);
        } else if let TSSignature::TSPropertySignature(prop) = sig {
            let name = property_key_name(&prop.key);
            let ts_type = prop
                .type_annotation
                .as_ref()
                .map(|ann| ts_type_to_rust_type(&ann.type_annotation))
                .unwrap_or(RustType::Unknown);

            if let Some(ann) = &prop.type_annotation
                && is_function_type(&ann.type_annotation)
            {
                let mut sig =
                    extract_property_as_method(&name, &ann.type_annotation, doc_map, rust_lint_map);
                sig.rust_lint_attrs = local_rust_lint;
                methods.push(sig);
                continue;
            }

            if let Some(ann) = &prop.type_annotation
                && is_object_type_with_methods(&ann.type_annotation)
            {
                let nested = extract_object_methods_with_path(
                    &ann.type_annotation,
                    doc_map,
                    rust_lint_map,
                    source_text,
                    std::slice::from_ref(&name),
                );
                if !nested.is_empty() {
                    methods.extend(nested);
                    continue;
                }
            }

            properties.push(InterfaceProperty {
                name: name.clone(),
                ts_type,
                optional: prop.optional,
                readonly: prop.readonly,
                doc_comment: doc_map.get(&name).cloned(),
            });
        } else if let TSSignature::TSCallSignatureDeclaration(call) = sig {
            let mut call_params = Vec::new();
            for param in &call.params.items {
                if let Some(name) = param_name(param) {
                    let ts_type = param
                        .type_annotation
                        .as_ref()
                        .map(|ann| ts_type_to_rust_type(&ann.type_annotation))
                        .unwrap_or(RustType::Unknown);
                    call_params.push(ParamInfo {
                        name,
                        ts_type,
                        optional: param.optional,
                    });
                }
            }
            let return_type = call
                .return_type
                .as_ref()
                .map(|ann| ts_type_to_rust_type(&ann.type_annotation))
                .unwrap_or(RustType::Void);
            methods.push(MethodSignature {
                name: "(call)".to_string(),
                params: call_params,
                return_type,
                doc_comment: None,
                rust_lint_attrs: Vec::new(),
                path: Vec::new(),
            });
        }
    }

    InterfaceInfo {
        doc_comment: doc_map.get(&iface_name).cloned(),
        rust_lint_attrs: extract_rust_lint_from_source_before(
            source_text,
            iface.span.start as usize,
        ),
        name: iface_name,
        methods,
        properties,
    }
}

fn extract_enum(enum_decl: &TSEnumDeclaration) -> EnumInfo {
    let members = enum_decl
        .body
        .members
        .iter()
        .map(|member| {
            let name = enum_member_name(&member.id);
            match &member.initializer {
                Some(expr) => EnumMember {
                    name,
                    value: Some(expr_to_string(expr)),
                },
                None => EnumMember { name, value: None },
            }
        })
        .collect();

    EnumInfo {
        name: enum_decl.id.name.to_string(),
        members,
    }
}

fn enum_member_name(name: &TSEnumMemberName) -> String {
    match name {
        TSEnumMemberName::Identifier(id) => id.name.to_string(),
        TSEnumMemberName::String(lit) => lit.value.to_string(),
        TSEnumMemberName::ComputedString(lit) => lit.value.to_string(),
        TSEnumMemberName::ComputedTemplateString(_) => String::new(),
    }
}

fn expr_to_string(expr: &Expression) -> String {
    match expr {
        Expression::StringLiteral(lit) => format!("\"{}\"", lit.value),
        Expression::NumericLiteral(lit) => lit.value.to_string(),
        Expression::BooleanLiteral(lit) => lit.value.to_string(),
        _ => "...".to_string(),
    }
}

fn property_key_name(key: &oxc_ast::ast::PropertyKey) -> String {
    match key {
        oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name.to_string(),
        oxc_ast::ast::PropertyKey::PrivateIdentifier(id) => id.name.to_string(),
        oxc_ast::ast::PropertyKey::StringLiteral(lit) => lit.value.to_string(),
        _ => String::new(),
    }
}

fn extract_type_name(ts_type: &TSType) -> String {
    match ts_type {
        TSType::TSTypeReference(ref_type) => type_name_to_string(&ref_type.type_name),
        _ => ts_type_to_rust_type(ts_type).to_rust_type_string(),
    }
}

fn type_name_to_string(name: &TSTypeName) -> String {
    match name {
        TSTypeName::IdentifierReference(id) => id.name.to_string(),
        TSTypeName::QualifiedName(qualified) => {
            format!(
                "{}.{}",
                type_name_to_string(&qualified.left),
                qualified.right.name
            )
        }
        TSTypeName::ThisExpression(_) => "this".to_string(),
    }
}

fn extract_ts_return_type(ts_type: &TSType) -> RustType {
    ts_type_to_rust_type(ts_type)
}

fn ts_type_to_rust_type(ts_type: &TSType) -> RustType {
    match ts_type {
        TSType::TSVoidKeyword(_) => RustType::Void,
        TSType::TSStringKeyword(_) => RustType::String,
        TSType::TSNumberKeyword(_) => RustType::F64,
        TSType::TSBooleanKeyword(_) => RustType::Bool,
        TSType::TSNullKeyword(_) => RustType::Null,
        TSType::TSUndefinedKeyword(_) => RustType::Undefined,
        TSType::TSNeverKeyword(_) => RustType::Never,
        TSType::TSAnyKeyword(_) => RustType::Unknown,
        TSType::TSUnknownKeyword(_) => RustType::Unknown,
        TSType::TSObjectKeyword(_) => RustType::Unknown,
        TSType::TSTypeReference(ref_type) => {
            let name = type_name_to_string(&ref_type.type_name);
            if name == "Promise" {
                if let Some(type_args) = &ref_type.type_arguments
                    && let Some(first) = type_args.params.first()
                {
                    let inner = ts_type_to_rust_type(first);
                    return RustType::Promise(Box::new(inner));
                }
                RustType::Promise(Box::new(RustType::Unknown))
            } else if name == "Array" {
                if let Some(type_args) = &ref_type.type_arguments
                    && let Some(first) = type_args.params.first()
                {
                    let inner = ts_type_to_rust_type(first);
                    return RustType::Vec(Box::new(inner));
                }
                RustType::Vec(Box::new(RustType::Unknown))
            } else {
                RustType::Reference(name)
            }
        }
        TSType::TSUnionType(union) => {
            let variants: Vec<RustType> = union.types.iter().map(ts_type_to_rust_type).collect();
            RustType::Union(variants)
        }
        TSType::TSLiteralType(lit) => match &lit.literal {
            oxc_ast::ast::TSLiteral::StringLiteral(s) => {
                RustType::StringLiteral(s.value.to_string())
            }
            oxc_ast::ast::TSLiteral::NumericLiteral(n) => RustType::NumberLiteral(n.value),
            oxc_ast::ast::TSLiteral::BooleanLiteral(b) => RustType::BoolLiteral(b.value),
            _ => RustType::Unknown,
        },
        TSType::TSTupleType(tuple) => {
            let elements: Vec<RustType> = tuple
                .element_types
                .iter()
                .map(|e| ts_tuple_element_to_rust_type(e))
                .collect();
            RustType::Tuple(elements)
        }
        TSType::TSNamedTupleMember(named) => ts_tuple_element_to_rust_type(&named.element_type),
        TSType::TSArrayType(arr) => {
            let inner = ts_type_to_rust_type(&arr.element_type);
            RustType::Vec(Box::new(inner))
        }
        TSType::TSParenthesizedType(paren) => ts_type_to_rust_type(&paren.type_annotation),
        _ => RustType::Unknown,
    }
}

fn ts_tuple_element_to_rust_type(element: &TSTupleElement) -> RustType {
    match element {
        TSTupleElement::TSOptionalType(opt) => ts_type_to_rust_type(&opt.type_annotation),
        TSTupleElement::TSRestType(rest) => ts_type_to_rust_type(&rest.type_annotation),
        TSTupleElement::TSNamedTupleMember(named) => {
            ts_tuple_element_to_rust_type(&named.element_type)
        }
        TSTupleElement::TSVoidKeyword(_) => RustType::Void,
        TSTupleElement::TSStringKeyword(_) => RustType::String,
        TSTupleElement::TSNumberKeyword(_) => RustType::F64,
        TSTupleElement::TSBooleanKeyword(_) => RustType::Bool,
        TSTupleElement::TSNullKeyword(_) => RustType::Null,
        TSTupleElement::TSUndefinedKeyword(_) => RustType::Undefined,
        TSTupleElement::TSNeverKeyword(_) => RustType::Never,
        TSTupleElement::TSAnyKeyword(_) => RustType::Unknown,
        TSTupleElement::TSUnknownKeyword(_) => RustType::Unknown,
        TSTupleElement::TSObjectKeyword(_) => RustType::Unknown,
        TSTupleElement::TSTypeReference(ref_type) => {
            let name = type_name_to_string(&ref_type.type_name);
            if name == "Promise" {
                if let Some(type_args) = &ref_type.type_arguments
                    && let Some(first) = type_args.params.first()
                {
                    let inner = ts_type_to_rust_type(first);
                    return RustType::Promise(Box::new(inner));
                }
                RustType::Promise(Box::new(RustType::Unknown))
            } else if name == "Array" {
                if let Some(type_args) = &ref_type.type_arguments
                    && let Some(first) = type_args.params.first()
                {
                    let inner = ts_type_to_rust_type(first);
                    return RustType::Vec(Box::new(inner));
                }
                RustType::Vec(Box::new(RustType::Unknown))
            } else {
                RustType::Reference(name)
            }
        }
        TSTupleElement::TSUnionType(union) => {
            let variants: Vec<RustType> = union.types.iter().map(ts_type_to_rust_type).collect();
            RustType::Union(variants)
        }
        TSTupleElement::TSLiteralType(lit) => match &lit.literal {
            oxc_ast::ast::TSLiteral::StringLiteral(s) => {
                RustType::StringLiteral(s.value.to_string())
            }
            oxc_ast::ast::TSLiteral::NumericLiteral(n) => RustType::NumberLiteral(n.value),
            oxc_ast::ast::TSLiteral::BooleanLiteral(b) => RustType::BoolLiteral(b.value),
            _ => RustType::Unknown,
        },
        TSTupleElement::TSTupleType(tuple) => {
            let elements: Vec<RustType> = tuple
                .element_types
                .iter()
                .map(ts_tuple_element_to_rust_type)
                .collect();
            RustType::Tuple(elements)
        }
        _ => RustType::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ts(source: &str) -> ParsedModule {
        parse_from_str(source, std::path::Path::new("test.ts")).expect("parse_from_str 不应失败")
    }

    fn parse_ts_err(source: &str) -> Error {
        parse_from_str(source, std::path::Path::new("test.ts"))
            .expect_err("parse_from_str 应当失败")
    }

    #[test]
    fn test_parse_empty_module() {
        let module = parse_ts("");
        assert!(module.factory_functions.is_empty());
        assert!(module.interfaces.is_empty());
        assert!(module.enums.is_empty());
        assert!(module.global_decl.is_none());
    }

    #[test]
    fn test_parse_export_function() {
        let source = r#"
            export function createEditor(config: EditorConfig): Editor {
                return {} as Editor;
            }
        "#;
        let module = parse_ts(source);
        assert_eq!(module.factory_functions.len(), 1);
        let func = &module.factory_functions[0];
        assert_eq!(func.name, "createEditor");
        assert_eq!(func.params.len(), 1);
        assert_eq!(func.params[0].name, "config");
        assert!(matches!(&func.return_type, RustType::Reference(n) if n == "Editor"));
        assert!(!func.is_default_export);
    }

    #[test]
    fn test_parse_export_interface() {
        let source = r#"
            export interface Editor {
                takeVirtualRange(): string | Promise<string>;
                destroy(): void;
            }
        "#;
        let module = parse_ts(source);
        assert_eq!(module.interfaces.len(), 1);
        let iface = &module.interfaces[0];
        assert_eq!(iface.name, "Editor");
        assert_eq!(iface.methods.len(), 2);

        let take_range = &iface.methods[0];
        assert_eq!(take_range.name, "takeVirtualRange");
        assert!(matches!(&take_range.return_type, RustType::Union(_)));

        let destroy = &iface.methods[1];
        assert_eq!(destroy.name, "destroy");
        assert!(destroy.return_type.is_void());
    }

    #[test]
    fn test_parse_export_enum() {
        let source = r#"
            export enum Direction {
                Up = "up",
                Down = "down",
            }
        "#;
        let module = parse_ts(source);
        assert_eq!(module.enums.len(), 1);
        let enum_info = &module.enums[0];
        assert_eq!(enum_info.name, "Direction");
        assert_eq!(enum_info.members.len(), 2);
        assert_eq!(enum_info.members[0].name, "Up");
        assert_eq!(enum_info.members[0].value.as_deref(), Some("\"up\""));
    }

    #[test]
    fn test_parse_declare_global() {
        let source = r#"
            declare global {
                interface Window {
                    __mtNote: {
                        createEditor: (config: EditorConfig) => Editor;
                    };
                    __editor?: Editor;
                }
            }
        "#;
        let module = parse_ts(source);
        let global = module.global_decl.as_ref().expect("应有 global_decl");
        assert_eq!(global.factory_methods.len(), 1);
        assert_eq!(global.factory_methods[0].name, "__mtNote");
        assert_eq!(global.factory_methods[0].methods.len(), 1);
        assert_eq!(global.factory_methods[0].methods[0].name, "createEditor");

        assert_eq!(global.instance_vars.len(), 1);
        assert_eq!(global.instance_vars[0].name, "__editor");
        assert_eq!(global.instance_vars[0].type_name, "Editor");
        assert!(global.instance_vars[0].optional);
    }

    #[test]
    fn test_parse_syntax_error() {
        let source = "export function {{{{";
        let err = parse_ts_err(source);
        assert!(matches!(err, Error::ParseFailed(_)));
    }

    #[test]
    fn test_parse_interface_with_properties() {
        let source = r#"
            export interface EditorConfig {
                textareaId: string;
                highlightId: string;
                readOnly?: boolean;
            }
        "#;
        let module = parse_ts(source);
        assert_eq!(module.interfaces.len(), 1);
        let iface = &module.interfaces[0];
        assert_eq!(iface.name, "EditorConfig");
        assert_eq!(iface.properties.len(), 3);

        assert_eq!(iface.properties[0].name, "textareaId");
        assert!(!iface.properties[0].optional);

        assert_eq!(iface.properties[2].name, "readOnly");
        assert!(iface.properties[2].optional);
    }

    #[test]
    fn test_parse_type_aliases() {
        let source = r#"
            export type Handler = (event: string) => void;
        "#;
        let module = parse_ts(source);
        assert_eq!(module.type_aliases.len(), 1);
        assert_eq!(module.type_aliases[0].name, "Handler");
    }

    #[test]
    fn test_parse_primitives() {
        let source = r#"
            export interface Types {
                s: string;
                n: number;
                b: boolean;
                v: void;
            }
        "#;
        let module = parse_ts(source);
        let iface = &module.interfaces[0];
        assert!(matches!(iface.properties[0].ts_type, RustType::String));
        assert!(matches!(iface.properties[1].ts_type, RustType::F64));
        assert!(matches!(iface.properties[2].ts_type, RustType::Bool));
        assert!(matches!(iface.properties[3].ts_type, RustType::Void));
    }

    #[test]
    fn test_parse_promise_and_array() {
        let source = r#"
            export interface AsyncApi {
                fetchData(): Promise<string>;
                getItems(): Array<number>;
            }
        "#;
        let module = parse_ts(source);
        let iface = &module.interfaces[0];

        assert!(
            matches!(&iface.methods[0].return_type, RustType::Promise(inner) if **inner == RustType::String)
        );
        assert!(
            matches!(&iface.methods[1].return_type, RustType::Vec(inner) if **inner == RustType::F64)
        );
    }

    #[test]
    fn test_parse_export_default_function() {
        let source = r#"
            export default function createEditor(config: EditorConfig): Editor {
                return {} as Editor;
            }
        "#;
        let module = parse_ts(source);
        assert_eq!(module.factory_functions.len(), 1);
        assert!(module.factory_functions[0].is_default_export);
        assert_eq!(module.factory_functions[0].name, "createEditor");
    }

    #[test]
    fn test_parse_export_default_interface() {
        let source = r#"
            export default interface Editor {
                destroy(): void;
            }
        "#;
        let module = parse_ts(source);
        assert_eq!(module.interfaces.len(), 1);
        assert_eq!(module.interfaces[0].name, "Editor");
    }

    #[test]
    fn test_parse_non_exported_function() {
        let source = r#"
            function helper(x: number): void {}
        "#;
        let module = parse_ts(source);
        assert_eq!(
            module.factory_functions.len(),
            0,
            "非 export 的函数不应被收集"
        );
    }

    #[test]
    fn test_parse_non_exported_interface() {
        let source = r#"
            interface Helper { x: number; }
        "#;
        let module = parse_ts(source);
        assert_eq!(module.interfaces.len(), 0, "非 export 的接口不应被收集");
    }

    #[test]
    fn test_parse_non_exported_enum() {
        let source = r#"
            enum Color { Red = "red", Blue = "blue" }
        "#;
        let module = parse_ts(source);
        assert_eq!(module.enums.len(), 0, "非 export 的枚举不应被收集");
    }

    #[test]
    fn test_parse_non_exported_type_alias() {
        let source = r#"
            type ID = string;
        "#;
        let module = parse_ts(source);
        assert_eq!(
            module.type_aliases.len(),
            0,
            "非 export 的类型别名不应被收集"
        );
    }

    #[test]
    fn test_parse_ts_module_declaration_global() {
        let source = r#"
            declare module "global" {
                interface Window {
                    __mtNote: {
                        createEditor: () => Editor;
                    };
                }
            }
        "#;
        let module = parse_ts(source);
        let global = module.global_decl.as_ref().expect("应有 global_decl");
        assert_eq!(global.factory_methods.len(), 1);
        assert_eq!(global.factory_methods[0].name, "__mtNote");
    }

    #[test]
    fn test_parse_ts_module_declaration_non_global_ignored() {
        let source = r#"
            declare module "foo" {
                interface Something { x: number; }
            }
        "#;
        let module = parse_ts(source);
        assert!(module.global_decl.is_none());
    }

    #[test]
    fn test_parse_null_undefined_never_any_unknown_types() {
        let source = r#"
            export interface EdgeTypes {
                a: null;
                b: undefined;
                c: never;
                d: any;
                e: unknown;
                f: object;
            }
        "#;
        let module = parse_ts(source);
        let iface = &module.interfaces[0];
        assert!(matches!(iface.properties[0].ts_type, RustType::Null));
        assert!(matches!(iface.properties[1].ts_type, RustType::Undefined));
        assert!(matches!(iface.properties[2].ts_type, RustType::Never));
        assert!(matches!(iface.properties[3].ts_type, RustType::Unknown));
        assert!(matches!(iface.properties[4].ts_type, RustType::Unknown));
        assert!(matches!(iface.properties[5].ts_type, RustType::Unknown));
    }

    #[test]
    fn test_parse_literal_types() {
        let source = r#"
            export interface LiteralTypes {
                s: "hello";
                n: 42;
                b: true;
            }
        "#;
        let module = parse_ts(source);
        let iface = &module.interfaces[0];
        assert!(matches!(&iface.properties[0].ts_type, RustType::StringLiteral(s) if s == "hello"));
        assert!(matches!(&iface.properties[1].ts_type, RustType::NumberLiteral(n) if *n == 42.0));
        assert!(matches!(&iface.properties[2].ts_type, RustType::BoolLiteral(b) if *b));
    }

    #[test]
    fn test_parse_tuple_type() {
        let source = r#"
            export interface TupleApi {
                pair: [string, number];
            }
        "#;
        let module = parse_ts(source);
        let iface = &module.interfaces[0];
        assert!(matches!(&iface.properties[0].ts_type, RustType::Tuple(v) if v.len() == 2));
    }

    #[test]
    fn test_parse_promise_without_type_arg() {
        let source = r#"
            export interface Api {
                run(): Promise;
            }
        "#;
        let module = parse_ts(source);
        let iface = &module.interfaces[0];
        assert!(
            matches!(&iface.methods[0].return_type, RustType::Promise(inner) if **inner == RustType::Unknown)
        );
    }

    #[test]
    fn test_parse_array_without_type_arg() {
        let source = r#"
            export interface Api {
                items(): Array;
            }
        "#;
        let module = parse_ts(source);
        let iface = &module.interfaces[0];
        assert!(
            matches!(&iface.methods[0].return_type, RustType::Vec(inner) if **inner == RustType::Unknown)
        );
    }

    #[test]
    fn test_parse_enum_without_initializer() {
        let source = r#"
            export enum Status {
                Active,
                Inactive,
            }
        "#;
        let module = parse_ts(source);
        let enum_info = &module.enums[0];
        assert_eq!(enum_info.members.len(), 2);
        assert!(enum_info.members[0].value.is_none());
        assert!(enum_info.members[1].value.is_none());
    }

    #[test]
    fn test_parse_enum_numeric_initializer() {
        let source = r#"
            export enum Code {
                Ok = 200,
                NotFound = 404,
            }
        "#;
        let module = parse_ts(source);
        let enum_info = &module.enums[0];
        assert_eq!(enum_info.members[0].value.as_deref(), Some("200"));
        assert_eq!(enum_info.members[1].value.as_deref(), Some("404"));
    }

    #[test]
    fn test_parse_enum_boolean_initializer() {
        let source = r#"
            export enum Flag {
                On = true,
                Off = false,
            }
        "#;
        let module = parse_ts(source);
        let enum_info = &module.enums[0];
        assert_eq!(enum_info.members[0].value.as_deref(), Some("true"));
        assert_eq!(enum_info.members[1].value.as_deref(), Some("false"));
    }

    #[test]
    fn test_parse_interface_with_call_signature() {
        let source = r#"
            export interface Callable {
                (x: string): void;
                name: string;
            }
        "#;
        let module = parse_ts(source);
        let iface = &module.interfaces[0];
        assert!(iface.methods.iter().any(|m| m.name == "(call)"));
        assert!(iface.properties.iter().any(|p| p.name == "name"));
    }

    // ========== doc_comment 提取测试 ==========

    #[test]
    fn test_clean_doc_comment_with_tags() {
        let raw = "  描述文本\n   * @rust-lint expect(dead_code, reason = \"由 Rust 侧调用\")\n   * @param x 参数说明\n   * 更多描述";
        let cleaned = clean_doc_comment(raw);
        assert_eq!(cleaned, Some("描述文本\n更多描述".to_string()));
    }

    #[test]
    fn test_clean_doc_comment_pure_text() {
        let raw = "  纯文本描述\n   * 第二行\n   * 第三行";
        let cleaned = clean_doc_comment(raw);
        assert_eq!(cleaned, Some("纯文本描述\n第二行\n第三行".to_string()));
    }

    #[test]
    fn test_clean_doc_comment_only_tags() {
        let raw = "  @rust-lint allow(dead_code)\n   * @param x\n   * @returns void";
        let cleaned = clean_doc_comment(raw);
        assert_eq!(cleaned, None);
    }

    #[test]
    fn test_clean_doc_comment_empty() {
        let raw = "";
        let cleaned = clean_doc_comment(raw);
        assert_eq!(cleaned, None);
    }

    #[test]
    fn test_extract_doc_comment_top_level_interface() {
        let source = r#"
            /** 文本编辑器：滚动同步、光标跟踪、剪贴板。 */
            export interface Editor {
                init: () => void;
            }
        "#;
        let module = parse_ts(source);
        let iface = &module.interfaces[0];
        assert_eq!(
            iface.doc_comment,
            Some("文本编辑器：滚动同步、光标跟踪、剪贴板。".to_string())
        );
    }

    #[test]
    fn test_extract_doc_comment_top_level_function() {
        let source = r#"
            /** 创建编辑器实例并返回 Editor 对象。 */
            export function createEditor(x: string): Editor {
                return {} as any;
            }
        "#;
        let module = parse_ts(source);
        let func = &module.factory_functions[0];
        assert_eq!(
            func.doc_comment,
            Some("创建编辑器实例并返回 Editor 对象。".to_string())
        );
    }

    #[test]
    fn test_extract_doc_comment_method() {
        let source = r#"
            export interface Editor {
                /** 获取滚动位置信息。 */
                getScrollInfo(): string;
                /** 销毁编辑器实例。 */
                destroy(): void;
            }
        "#;
        let module = parse_ts(source);
        let iface = &module.interfaces[0];
        let get_info = iface
            .methods
            .iter()
            .find(|m| m.name == "getScrollInfo")
            .unwrap();
        assert_eq!(get_info.doc_comment, Some("获取滚动位置信息。".to_string()));
        let destroy = iface.methods.iter().find(|m| m.name == "destroy").unwrap();
        assert_eq!(destroy.doc_comment, Some("销毁编辑器实例。".to_string()));
    }

    #[test]
    fn test_extract_doc_comment_mixed_with_rust_lint() {
        let source = r#"
            export interface Editor {
                /** 清理事件监听器。 @rust-lint expect(dead_code, reason = "由 Rust 侧调用") */
                cleanup(): void;
            }
        "#;
        let module = parse_ts(source);
        let iface = &module.interfaces[0];
        let cleanup = iface.methods.iter().find(|m| m.name == "cleanup").unwrap();
        assert_eq!(cleanup.doc_comment, Some("清理事件监听器。".to_string()));
        assert_eq!(cleanup.rust_lint_attrs.len(), 1);
        assert_eq!(
            cleanup.rust_lint_attrs[0].content,
            "expect(dead_code, reason = \"由 Rust 侧调用\")"
        );
    }

    #[test]
    fn test_extract_doc_comment_no_doc() {
        let source = r#"
            export interface NoDoc {
                doStuff(): void;
            }
        "#;
        let module = parse_ts(source);
        let iface = &module.interfaces[0];
        assert_eq!(iface.doc_comment, None);
        let method = &iface.methods[0];
        assert_eq!(method.doc_comment, None);
    }

    #[test]
    fn test_extract_doc_comment_multiline() {
        let source = r#"
            /**
             * 捏合结束：清除 transform，将缩放应用到实际 DOM 尺寸（width），
             * 并同步滚动位置使焦点不动。
             */
            export function pinchEnd(ns: number): void {}
        "#;
        let module = parse_ts(source);
        let func = &module.factory_functions[0];
        let expected = "捏合结束：清除 transform，将缩放应用到实际 DOM 尺寸（width），\n并同步滚动位置使焦点不动。";
        assert_eq!(func.doc_comment, Some(expected.to_string()));
    }

    #[test]
    fn test_parse_interface_readonly_property() {
        let source = r#"
            export interface ReadOnly {
                readonly id: string;
            }
        "#;
        let module = parse_ts(source);
        let iface = &module.interfaces[0];
        assert!(iface.properties[0].readonly);
    }

    #[test]
    fn test_parse_method_with_params() {
        let source = r#"
            export interface Editor {
                setHighlight(id: string, range: number): void;
            }
        "#;
        let module = parse_ts(source);
        let method = &module.interfaces[0].methods[0];
        assert_eq!(method.params.len(), 2);
        assert_eq!(method.params[0].name, "id");
        assert_eq!(method.params[1].name, "range");
    }

    #[test]
    fn test_parse_method_optional_param() {
        let source = r#"
            export interface Editor {
                configure(opts?: string): void;
            }
        "#;
        let module = parse_ts(source);
        let param = &module.interfaces[0].methods[0].params[0];
        assert!(param.optional);
    }

    #[test]
    fn test_parse_method_no_return_type() {
        let source = r#"
            export interface Editor {
                doSomething(): void;
            }
        "#;
        let module = parse_ts(source);
        let method = &module.interfaces[0].methods[0];
        assert!(method.return_type.is_void());
    }

    #[test]
    fn test_parse_declare_global_with_factory_object() {
        let source = r#"
            declare global {
                interface Window {
                    __mtNote: {
                        createEditor: (config: EditorConfig) => Editor;
                        createViewer: () => Viewer;
                    };
                    __editor?: Editor;
                    __viewer: Viewer;
                }
            }
        "#;
        let module = parse_ts(source);
        let global = module.global_decl.as_ref().expect("应有 global_decl");
        assert_eq!(global.factory_methods.len(), 1);
        assert_eq!(global.factory_methods[0].methods.len(), 2);
        assert_eq!(global.factory_methods[0].methods[0].name, "createEditor");
        assert_eq!(global.factory_methods[0].methods[1].name, "createViewer");
        assert_eq!(global.instance_vars.len(), 2);
        assert!(global.instance_vars[0].optional);
        assert!(!global.instance_vars[1].optional);
    }

    #[test]
    fn test_parse_function_no_return_type() {
        let source = r#"
            export function doSomething(): void {}
        "#;
        let module = parse_ts(source);
        assert_eq!(module.factory_functions.len(), 1);
        assert!(module.factory_functions[0].return_type.is_void());
    }

    #[test]
    fn test_parse_function_param_no_type_annotation() {
        let source = r#"
            export function greet(name): void {}
        "#;
        let module = parse_ts(source);
        let param = &module.factory_functions[0].params[0];
        assert_eq!(param.name, "name");
        assert!(matches!(param.ts_type, RustType::Unknown));
    }

    #[test]
    fn test_parse_union_type() {
        let source = r#"
            export interface Mixed {
                value: string | number | boolean;
            }
        "#;
        let module = parse_ts(source);
        let prop = &module.interfaces[0].properties[0];
        assert!(matches!(&prop.ts_type, RustType::Union(v) if v.len() == 3));
    }

    #[test]
    fn test_parse_type_reference() {
        let source = r#"
            export interface Ref {
                editor: Editor;
            }
        "#;
        let module = parse_ts(source);
        let prop = &module.interfaces[0].properties[0];
        assert!(matches!(&prop.ts_type, RustType::Reference(n) if n == "Editor"));
    }

    #[test]
    fn test_collect_relative_imports_ignores_npm() {
        let source = r#"
            import { something } from 'lodash';
            import { other } from './local';
        "#;
        let allocator = Allocator::default();
        let source_type = SourceType::default().with_typescript(true);
        let result = Parser::new(&allocator, source, source_type).parse();
        let program = result.program;
        let specs = collect_relative_import_specs(&program);
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0], "./local");
    }

    #[test]
    fn test_collect_relative_imports_handles_reexport() {
        let source = r#"
            export { createEditor, type Editor } from './editor';
        "#;
        let allocator = Allocator::default();
        let source_type = SourceType::default().with_typescript(true);
        let result = Parser::new(&allocator, source, source_type).parse();
        let program = result.program;
        let specs = collect_relative_import_specs(&program);
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0], "./editor");
    }

    #[test]
    fn test_collect_relative_imports_handles_export_all() {
        let source = r#"
            export * from './shared';
        "#;
        let allocator = Allocator::default();
        let source_type = SourceType::default().with_typescript(true);
        let result = Parser::new(&allocator, source, source_type).parse();
        let program = result.program;
        let specs = collect_relative_import_specs(&program);
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0], "./shared");
    }

    #[test]
    fn test_parse_interface_property_as_method() {
        let source = r#"
            export interface Handler {
                onClick: (event: string) => void;
                name: string;
            }
        "#;
        let module = parse_ts(source);
        let iface = &module.interfaces[0];
        assert_eq!(iface.methods.len(), 1, "函数类型属性应被解析为方法");
        assert_eq!(iface.methods[0].name, "onClick");
        assert_eq!(iface.methods[0].params.len(), 1);
        assert_eq!(iface.properties.len(), 1, "非函数类型属性应保留为属性");
        assert_eq!(iface.properties[0].name, "name");
    }

    #[test]
    fn test_parse_window_method_signature() {
        let source = r#"
            declare global {
                interface Window {
                    __mtNote: {
                        createEditor(config: EditorConfig): Editor;
                    };
                    __editor?: Editor;
                }
            }
        "#;
        let module = parse_ts(source);
        let global = module.global_decl.as_ref().expect("应有 global_decl");
        assert_eq!(global.factory_methods[0].methods.len(), 1);
        assert_eq!(global.factory_methods[0].methods[0].name, "createEditor");
        assert_eq!(global.factory_methods[0].methods[0].params.len(), 1);
    }

    #[test]
    fn test_parse_array_type_shorthand() {
        let source = r#"
            export interface Api {
                items: string[];
            }
        "#;
        let module = parse_ts(source);
        let prop = &module.interfaces[0].properties[0];
        assert!(matches!(&prop.ts_type, RustType::Vec(inner) if **inner == RustType::String));
    }

    #[test]
    fn test_parse_parenthesized_type() {
        let source = r#"
            export interface Api {
                value: (string | number);
            }
        "#;
        let module = parse_ts(source);
        let prop = &module.interfaces[0].properties[0];
        assert!(matches!(&prop.ts_type, RustType::Union(v) if v.len() == 2));
    }

    #[test]
    fn test_parse_named_tuple_member() {
        let source = r#"
            export interface Api {
                pair: [first: string, second: number];
            }
        "#;
        let module = parse_ts(source);
        let prop = &module.interfaces[0].properties[0];
        assert!(matches!(&prop.ts_type, RustType::Tuple(v) if v.len() == 2));
    }

    #[test]
    fn test_parse_tsglobal_declaration() {
        let source = r#"
            global {
                interface Window {
                    __mtNote: {
                        createEditor(): Editor;
                    };
                }
            }
        "#;
        let module = parse_ts(source);
        let global = module
            .global_decl
            .as_ref()
            .expect("TSGlobalDeclaration 应被解析");
        assert_eq!(global.factory_methods.len(), 1);
    }

    #[test]
    fn test_parse_qualified_type_name() {
        let source = r#"
            export interface Api {
                value: Namespace.Type;
            }
        "#;
        let module = parse_ts(source);
        let prop = &module.interfaces[0].properties[0];
        if let RustType::Reference(name) = &prop.ts_type {
            assert_eq!(name, "Namespace.Type");
        } else {
            panic!("应为 Reference 类型，实际: {:?}", prop.ts_type);
        }
    }

    #[test]
    fn test_parse_object_pattern_param() {
        let source = r#"
            export interface Api {
                method({ name }: { name: string }): void;
            }
        "#;
        let module = parse_ts(source);
        let method = &module.interfaces[0].methods[0];
        assert_eq!(method.params.len(), 1);
        assert_eq!(method.params[0].name, "name");
    }

    #[test]
    fn test_parse_enum_string_member_name() {
        let source = r#"
            export enum Status {
                "active" = "active",
                "inactive" = "inactive",
            }
        "#;
        let module = parse_ts(source);
        let enum_info = &module.enums[0];
        assert_eq!(enum_info.members[0].name, "active");
        assert_eq!(enum_info.members[1].name, "inactive");
    }

    #[test]
    fn test_parse_non_exported_module_declaration() {
        let source = r#"
            declare module "global" {
                interface Window {
                    __f: {
                        createThing(): Thing;
                    };
                }
            }
        "#;
        let module = parse_ts(source);
        assert!(
            module.global_decl.is_some(),
            "declare module 'global' 应被解析为 global_decl"
        );
    }

    #[test]
    fn test_parse_string_literal_property_key() {
        let source = r#"
            declare global {
                interface Window {
                    "__mtNote": {
                        createEditor(): Editor;
                    };
                }
            }
        "#;
        let module = parse_ts(source);
        let global = module.global_decl.as_ref().expect("应有 global_decl");
        assert_eq!(global.factory_methods[0].name, "__mtNote");
    }

    #[test]
    fn test_parse_complex_initializer() {
        let source = r#"
            export enum Complex {
                A = someFunc(),
            }
        "#;
        let module = parse_ts(source);
        let enum_info = &module.enums[0];
        assert_eq!(enum_info.members[0].value.as_deref(), Some("..."));
    }

    #[test]
    fn test_parse_nested_object_methods() {
        let source = r#"
            export interface PdfPreview {
                configure(workerUrl: string, cmapsUrl: string): void;
                render: {
                    load(b64: string, cid: string, scale: number): Promise<string>;
                    setScale(scale: number): void;
                };
                query: {
                    pageInfos(): string;
                    contentRect(): string;
                };
                cleanup(): void;
            }
        "#;
        let module = parse_ts(source);
        let iface = &module.interfaces[0];
        assert_eq!(iface.name, "PdfPreview");
        assert!(
            iface.properties.is_empty(),
            "嵌套对象属性应提取为方法，不应残留为 properties"
        );
        let path_of = |name: &str| -> Vec<String> {
            iface
                .methods
                .iter()
                .find(|m| m.name == name)
                .map(|m| m.path.clone())
                .unwrap_or_else(|| panic!("方法 {name} 未生成"))
        };
        assert_eq!(path_of("configure"), Vec::<String>::new());
        assert_eq!(path_of("cleanup"), Vec::<String>::new());
        assert_eq!(path_of("load"), vec!["render".to_string()]);
        assert_eq!(path_of("setScale"), vec!["render".to_string()]);
        assert_eq!(path_of("pageInfos"), vec!["query".to_string()]);
        assert_eq!(path_of("contentRect"), vec!["query".to_string()]);
        // 嵌套方法参数应被正确提取（方法形式 setScale(scale)）
        let set_scale = iface.methods.iter().find(|m| m.name == "setScale").unwrap();
        assert_eq!(set_scale.params.len(), 1);
        assert_eq!(set_scale.params[0].name, "scale");
    }
}
