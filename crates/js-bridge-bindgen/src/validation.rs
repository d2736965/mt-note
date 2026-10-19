use crate::config::Config;
use crate::error::ValidationResult;
use crate::types::*;

/// 校验规则编号
mod rules {
    pub const MISSING_DECLARE_GLOBAL: &str = "E001";
    pub const MISSING_FACTORY_OBJECT: &str = "E003";
    pub const FACTORY_PROPERTY_NOT_FUNCTION: &str = "E004";
    pub const MISSING_EXPORT_FUNCTION: &str = "E005";
    pub const MISSING_INTERFACE: &str = "E006";
    pub const INSTANCE_TYPE_MISMATCH: &str = "E007";
    pub const MISSING_RETURN_TYPE: &str = "E008";
    pub const NON_SERIALIZABLE_RETURN: &str = "E009";
    pub const CANNOT_ASSOCIATE_INSTANCE: &str = "W001";
}

/// 校验 TS 模块是否符合工厂模式要求
///
/// 工厂模式要求：
/// 1. 必须有 `declare global` 块扩展 `Window` 接口
/// 2. `Window` 中必须存在 `global_factory_name` 指定的工厂对象
/// 3. 工厂对象的每个属性必须是函数类型
/// 4. 每个工厂函数必须有对应的 `export function`
/// 5. 工厂函数返回值类型必须有对应的 `export interface`
/// 6. `Window` 中的实例变量类型必须与对应工厂函数返回类型一致
/// 7. 实例方法必须有返回类型标注
/// 8. 返回类型必须是可序列化的
pub fn validate(module: &ParsedModule, config: &Config) -> ValidationResult {
    let mut result = ValidationResult::default();

    validate_global_declaration(module, config, &mut result);
    validate_factory_functions(module, config, &mut result);
    validate_interfaces(module, config, &mut result);
    validate_instance_vars(module, config, &mut result);

    result
}

/// 校验规则 1-3：declare global + Window + 工厂对象
fn validate_global_declaration(
    module: &ParsedModule,
    config: &Config,
    result: &mut ValidationResult,
) {
    if let Some(global_decl) = &module.global_decl {
        let factory_name = &config.global_factory_name;

        let factory_method = global_decl
            .factory_methods
            .iter()
            .find(|m| m.name == *factory_name);

        if let Some(factory) = factory_method {
            if factory.methods.is_empty() {
                result.add_error_with_hint(
                    rules::FACTORY_PROPERTY_NOT_FUNCTION,
                    format!("工厂对象 `Window.{factory_name}` 没有声明任何方法"),
                    format!("在 `{factory_name}` 对象中添加 `createXxx` 工厂方法"),
                );
            }
        } else {
            let instance_var = global_decl
                .instance_vars
                .iter()
                .find(|v| v.name == *factory_name);

            if instance_var.is_some() {
                result.add_error_with_hint(
                    rules::FACTORY_PROPERTY_NOT_FUNCTION,
                    format!("`Window.{factory_name}` 不是函数类型，而是实例变量"),
                    format!("`{factory_name}` 应该是工厂对象（包含 createXxx 方法），不是实例变量"),
                );
            } else {
                result.add_error_with_hint(
                    rules::MISSING_FACTORY_OBJECT,
                    format!("`Window` 接口中未声明工厂对象 `{factory_name}`"),
                    format!(
                        "在 `declare global {{ interface Window {{ {factory_name}: {{ ... }} }} }}` 中声明工厂对象"
                    ),
                );
            }
        }
    } else {
        result.add_error_with_hint(
            rules::MISSING_DECLARE_GLOBAL,
            "缺少 `declare global` 块",
            "添加 `declare global { interface Window { ... } }` 块声明全局变量",
        );
    }
}

/// 校验规则 4-5：工厂函数必须有 export function 和对应 interface
fn validate_factory_functions(
    module: &ParsedModule,
    config: &Config,
    result: &mut ValidationResult,
) {
    if let Some(global_decl) = &module.global_decl {
        let factory_name = &config.global_factory_name;

        if let Some(factory) = global_decl
            .factory_methods
            .iter()
            .find(|m| m.name == *factory_name)
        {
            for method in &factory.methods {
                validate_single_factory_method(module, &method.name, result);
            }
        }
    }
}

