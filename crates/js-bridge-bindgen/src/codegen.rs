use crate::config::Config;
use crate::error::Error;
use crate::types::*;

/// 从解析结果生成 Rust 绑定代码字符串（纯计算，不写文件）。
pub fn generate_code(module: &ParsedModule, config: &Config) -> String {
    let mut w = CodeWriter::new();

    emit_header(&mut w, module);
    emit_factory_functions(&mut w, module, config);
    emit_structs(&mut w, module, config);
    emit_enums(&mut w, module, config);

    w.into_string()
}

/// 从解析结果生成 Rust 绑定代码并写入文件。
pub fn generate(module: &ParsedModule, config: &Config) -> Result<(), Error> {
    let code = generate_code(module, config);

    std::fs::create_dir_all(&config.out_dir).map_err(|e| Error::OutDirCreateFailed {
        path: config.out_dir.clone(),
        source: e,
    })?;

    let output_path = config.out_dir.join(&config.output_file);
    std::fs::write(&output_path, &code).map_err(|e| Error::OutputWriteFailed {
        path: output_path.clone(),
        source: e,
    })?;

    Ok(())
}

fn emit_header(w: &mut CodeWriter, _module: &ParsedModule) {
    w.write_line("// !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
    w.write_line("// !!! 此文件由 js-bridge-bindgen 自动生成，请勿手动编辑 !!!");
    w.write_line("// !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
    w.write_line("");
    w.write_line("use js_bridge_bindgen::JsBridge;");

    w.write_line("");
}

/// 输出 `///` 文档注释行（若 `doc_comment` 为 `Some`）。
fn emit_doc_comment(w: &mut CodeWriter, doc_comment: &Option<String>) {
    if let Some(doc) = doc_comment {
        for line in doc.lines() {
            w.write_line(&format!("/// {line}"));
        }
    }
}

fn emit_rust_lint_attrs(w: &mut CodeWriter, attrs: &[RustLintAttr]) {
    for attr in attrs {
        w.write_line(&format!("#[{}]", attr.content));
    }
}

fn emit_factory_functions(w: &mut CodeWriter, module: &ParsedModule, config: &Config) {
    if let Some(global_decl) = &module.global_decl {
        let factory_name = &config.global_factory_name;
        if let Some(factory) = global_decl
            .factory_methods
            .iter()
            .find(|m| m.name == *factory_name)
        {
            emit_factory_methods(w, module, config, factory);
        }
    }
}

fn emit_factory_methods(
    w: &mut CodeWriter,
    module: &ParsedModule,
    config: &Config,
    factory: &crate::types::FactoryMethodDecl,
) {
    let known_types: Vec<String> = module
        .interfaces
        .iter()
        .map(|i| i.name.clone())
        .chain(module.enums.iter().map(|e| e.name.clone()))
        .chain(module.type_aliases.iter().map(|t| t.name.clone()))
        .collect();

    for method in &factory.methods {
        let factory_func = module
            .factory_functions
            .iter()
            .find(|f| f.name == method.name);
        let Some(func) = factory_func else {
            continue;
        };

        let instance_var = config.resolve_instance_var(&func.name);
        let module_name = factory_func_to_module_name(&func.name);

        let iface = module.interfaces.iter().find(|i| {
            if let RustType::Reference(ref_name) = &func.return_type {
                i.name == *ref_name
            } else {
                false
            }
        });

        if let Some(iface) = iface {
            emit_doc_comment(w, &iface.doc_comment);
            emit_rust_lint_attrs(w, &iface.rust_lint_attrs);
        }
        w.write_line(&format!("pub mod {module_name} {{"));
        w.indent();

        emit_create_fn(
            w,
            &func.name,
            &func.params,
            &func.doc_comment,
            &func.rust_lint_attrs,
            &instance_var,
            config,
            &known_types,
        );
        w.write_line("");

        if let Some(iface) = iface {
            for method in &iface.methods {
                if method.name == "destroy" {
                    emit_destroy_fn(
                        w,
                        &instance_var,
                        &method.doc_comment,
                        &method.rust_lint_attrs,
                    );
                } else {
                    emit_instance_method_fn(w, method, &instance_var, &known_types);
                }
            }
            if !iface.methods.iter().any(|m| m.name == "destroy") {
                emit_destroy_fn(w, &instance_var, &None, &[]);
            }
        }

        w.dedent();
        w.write_line("}");
        w.write_line("");
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "generated bindings require multiple parameters"
)]
fn emit_create_fn(
    w: &mut CodeWriter,
    factory_name: &str,
    params: &[ParamInfo],
    doc_comment: &Option<String>,
    rust_lint_attrs: &[RustLintAttr],
    instance_var: &str,
    config: &Config,
    known_types: &[String],
) {
    if has_non_serializable_param(params, known_types) {
        w.write_line(&format!("// 跳过 {factory_name}: 参数包含不可序列化类型"));
    } else {
        emit_create_fn_inner(
            w,
            factory_name,
            params,
            doc_comment,
            rust_lint_attrs,
            instance_var,
            config,
        );
    }
}

fn emit_create_fn_inner(
    w: &mut CodeWriter,
    factory_name: &str,
    params: &[ParamInfo],
    doc_comment: &Option<String>,
    rust_lint_attrs: &[RustLintAttr],
    instance_var: &str,
    config: &Config,
) {
    let rust_params: Vec<String> = params
        .iter()
        .map(|p| format!("{}: {}", to_snake_case(&p.name), ts_param_to_rust(p)))
        .collect();
    let rust_params_str = rust_params.join(", ");

    emit_doc_comment(w, doc_comment);
    emit_rust_lint_attrs(w, rust_lint_attrs);
    w.write_line(&format!(
        "pub fn create<B: super::JsBridge>({rust_params_str}) {{"
    ));
    w.indent();

    w.write_line("B::ensure_injected();");
    w.write_line("let g = B::GLOBAL_OBJECT;");

    let type_name = factory_name.strip_prefix("create").unwrap_or(factory_name);

    let js_args = if params.is_empty() {
        String::new()
    } else {
        let args: Vec<String> = params
            .iter()
            .map(|p| {
                let rust_name = to_snake_case(&p.name);
                if ts_param_is_string(p) {
                    format!("{{{rust_name}_json}}")
                } else if ts_param_is_struct(p) {
                    format!("JSON.parse('{{{rust_name}_json}}')")
                } else if ts_param_is_optional(p) {
                    format!("{{{rust_name}_js}}")
                } else {
                    format!("{{{rust_name}}}")
                }
            })
            .collect();
        args.join(",")
    };

    for p in params {
        if ts_param_is_string(p) || ts_param_is_struct(p) {
            let n = to_snake_case(&p.name);
            w.write_line(&format!(
                "let {n}_json = serde_json::to_string(&{n}).unwrap_or_default();"
            ));
        }
        if ts_param_is_optional(p) && !ts_param_is_string(p) && !ts_param_is_struct(p) {
            let n = to_snake_case(&p.name);
            w.write_line(&format!(
                "let {n}_js = match &{n} {{ Some(v) => v.to_string(), None => \"undefined\".to_string() }};"
            ));
        }
    }

    let mut format_parts = vec!["g=g".to_string()];
    for p in params {
        let rust_name = to_snake_case(&p.name);
        if ts_param_is_string(p) {
            format_parts.push(format!("{rust_name}_json={rust_name}_json"));
        } else if ts_param_is_struct(p) {
            format_parts.push(format!(
                "{rust_name}_json={rust_name}_json.replace(\"'\", \"\\\\'\")"
            ));
        } else if ts_param_is_optional(p) {
            format_parts.push(format!("{rust_name}_js={rust_name}_js"));
        } else {
            format_parts.push(format!("{rust_name}={rust_name}"));
        }
    }
    let all_format_args = format_parts.join(", ");

    w.write_line(&format!(
        "B::eval_js(&format!(\"if(!{{g}}.{instance_var}){{g}}.{instance_var}={{g}}.{fac}.create{type_name}({js_args})\", {all_format_args}));",
        fac = config.global_factory_name,
    ));

    w.dedent();
    w.write_line("}");
}

fn emit_destroy_fn(
    w: &mut CodeWriter,
    instance_var: &str,
    doc_comment: &Option<String>,
    rust_lint_attrs: &[RustLintAttr],
) {
    emit_doc_comment(w, doc_comment);
    emit_rust_lint_attrs(w, rust_lint_attrs);
    w.write_line("pub fn destroy<B: super::JsBridge>() {");
    w.indent();
    w.write_line("B::ensure_injected();");
    w.write_line("let g = B::GLOBAL_OBJECT;");
    w.write_line(&format!(
        "let js = [\"if(\", g, \".{instance_var}){{\", g, \".{instance_var}.destroy();\", g, \".{instance_var}=undefined;}}\"].concat();"
    ));
    w.write_line("B::eval_js(&js);");
    w.dedent();
    w.write_line("}");
}

fn emit_instance_method_fn(
    w: &mut CodeWriter,
    method: &MethodSignature,
    instance_var: &str,
    known_types: &[String],
) {
    if has_non_serializable_param(&method.params, known_types) {
        let full_name = if method.path.is_empty() {
            method.name.clone()
        } else {
            format!("{}.{}", method.path.join("."), method.name)
        };
        w.write_line(&format!("// 跳过 {full_name}: 参数包含不可序列化类型"));
    } else {
        emit_instance_method_fn_inner(w, method, instance_var);
    }
}

fn emit_instance_method_fn_inner(w: &mut CodeWriter, method: &MethodSignature, instance_var: &str) {
    let method_name = &method.name;
    let params = &method.params;
    let return_type = &method.return_type;
    let doc_comment = &method.doc_comment;
    let rust_lint_attrs = &method.rust_lint_attrs;
    let path = &method.path;
    let rust_fn_name = if path.is_empty() {
        to_snake_case(method_name)
    } else {
        let mut parts: Vec<String> = path.iter().map(|p| to_snake_case(p)).collect();
        parts.push(to_snake_case(method_name));
        parts.join("_")
    };
    let rust_params: Vec<String> = params
        .iter()
        .map(|p| format!("{}: {}", to_snake_case(&p.name), ts_param_to_rust(p)))
        .collect();
    let rust_params_str = rust_params.join(", ");

    let is_void = return_type.is_void() || return_type.is_promise_void();
    let needs_async = return_type.needs_async_return();

    let return_sig = if is_void {
        String::new()
    } else {
        format!(" -> {}", rust_type_with_super(return_type))
    };

    let async_kw = if needs_async { "async " } else { "" };

    emit_doc_comment(w, doc_comment);
    emit_rust_lint_attrs(w, rust_lint_attrs);
    w.write_line(&format!(
        "pub {async_kw}fn {rust_fn_name}<B: super::JsBridge>({rust_params_str}){return_sig} {{"
    ));
    w.indent();

    w.write_line("B::ensure_injected();");
    w.write_line("let g = B::GLOBAL_OBJECT;");

    for p in params {
        if ts_param_is_string(p) || ts_param_is_struct(p) {
            let n = to_snake_case(&p.name);
            w.write_line(&format!(
                "let {n}_json = serde_json::to_string(&{n}).unwrap_or_default();"
            ));
        }
        if ts_param_is_optional(p) && !ts_param_is_string(p) && !ts_param_is_struct(p) {
            let n = to_snake_case(&p.name);
            w.write_line(&format!(
                "let {n}_js = match &{n} {{ Some(v) => v.to_string(), None => \"undefined\".to_string() }};"
            ));
        }
    }

    let js_args = build_js_args(params);
    let access_path = if path.is_empty() {
        format!("{{g}}.{instance_var}.{method_name}")
    } else {
        format!("{{g}}.{instance_var}.{}.{}", path.join("."), method_name)
    };
    let js_call = if params.is_empty() {
        format!("{access_path}()")
    } else {
        format!("{access_path}({js_args})")
    };

    let mut format_parts = vec!["g=g".to_string()];
    for p in params {
        let rust_name = to_snake_case(&p.name);
        if ts_param_is_string(p) {
            format_parts.push(format!("{rust_name}_json={rust_name}_json"));
        } else if ts_param_is_struct(p) {
            format_parts.push(format!(
                "{rust_name}_json={rust_name}_json.replace(\"'\", \"\\\\'\")"
            ));
        } else if ts_param_is_optional(p) {
            format_parts.push(format!("{rust_name}_js={rust_name}_js"));
        } else {
            format_parts.push(format!("{rust_name}={rust_name}"));
        }
    }
    let all_format_args = format_parts.join(", ");

    if is_void {
        w.write_line(&format!(
            "B::eval_js(&format!(\"if({{g}}.{instance_var}){js_call}\", {all_format_args}));",
        ));
    } else if needs_async {
        w.write_line(&format!(
            "let __v = B::eval_async(&format!(\"if({{g}}.{instance_var})return {js_call}\", {all_format_args})).await;"
        ));
        let rust_type = rust_type_with_super(return_type);
        w.write_line(&format!(
            "serde_json::from_value::<{rust_type}>(__v).unwrap_or_default()"
        ));
    } else {
        w.write_line(&format!(
            "B::eval_js(&format!(\"if({{g}}.{instance_var}){js_call}\", {all_format_args}));",
        ));
    }

    w.dedent();
    w.write_line("}");
}

fn emit_structs(w: &mut CodeWriter, module: &ParsedModule, _config: &Config) {
    for iface in &module.interfaces {
        if iface.properties.is_empty() {
            continue;
        }

        let struct_name = &iface.name;
        emit_doc_comment(w, &iface.doc_comment);
        emit_rust_lint_attrs(w, &iface.rust_lint_attrs);
        w.write_line("#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]");
        w.write_line(&format!("pub struct {struct_name} {{"));
        w.indent();

        for prop in &iface.properties {
            let rust_type = if prop.optional {
                format!("Option<{}>", prop.ts_type.to_rust_type_string())
            } else {
                prop.ts_type.to_rust_type_string()
            };
            emit_doc_comment(w, &prop.doc_comment);
            w.write_line(&format!("#[serde(rename = \"{}\")]", prop.name));
            w.write_line(&format!("pub {}: {rust_type},", to_snake_case(&prop.name)));
        }

        w.dedent();
        w.write_line("}");
        w.write_line("");
    }
}

fn emit_enums(w: &mut CodeWriter, module: &ParsedModule, _config: &Config) {
    for enum_info in &module.enums {
        let enum_name = &enum_info.name;
        w.write_line("#[derive(Debug, Clone, PartialEq)]");
        w.write_line(&format!("pub enum {enum_name} {{"));
        w.indent();

        for member in &enum_info.members {
            w.write_line(&format!("{},", member.name));
        }

        w.dedent();
        w.write_line("}");
        w.write_line("");
    }
}

fn ts_param_to_rust(param: &ParamInfo) -> String {
    let base_type = match &param.ts_type {
        RustType::String => "&str".to_string(),
        RustType::F64 => "f64".to_string(),
        RustType::Bool => "bool".to_string(),
        RustType::Reference(name) => format!("super::{name}"),
        other => other.to_rust_type_string(),
    };

    if param.optional {
        format!("Option<{base_type}>")
    } else {
        base_type
    }
}

fn rust_type_with_super(rt: &RustType) -> String {
    match rt {
        RustType::Reference(name) => format!("super::{name}"),
        RustType::Vec(inner) => format!("Vec<{}>", rust_type_with_super(inner)),
        RustType::Promise(inner) => {
            if inner.is_void() {
                "()".to_string()
            } else {
                rust_type_with_super(inner)
            }
        }
        RustType::Union(variants) => {
            if rt.is_string_or_promise_string() {
                "String".to_string()
            } else if variants.len() == 2 && variants.iter().any(|v| matches!(v, RustType::Null)) {
                let non_null = variants.iter().find(|v| !matches!(v, RustType::Null));
                if let Some(inner) = non_null {
                    format!("Option<{}>", rust_type_with_super(inner))
                } else {
                    "serde_json::Value".to_string()
                }
            } else {
                "serde_json::Value".to_string()
            }
        }
        other => other.to_rust_type_string(),
    }
}

fn ts_param_is_string(param: &ParamInfo) -> bool {
    matches!(param.ts_type, RustType::String)
}

fn ts_param_is_struct(param: &ParamInfo) -> bool {
    matches!(param.ts_type, RustType::Reference(_))
}

fn ts_param_is_optional(param: &ParamInfo) -> bool {
    param.optional || ts_param_is_null_union(&param.ts_type)
}

/// `T | null` 联合类型映射为 `Option<T>`，按可空参数处理（生成 `{n}_js` 插值，
/// None 传 `undefined`），否则会落入裸 Display 插值导致编译失败
fn ts_param_is_null_union(t: &RustType) -> bool {
    matches!(t, RustType::Union(variants)
        if variants.len() == 2 && variants.iter().any(|v| matches!(v, RustType::Null)))
}

fn has_non_serializable_param(params: &[ParamInfo], known_types: &[String]) -> bool {
    params.iter().any(|p| {
        if let RustType::Reference(name) = &p.ts_type {
            !known_types.contains(name)
        } else {
            false
        }
    })
}

fn build_js_args(params: &[ParamInfo]) -> String {
    let args: Vec<String> = params
        .iter()
        .map(|p| {
            let rust_name = to_snake_case(&p.name);
            if ts_param_is_string(p) {
                format!("{{{rust_name}_json}}")
            } else if ts_param_is_struct(p) {
                format!("JSON.parse('{{{rust_name}_json}}')")
            } else if ts_param_is_optional(p) {
                format!("{{{rust_name}_js}}")
            } else {
                format!("{{{rust_name}}}")
            }
        })
        .collect();
    args.join(",")
}

fn factory_func_to_module_name(name: &str) -> String {
    let name = name.strip_prefix("create").unwrap_or(name);
    to_snake_case(name)
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

#[cfg(test)]
fn to_camel_case_param(s: &str) -> String {
    let mut result = String::new();
    let mut next_upper = false;
    for c in s.chars() {
        if c == '_' {
            next_upper = true;
        } else if next_upper {
            result.push(c.to_ascii_uppercase());
            next_upper = false;
        } else {
            result.push(c);
        }
    }
    result
}

struct CodeWriter {
    lines: Vec<String>,
    indent_level: usize,
}

impl CodeWriter {
    fn new() -> Self {
        Self {
            lines: Vec::new(),
            indent_level: 0,
        }
    }

    fn write_line(&mut self, line: &str) {
        if line.is_empty() {
            self.lines.push(String::new());
        } else {
            let indent = "    ".repeat(self.indent_level);
            self.lines.push(format!("{indent}{line}"));
        }
    }

    fn indent(&mut self) {
        self.indent_level += 1;
    }

    fn dedent(&mut self) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
    }

    fn into_string(self) -> String {
        self.lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use super::*;

    fn make_config() -> Config {
        Config {
            entry: PathBuf::from("test.ts"),
            out_dir: PathBuf::from("/tmp"),
            global_factory_name: "__mtNote".into(),
            ..Default::default()
        }
    }

    fn make_editor_module() -> ParsedModule {
        ParsedModule {
            factory_functions: vec![FactoryFunction {
                doc_comment: None,
                name: "createEditor".into(),
                params: vec![
                    ParamInfo {
                        name: "textarea_id".into(),
                        ts_type: RustType::String,
                        optional: false,
                    },
                    ParamInfo {
                        name: "highlight_id".into(),
                        ts_type: RustType::String,
                        optional: false,
                    },
                ],
                return_type: RustType::Reference("Editor".into()),
                is_default_export: false,
                rust_lint_attrs: Vec::new(),
            }],
            interfaces: vec![InterfaceInfo {
                name: "Editor".into(),
                methods: vec![
                    MethodSignature {
                        name: "takeVirtualRange".into(),
                        params: vec![],
                        return_type: RustType::Union(vec![
                            RustType::String,
                            RustType::Promise(Box::new(RustType::String)),
                        ]),
                        ..Default::default()
                    },
                    MethodSignature {
                        name: "destroy".into(),
                        params: vec![],
                        return_type: RustType::Void,
                        ..Default::default()
                    },
                ],
                properties: vec![],
                ..Default::default()
            }],
            enums: vec![],
            type_aliases: vec![],
            global_decl: Some(GlobalDeclaration {
                factory_methods: vec![FactoryMethodDecl {
                    name: "__mtNote".into(),
                    methods: vec![MethodSignature {
                        name: "createEditor".into(),
                        params: vec![],
                        return_type: RustType::Reference("Editor".into()),
                        ..Default::default()
                    }],
                }],
                instance_vars: vec![InstanceVarDecl {
                    name: "__editor".into(),
                    type_name: "Editor".into(),
                    optional: true,
                }],
            }),
        }
    }

    #[test]
    fn test_generate_code_contains_auto_generated_header() {
        let module = make_editor_module();
        let config = make_config();
        let code = generate_code(&module, &config);
        assert!(code.contains("此文件由 js-bridge-bindgen 自动生成"));
    }

    #[test]
    fn test_generate_code_contains_js_bridge_import() {
        let module = make_editor_module();
        let config = make_config();
        let code = generate_code(&module, &config);
        assert!(code.contains("use js_bridge_bindgen::JsBridge;"));
    }

    #[test]
    fn test_generate_code_editor_module() {
        let module = make_editor_module();
        let config = make_config();
        let code = generate_code(&module, &config);
        assert!(code.contains("pub mod editor {"), "应生成 editor 模块");
        assert!(
            code.contains("pub fn create<B: super::JsBridge>"),
            "应生成 create 函数"
        );
        assert!(
            code.contains("pub fn destroy<B: super::JsBridge>()"),
            "应生成 destroy 函数"
        );
        assert!(
            code.contains("pub async fn take_virtual_range<B: super::JsBridge>()"),
            "应生成 take_virtual_range 函数"
        );
    }

    #[test]
    fn test_generate_code_create_fn_uses_global_object() {
        let module = make_editor_module();
        let config = make_config();
        let code = generate_code(&module, &config);
        assert!(code.contains("let g = B::GLOBAL_OBJECT;"));
        assert!(code.contains("B::ensure_injected();"));
        assert!(code.contains("B::eval_js"));
    }

    #[test]
    fn test_generate_code_destroy_uses_concat() {
        let module = make_editor_module();
        let config = make_config();
        let code = generate_code(&module, &config);
        assert!(code.contains(".concat()"), "destroy 应使用字符串拼接");
        assert!(code.contains("__editor"), "destroy 应引用实例变量");
    }

    #[test]
    fn test_generate_code_empty_module() {
        let module = ParsedModule {
            factory_functions: vec![],
            interfaces: vec![],
            enums: vec![],
            type_aliases: vec![],
            global_decl: None,
        };
        let config = make_config();
        let code = generate_code(&module, &config);
        assert!(code.contains("此文件由 js-bridge-bindgen 自动生成"));
        assert!(!code.contains("pub mod"));
    }

    #[test]
    fn test_generate_code_enum() {
        let module = ParsedModule {
            factory_functions: vec![],
            interfaces: vec![],
            enums: vec![EnumInfo {
                name: "Direction".into(),
                members: vec![
                    EnumMember {
                        name: "Up".into(),
                        value: None,
                    },
                    EnumMember {
                        name: "Down".into(),
                        value: None,
                    },
                ],
            }],
            type_aliases: vec![],
            global_decl: None,
        };
        let config = make_config();
        let code = generate_code(&module, &config);
        assert!(code.contains("pub enum Direction {"));
        assert!(code.contains("Up,"));
        assert!(code.contains("Down,"));
    }

    #[test]
    fn test_generate_code_struct_with_properties() {
        let module = ParsedModule {
            factory_functions: vec![],
            interfaces: vec![InterfaceInfo {
                name: "EditorConfig".into(),
                methods: vec![],
                properties: vec![
                    InterfaceProperty {
                        name: "textareaId".into(),
                        ts_type: RustType::String,
                        optional: false,
                        readonly: false,
                        doc_comment: None,
                    },
                    InterfaceProperty {
                        name: "readOnly".into(),
                        ts_type: RustType::Bool,
                        optional: true,
                        readonly: false,
                        doc_comment: None,
                    },
                ],
                ..Default::default()
            }],
            enums: vec![],
            type_aliases: vec![],
            global_decl: None,
        };
        let config = make_config();
        let code = generate_code(&module, &config);
        assert!(code.contains("pub struct EditorConfig {"));
        assert!(code.contains("pub textarea_id: String,"));
        assert!(code.contains("pub read_only: Option<bool>,"));
        assert!(code.contains("serde::Serialize"));
    }

    #[test]
    fn test_generate_code_custom_instance_var() {
        let module = make_editor_module();
        let config = Config {
            instance_vars: Some(HashMap::from([(
                "createEditor".into(),
                "__myEditor".into(),
            )])),
            ..make_config()
        };
        let code = generate_code(&module, &config);
        assert!(code.contains("__myEditor"), "应使用自定义实例变量名");
    }

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("Editor"), "editor");
        assert_eq!(to_snake_case("PdfPreview"), "pdf_preview");
        assert_eq!(to_snake_case("takeVirtualRange"), "take_virtual_range");
    }

    #[test]
    fn test_to_camel_case_param() {
        assert_eq!(to_camel_case_param("textarea_id"), "textareaId");
        assert_eq!(to_camel_case_param("highlight_id"), "highlightId");
        assert_eq!(to_camel_case_param("simple"), "simple");
    }

    #[test]
    fn test_factory_func_to_module_name() {
        assert_eq!(factory_func_to_module_name("createEditor"), "editor");
        assert_eq!(
            factory_func_to_module_name("createPdfPreview"),
            "pdf_preview"
        );
    }

    #[test]
    fn test_generate_code_struct_param_create() {
        let module = ParsedModule {
            factory_functions: vec![FactoryFunction {
                doc_comment: None,
                name: "createEditor".into(),
                params: vec![ParamInfo {
                    name: "config".into(),
                    ts_type: RustType::Reference("EditorConfig".into()),
                    optional: false,
                }],
                return_type: RustType::Reference("Editor".into()),
                is_default_export: false,
                rust_lint_attrs: Vec::new(),
            }],
            interfaces: vec![
                InterfaceInfo {
                    name: "Editor".into(),
                    methods: vec![MethodSignature {
                        name: "destroy".into(),
                        params: vec![],
                        return_type: RustType::Void,
                        ..Default::default()
                    }],
                    properties: vec![],
                    ..Default::default()
                },
                InterfaceInfo {
                    name: "EditorConfig".into(),
                    methods: vec![],
                    properties: vec![InterfaceProperty {
                        name: "textareaId".into(),
                        ts_type: RustType::String,
                        optional: false,
                        readonly: false,
                        doc_comment: None,
                    }],
                    ..Default::default()
                },
            ],
            enums: vec![],
            type_aliases: vec![],
            global_decl: Some(GlobalDeclaration {
                factory_methods: vec![FactoryMethodDecl {
                    name: "__mtNote".into(),
                    methods: vec![MethodSignature {
                        name: "createEditor".into(),
                        params: vec![],
                        return_type: RustType::Reference("Editor".into()),
                        ..Default::default()
                    }],
                }],
                instance_vars: vec![InstanceVarDecl {
                    name: "__editor".into(),
                    type_name: "Editor".into(),
                    optional: true,
                }],
            }),
        };
        let config = make_config();
        let code = generate_code(&module, &config);
        assert!(
            code.contains("config_json = serde_json::to_string(&config).unwrap_or_default()"),
            "struct 参数应使用 JSON 序列化"
        );
        assert!(
            code.contains("JSON.parse('{config_json}')"),
            "JS 调用应使用 JSON.parse 解析 struct JSON 字符串"
        );
    }

    #[test]
    fn test_generate_code_method_with_params_and_return() {
        let module = ParsedModule {
            factory_functions: vec![FactoryFunction {
                doc_comment: None,
                name: "createEditor".into(),
                params: vec![],
                return_type: RustType::Reference("Editor".into()),
                is_default_export: false,
                rust_lint_attrs: Vec::new(),
            }],
            interfaces: vec![InterfaceInfo {
                name: "Editor".into(),
                methods: vec![
                    MethodSignature {
                        name: "setHighlight".into(),
                        params: vec![
                            ParamInfo {
                                name: "id".into(),
                                ts_type: RustType::String,
                                optional: false,
                            },
                            ParamInfo {
                                name: "range".into(),
                                ts_type: RustType::F64,
                                optional: false,
                            },
                        ],
                        return_type: RustType::Void,
                        ..Default::default()
                    },
                    MethodSignature {
                        name: "getValue".into(),
                        params: vec![],
                        return_type: RustType::String,
                        ..Default::default()
                    },
                    MethodSignature {
                        name: "destroy".into(),
                        params: vec![],
                        return_type: RustType::Void,
                        ..Default::default()
                    },
                ],
                properties: vec![],
                ..Default::default()
            }],
            enums: vec![],
            type_aliases: vec![],
            global_decl: Some(GlobalDeclaration {
                factory_methods: vec![FactoryMethodDecl {
                    name: "__mtNote".into(),
                    methods: vec![MethodSignature {
                        name: "createEditor".into(),
                        params: vec![],
                        return_type: RustType::Reference("Editor".into()),
                        ..Default::default()
                    }],
                }],
                instance_vars: vec![InstanceVarDecl {
                    name: "__editor".into(),
                    type_name: "Editor".into(),
                    optional: true,
                }],
            }),
        };
        let config = make_config();
        let code = generate_code(&module, &config);
        assert!(
            code.contains("pub fn set_highlight<B: super::JsBridge>(id: &str, range: f64)"),
            "带参数的方法应正确生成"
        );
        assert!(
            code.contains("pub async fn get_value<B: super::JsBridge>() -> String"),
            "带返回值的方法应正确生成"
        );
        assert!(
            code.contains("return {g}.__editor.getValue()"),
            "有返回值的方法应包含 return"
        );
    }

    #[test]
    fn test_generate_code_no_destroy_auto_added() {
        let module = ParsedModule {
            factory_functions: vec![FactoryFunction {
                doc_comment: None,
                name: "createEditor".into(),
                params: vec![],
                return_type: RustType::Reference("Editor".into()),
                is_default_export: false,
                rust_lint_attrs: Vec::new(),
            }],
            interfaces: vec![InterfaceInfo {
                name: "Editor".into(),
                methods: vec![MethodSignature {
                    name: "doStuff".into(),
                    params: vec![],
                    return_type: RustType::Void,
                    ..Default::default()
                }],
                properties: vec![],
                ..Default::default()
            }],
            enums: vec![],
            type_aliases: vec![],
            global_decl: Some(GlobalDeclaration {
                factory_methods: vec![FactoryMethodDecl {
                    name: "__mtNote".into(),
                    methods: vec![MethodSignature {
                        name: "createEditor".into(),
                        params: vec![],
                        return_type: RustType::Reference("Editor".into()),
                        ..Default::default()
                    }],
                }],
                instance_vars: vec![InstanceVarDecl {
                    name: "__editor".into(),
                    type_name: "Editor".into(),
                    optional: true,
                }],
            }),
        };
        let config = make_config();
        let code = generate_code(&module, &config);
        assert!(
            code.contains("pub fn destroy"),
            "接口没有 destroy 方法时应自动添加"
        );
    }

    #[test]
    fn test_to_camel_case_param_multi_segment() {
        assert_eq!(to_camel_case_param("viewport_left"), "viewportLeft");
        assert_eq!(to_camel_case_param("initial_scale"), "initialScale");
        assert_eq!(to_camel_case_param("a_b_c"), "aBC");
    }

    #[test]
    fn test_generate_code_method_with_struct_param() {
        let module = ParsedModule {
            factory_functions: vec![FactoryFunction {
                doc_comment: None,
                name: "createEditor".into(),
                params: vec![],
                return_type: RustType::Reference("Editor".into()),
                is_default_export: false,
                rust_lint_attrs: Vec::new(),
            }],
            interfaces: vec![
                InterfaceInfo {
                    name: "Editor".into(),
                    methods: vec![
                        MethodSignature {
                            name: "configure".into(),
                            params: vec![ParamInfo {
                                name: "opts".into(),
                                ts_type: RustType::Reference("Config".into()),
                                optional: false,
                            }],
                            return_type: RustType::Void,
                            ..Default::default()
                        },
                        MethodSignature {
                            name: "destroy".into(),
                            params: vec![],
                            return_type: RustType::Void,
                            ..Default::default()
                        },
                    ],
                    properties: vec![],
                    ..Default::default()
                },
                InterfaceInfo {
                    name: "Config".into(),
                    methods: vec![],
                    properties: vec![InterfaceProperty {
                        name: "key".into(),
                        ts_type: RustType::String,
                        optional: false,
                        readonly: false,
                        doc_comment: None,
                    }],
                    ..Default::default()
                },
            ],
            enums: vec![],
            type_aliases: vec![],
            global_decl: Some(GlobalDeclaration {
                factory_methods: vec![FactoryMethodDecl {
                    name: "__mtNote".into(),
                    methods: vec![MethodSignature {
                        name: "createEditor".into(),
                        params: vec![],
                        return_type: RustType::Reference("Editor".into()),
                        ..Default::default()
                    }],
                }],
                instance_vars: vec![InstanceVarDecl {
                    name: "__editor".into(),
                    type_name: "Editor".into(),
                    optional: true,
                }],
            }),
        };
        let config = make_config();
        let code = generate_code(&module, &config);
        assert!(
            code.contains("opts_json = serde_json::to_string(&opts).unwrap_or_default()"),
            "instance method 的 struct 参数应序列化"
        );
        assert!(
            code.contains("JSON.parse('{opts_json}')"),
            "instance method 的 JS 调用应使用 JSON.parse"
        );
    }

    #[test]
    fn test_generate_code_nested_object_method() {
        let module = ParsedModule {
            factory_functions: vec![FactoryFunction {
                doc_comment: None,
                name: "createPdfPreview".into(),
                params: vec![],
                return_type: RustType::Reference("PdfPreview".into()),
                is_default_export: false,
                rust_lint_attrs: Vec::new(),
            }],
            interfaces: vec![InterfaceInfo {
                doc_comment: None,
                name: "PdfPreview".into(),
                methods: vec![
                    MethodSignature {
                        doc_comment: None,
                        name: "setScale".into(),
                        params: vec![ParamInfo {
                            name: "scale".into(),
                            ts_type: RustType::F64,
                            optional: false,
                        }],
                        return_type: RustType::Void,
                        rust_lint_attrs: Vec::new(),
                        path: vec!["render".into()],
                    },
                    MethodSignature {
                        doc_comment: None,
                        name: "pageInfos".into(),
                        params: vec![],
                        return_type: RustType::String,
                        rust_lint_attrs: Vec::new(),
                        path: vec!["query".into()],
                    },
                    MethodSignature {
                        doc_comment: None,
                        name: "destroy".into(),
                        params: vec![],
                        return_type: RustType::Void,
                        rust_lint_attrs: Vec::new(),
                        path: vec![],
                    },
                ],
                properties: vec![],
                rust_lint_attrs: Vec::new(),
            }],
            enums: vec![],
            type_aliases: vec![],
            global_decl: Some(GlobalDeclaration {
                factory_methods: vec![FactoryMethodDecl {
                    name: "__mt_note_mobile".into(),
                    methods: vec![MethodSignature {
                        doc_comment: None,
                        name: "createPdfPreview".into(),
                        params: vec![],
                        return_type: RustType::Reference("PdfPreview".into()),
                        rust_lint_attrs: Vec::new(),
                        path: vec![],
                    }],
                }],
                instance_vars: vec![InstanceVarDecl {
                    name: "__pdfPreview".into(),
                    type_name: "PdfPreview".into(),
                    optional: true,
                }],
            }),
        };
        let config = Config {
            entry: PathBuf::from("test.ts"),
            out_dir: PathBuf::from("/tmp"),
            global_factory_name: "__mt_note_mobile".into(),
            instance_vars: Some(HashMap::from([(
                "createPdfPreview".into(),
                "__pdfPreview".into(),
            )])),
            ..Default::default()
        };
        let code = generate_code(&module, &config);
        assert!(
            code.contains("pub fn render_set_scale"),
            "嵌套方法应生成 render_set_scale 函数: {code}"
        );
        assert!(
            code.contains("__pdfPreview.render.setScale"),
            "JS 路径应为 __pdfPreview.render.setScale: {code}"
        );
        assert!(
            code.contains("pub async fn query_page_infos"),
            "query 嵌套方法应生成 query_page_infos"
        );
        assert!(
            code.contains("__pdfPreview.query.pageInfos"),
            "JS 路径应为 __pdfPreview.query.pageInfos: {code}"
        );
    }

    // ========== doc_comment 输出测试 ==========

    #[test]
    fn test_generate_code_doc_comment_on_factory_function() {
        let module = ParsedModule {
            factory_functions: vec![FactoryFunction {
                doc_comment: Some("创建编辑器实例。".into()),
                name: "createEditor".into(),
                params: vec![],
                return_type: RustType::Reference("Editor".into()),
                is_default_export: false,
                rust_lint_attrs: Vec::new(),
            }],
            interfaces: vec![InterfaceInfo {
                doc_comment: None,
                name: "Editor".into(),
                methods: vec![MethodSignature {
                    doc_comment: None,
                    name: "destroy".into(),
                    params: vec![],
                    return_type: RustType::Void,
                    ..Default::default()
                }],
                ..Default::default()
            }],
            enums: vec![],
            type_aliases: vec![],
            global_decl: Some(GlobalDeclaration {
                factory_methods: vec![FactoryMethodDecl {
                    name: "__mtNote".into(),
                    methods: vec![MethodSignature {
                        name: "createEditor".into(),
                        params: vec![],
                        return_type: RustType::Void,
                        ..Default::default()
                    }],
                }],
                instance_vars: vec![],
            }),
        };
        let config = make_config();
        let code = generate_code(&module, &config);
        assert!(
            code.contains("/// 创建编辑器实例。"),
            "应包含 doc comment: {code}"
        );
    }

    #[test]
    fn test_generate_code_doc_comment_on_method() {
        let module = ParsedModule {
            factory_functions: vec![FactoryFunction {
                doc_comment: None,
                name: "createEditor".into(),
                params: vec![],
                return_type: RustType::Reference("Editor".into()),
                is_default_export: false,
                rust_lint_attrs: Vec::new(),
            }],
            interfaces: vec![InterfaceInfo {
                doc_comment: None,
                name: "Editor".into(),
                methods: vec![
                    MethodSignature {
                        doc_comment: Some("获取滚动位置。".into()),
                        name: "getScrollInfo".into(),
                        params: vec![],
                        return_type: RustType::String,
                        ..Default::default()
                    },
                    MethodSignature {
                        doc_comment: None,
                        name: "destroy".into(),
                        params: vec![],
                        return_type: RustType::Void,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            enums: vec![],
            type_aliases: vec![],
            global_decl: Some(GlobalDeclaration {
                factory_methods: vec![FactoryMethodDecl {
                    name: "__mtNote".into(),
                    methods: vec![MethodSignature {
                        name: "createEditor".into(),
                        params: vec![],
                        return_type: RustType::Void,
                        ..Default::default()
                    }],
                }],
                instance_vars: vec![],
            }),
        };
        let config = make_config();
        let code = generate_code(&module, &config);
        assert!(
            code.contains("/// 获取滚动位置。"),
            "应包含 method doc comment: {code}"
        );
    }

    #[test]
    fn test_generate_code_doc_comment_on_struct() {
        let module = ParsedModule {
            factory_functions: vec![FactoryFunction {
                doc_comment: None,
                name: "createEditor".into(),
                params: vec![],
                return_type: RustType::Reference("Editor".into()),
                is_default_export: false,
                rust_lint_attrs: Vec::new(),
            }],
            interfaces: vec![
                InterfaceInfo {
                    doc_comment: Some("编辑器配置。".into()),
                    name: "EditorConfig".into(),
                    methods: vec![],
                    properties: vec![InterfaceProperty {
                        name: "textareaId".into(),
                        ts_type: RustType::String,
                        optional: false,
                        readonly: false,
                        doc_comment: None,
                    }],
                    ..Default::default()
                },
                InterfaceInfo {
                    name: "Editor".into(),
                    methods: vec![MethodSignature {
                        doc_comment: None,
                        name: "destroy".into(),
                        params: vec![],
                        return_type: RustType::Void,
                        ..Default::default()
                    }],
                    ..Default::default()
                },
            ],
            enums: vec![],
            type_aliases: vec![],
            global_decl: Some(GlobalDeclaration {
                factory_methods: vec![FactoryMethodDecl {
                    name: "__mtNote".into(),
                    methods: vec![MethodSignature {
                        name: "createEditor".into(),
                        params: vec![],
                        return_type: RustType::Void,
                        ..Default::default()
                    }],
                }],
                instance_vars: vec![],
            }),
        };
        let config = make_config();
        let code = generate_code(&module, &config);
        assert!(
            code.contains("/// 编辑器配置。"),
            "应包含 struct doc comment: {code}"
        );
    }

    #[test]
    fn test_generate_code_doc_comment_on_property() {
        let module = ParsedModule {
            factory_functions: vec![FactoryFunction {
                doc_comment: None,
                name: "createEditor".into(),
                params: vec![],
                return_type: RustType::Reference("Editor".into()),
                is_default_export: false,
                rust_lint_attrs: Vec::new(),
            }],
            interfaces: vec![
                InterfaceInfo {
                    doc_comment: None,
                    name: "EditorConfig".into(),
                    methods: vec![],
                    properties: vec![InterfaceProperty {
                        name: "textareaId".into(),
                        ts_type: RustType::String,
                        optional: false,
                        readonly: false,
                        doc_comment: Some("文本域元素 ID。".into()),
                    }],
                    ..Default::default()
                },
                InterfaceInfo {
                    name: "Editor".into(),
                    methods: vec![MethodSignature {
                        doc_comment: None,
                        name: "destroy".into(),
                        params: vec![],
                        return_type: RustType::Void,
                        ..Default::default()
                    }],
                    ..Default::default()
                },
            ],
            enums: vec![],
            type_aliases: vec![],
            global_decl: Some(GlobalDeclaration {
                factory_methods: vec![FactoryMethodDecl {
                    name: "__mtNote".into(),
                    methods: vec![MethodSignature {
                        name: "createEditor".into(),
                        params: vec![],
                        return_type: RustType::Void,
                        ..Default::default()
                    }],
                }],
                instance_vars: vec![],
            }),
        };
        let config = make_config();
        let code = generate_code(&module, &config);
        assert!(
            code.contains("/// 文本域元素 ID。"),
            "应包含 property doc comment: {code}"
        );
    }

    #[test]
    fn test_generate_code_doc_comment_multiline() {
        let module = ParsedModule {
            factory_functions: vec![FactoryFunction {
                doc_comment: Some("第一行\n第二行".into()),
                name: "createEditor".into(),
                params: vec![],
                return_type: RustType::Reference("Editor".into()),
                is_default_export: false,
                rust_lint_attrs: Vec::new(),
            }],
            interfaces: vec![InterfaceInfo {
                doc_comment: None,
                name: "Editor".into(),
                methods: vec![MethodSignature {
                    doc_comment: None,
                    name: "destroy".into(),
                    params: vec![],
                    return_type: RustType::Void,
                    ..Default::default()
                }],
                ..Default::default()
            }],
            enums: vec![],
            type_aliases: vec![],
            global_decl: Some(GlobalDeclaration {
                factory_methods: vec![FactoryMethodDecl {
                    name: "__mtNote".into(),
                    methods: vec![MethodSignature {
                        name: "createEditor".into(),
                        params: vec![],
                        return_type: RustType::Void,
                        ..Default::default()
                    }],
                }],
                instance_vars: vec![],
            }),
        };
        let config = make_config();
        let code = generate_code(&module, &config);
        assert!(
            code.contains("/// 第一行") && code.contains("/// 第二行"),
            "应包含多行 doc comment: {code}"
        );
    }
}