fn validate_single_factory_method(
    module: &ParsedModule,
    factory_func_name: &str,
    result: &mut ValidationResult,
) {
    let exported_func = module
        .factory_functions
        .iter()
        .find(|f| f.name == *factory_func_name);

    if let Some(func) = exported_func {
        if let RustType::Reference(type_name) = &func.return_type {
            let has_interface = module.interfaces.iter().any(|i| i.name == *type_name);
            if !has_interface {
                result.add_error_with_hint(
                    rules::MISSING_INTERFACE,
                    format!("工厂函数 `{factory_func_name}` 返回类型 `{type_name}` 没有对应的 `export interface`"),
                    format!("添加 `export interface {type_name} {{ ... }}`"),
                );
            }
        } else if let RustType::Unknown = &func.return_type {
            result.add_error_with_hint(
                rules::MISSING_RETURN_TYPE,
                format!("工厂函数 `{factory_func_name}` 缺少返回类型标注"),
                "所有工厂函数必须有显式返回类型（如 `: Editor`）",
            );
        }

        validate_method_return_types(factory_func_name, &func.return_type, result);
    } else {
        result.add_error_with_hint(
            rules::MISSING_EXPORT_FUNCTION,
            format!("工厂方法 `{factory_func_name}` 在 `declare global` 中声明，但没有对应的 `export function`"),
            format!("添加 `export function {factory_func_name}(...): Xxx {{ ... }}`"),
        );
    }
}

/// 校验接口方法签名
fn validate_interfaces(module: &ParsedModule, _config: &Config, result: &mut ValidationResult) {
    for iface in &module.interfaces {
        for method in &iface.methods {
            if method.name == "(call)" {
                continue;
            }

            if matches!(method.return_type, RustType::Unknown) {
                result.add_error_with_hint(
                    rules::MISSING_RETURN_TYPE,
                    format!(
                        "接口 `{}` 的方法 `{}` 缺少返回类型标注",
                        iface.name, method.name
                    ),
                    "所有实例方法必须有显式返回类型",
                );
            }

            validate_method_return_types(
                &format!("{}.{}", iface.name, method.name),
                &method.return_type,
                result,
            );
        }
    }
}

/// 校验规则 8：返回类型必须可序列化
fn validate_method_return_types(
    method_path: &str,
    return_type: &RustType,
    result: &mut ValidationResult,
) {
    if !return_type.is_serializable() && !matches!(return_type, RustType::Unknown) {
        result.add_error_with_hint(
            rules::NON_SERIALIZABLE_RETURN,
            format!("方法 `{method_path}` 的返回类型 `{return_type}` 无法跨 JS-Rust 边界序列化"),
            "返回类型必须是 void、string、number、boolean、Promise<T> 或可序列化结构体",
        );
    }

    if let RustType::Promise(inner) = return_type
        && !inner.is_serializable()
    {
        result.add_error_with_hint(
            rules::NON_SERIALIZABLE_RETURN,
            format!("方法 `{method_path}` 的 Promise 内部类型 `{inner}` 无法序列化"),
            "Promise 内部类型必须是可序列化的",
        );
    }
}

/// 校验规则 6, 9：实例变量类型与工厂函数返回类型一致性
fn validate_instance_vars(module: &ParsedModule, config: &Config, result: &mut ValidationResult) {
    if let Some(global_decl) = &module.global_decl {
        for instance_var in &global_decl.instance_vars {
            let var_name = &instance_var.name;
            let var_type = &instance_var.type_name;

            let factory_func = module.factory_functions.iter().find(|f| {
                let expected_var = config.resolve_instance_var(&f.name);
                expected_var == *var_name
            });

            if let Some(func) = factory_func {
                if let RustType::Reference(return_type_name) = &func.return_type
                    && return_type_name != var_type
                {
                    result.add_error_with_hint(
                        rules::INSTANCE_TYPE_MISMATCH,
                        format!(
                            "实例变量 `{var_name}` 类型为 `{var_type}`，但工厂函数 `{}` 返回 `{return_type_name}`",
                            func.name
                        ),
                        "实例变量类型必须与对应工厂函数返回类型一致",
                    );
                }
            } else {
                result.add_warning(
                    rules::CANNOT_ASSOCIATE_INSTANCE,
                    format!(
                        "实例变量 `{var_name}` 无法自动关联到任何工厂函数（期望 `createXxx` → `{var_name}`）"
                    ),
                );
            }
        }
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

    fn make_valid_module() -> ParsedModule {
        ParsedModule {
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
        }
    }

    #[test]
    fn test_validate_valid_module() {
        let module = make_valid_module();
        let config = make_config();
        let result = validate(&module, &config);
        assert!(result.is_valid(), "合法模块不应有校验错误");
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_validate_missing_declare_global() {
        let mut module = make_valid_module();
        module.global_decl = None;
        let config = make_config();
        let result = validate(&module, &config);
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| e.rule == "E001"));
    }

    #[test]
    fn test_validate_missing_factory_object() {
        let mut module = make_valid_module();
        module.global_decl = Some(GlobalDeclaration {
            factory_methods: vec![],
            instance_vars: vec![],
        });
        let config = make_config();
        let result = validate(&module, &config);
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| e.rule == "E003"));
    }

    #[test]
    fn test_validate_missing_export_function() {
        let mut module = make_valid_module();
        module.factory_functions.clear();
        let config = make_config();
        let result = validate(&module, &config);
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| e.rule == "E005"));
    }

    #[test]
    fn test_validate_missing_interface() {
        let mut module = make_valid_module();
        module.interfaces.clear();
        let config = make_config();
        let result = validate(&module, &config);
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| e.rule == "E006"));
    }

    #[test]
    fn test_validate_instance_type_mismatch() {
        let mut module = make_valid_module();
        if let Some(ref mut global) = module.global_decl {
            global.instance_vars[0].type_name = "WrongType".into();
        }
        let config = make_config();
        let result = validate(&module, &config);
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| e.rule == "E007"));
    }

    #[test]
    fn test_validate_missing_return_type() {
        let mut module = make_valid_module();
        module.factory_functions[0].return_type = RustType::Unknown;
        let config = make_config();
        let result = validate(&module, &config);
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| e.rule == "E008"));
    }

    #[test]
    fn test_validate_non_serializable_return() {
        let mut module = make_valid_module();
        module.interfaces[0].methods[0].return_type = RustType::Never;
        let config = make_config();
        let result = validate(&module, &config);
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| e.rule == "E009"));
    }

    #[test]
    fn test_validate_cannot_associate_instance_warning() {
        let mut module = make_valid_module();
        if let Some(ref mut global) = module.global_decl {
            global.instance_vars.push(InstanceVarDecl {
                name: "__orphan".into(),
                type_name: "Something".into(),
                optional: false,
            });
        }
        let config = make_config();
        let result = validate(&module, &config);
        assert!(result.warnings.iter().any(|w| w.rule == "W001"));
    }

    #[test]
    fn test_validate_factory_is_instance_var() {
        let mut module = make_valid_module();
        module.global_decl = Some(GlobalDeclaration {
            factory_methods: vec![],
            instance_vars: vec![InstanceVarDecl {
                name: "__mtNote".into(),
                type_name: "Something".into(),
                optional: false,
            }],
        });
        let config = make_config();
        let result = validate(&module, &config);
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| e.rule == "E004"));
    }

    #[test]
    fn test_validate_explicit_instance_var_mapping() {
        let mut module = make_valid_module();
        if let Some(ref mut global) = module.global_decl {
            global.instance_vars[0].name = "__myEditor".into();
        }
        let config = Config {
            instance_vars: Some(HashMap::from([(
                "createEditor".into(),
                "__myEditor".into(),
            )])),
            ..make_config()
        };
        let result = validate(&module, &config);
        assert!(result.is_valid());
    }

    #[test]
    fn test_validate_factory_object_empty_methods() {
        let mut module = make_valid_module();
        module.global_decl = Some(GlobalDeclaration {
            factory_methods: vec![FactoryMethodDecl {
                name: "__mtNote".into(),
                methods: vec![],
            }],
            instance_vars: vec![],
        });
        let config = make_config();
        let result = validate(&module, &config);
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| e.rule == "E004"));
    }

    #[test]
    fn test_validate_interface_method_unknown_return() {
        let mut module = make_valid_module();
        module.interfaces[0].methods[0].return_type = RustType::Unknown;
        let config = make_config();
        let result = validate(&module, &config);
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| e.rule == "E008"));
    }

    #[test]
    fn test_validate_promise_inner_not_serializable() {
        let mut module = make_valid_module();
        module.interfaces[0].methods[0].return_type =
            RustType::Promise(Box::new(RustType::Unknown));
        let config = make_config();
        let result = validate(&module, &config);
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| e.rule == "E009"));
    }

    #[test]
    fn test_validate_call_signature_skipped() {
        let mut module = make_valid_module();
        module.interfaces[0].methods.push(MethodSignature {
            name: "(call)".into(),
            params: vec![],
            return_type: RustType::Unknown,
            ..Default::default()
        });
        let config = make_config();
        let result = validate(&module, &config);
        let call_errors: Vec<_> = result
            .errors
            .iter()
            .filter(|e| e.message.contains("(call)"))
            .collect();
        assert!(call_errors.is_empty(), "call signature 不应触发校验错误");
    }
}
