/// 从 TS 源码提取的完整模块信息
#[derive(Debug, Clone)]
pub struct ParsedModule {
    /// 导出的工厂函数（`export function createXxx(...)`）
    pub factory_functions: Vec<FactoryFunction>,
    /// 导出的接口（`export interface Xxx { ... }`）
    pub interfaces: Vec<InterfaceInfo>,
    /// 导出的枚举（`export enum Xxx { ... }`）
    pub enums: Vec<EnumInfo>,
    /// 导出的类型别名（`export type Xxx = ...`）
    pub type_aliases: Vec<TypeAliasInfo>,
    /// `declare global` 块中的 Window 扩展信息
    pub global_decl: Option<GlobalDeclaration>,
}

/// Rust lint 属性，由 TS 中的 `@rust-lint` 注释生成
///
/// 语法：`@rust-lint expect(dead_code, reason = "由 Rust 侧调用")`
///
/// `content` 是完整的 Rust 属性内容（不含 `#[` 和 `]`），
/// 生成代码时直接拼接为 `#[{content}]`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustLintAttr {
    /// 完整的 Rust 属性内容，如 `expect(dead_code, reason = "由 Rust 侧调用")`
    pub content: String,
}

/// 工厂函数信息
///
/// 对应 TS 中的 `export function createEditor(config: EditorConfig): Editor`
#[derive(Debug, Clone)]
pub struct FactoryFunction {
    /// 函数名（如 `"createEditor"`）
    pub name: String,
    /// 参数列表
    pub params: Vec<ParamInfo>,
    /// 返回类型
    pub return_type: RustType,
    /// 是否为 `export default`
    pub is_default_export: bool,
    /// TS 源码中的 JSDoc 描述文本（不含 `@` 标签），`None` 表示无注释。
    /// 生成 Rust 代码时转为 `///` 文档注释。
    pub doc_comment: Option<String>,
    /// `@rust-lint` 注解生成的 Rust lint 属性列表
    pub rust_lint_attrs: Vec<RustLintAttr>,
}

/// 接口信息
///
/// 对应 TS 中的 `export interface Editor { takeVirtualRange(): string; destroy(): void }`
#[derive(Debug, Clone, Default)]
pub struct InterfaceInfo {
    /// 接口名（如 `"Editor"`）
    pub name: String,
    /// 方法签名列表
    pub methods: Vec<MethodSignature>,
    /// 属性列表
    pub properties: Vec<InterfaceProperty>,
    /// TS 源码中的 JSDoc 描述文本（不含 `@` 标签），`None` 表示无注释。
    /// 生成 Rust 代码时转为 `///` 文档注释。
    pub doc_comment: Option<String>,
    /// `@rust-lint` 注解生成的 Rust lint 属性列表
    pub rust_lint_attrs: Vec<RustLintAttr>,
}

/// 接口方法签名
#[derive(Debug, Clone)]
pub struct MethodSignature {
    /// 方法名
    pub name: String,
    /// 参数列表
    pub params: Vec<ParamInfo>,
    /// 返回类型
    pub return_type: RustType,
    /// TS 源码中的 JSDoc 描述文本（不含 `@` 标签），`None` 表示无注释。
    pub doc_comment: Option<String>,
    /// 嵌套对象路径前缀。空表示方法直接挂在实例上（`instance.method`）；
    /// 非空如 `["render"]` 表示方法位于嵌套子对象（`instance.render.method`）。
    /// 由 interface 内「对象字面量属性」递归提取而来。
    pub path: Vec<String>,
    /// `@rust-lint` 注解生成的 Rust lint 属性列表
    pub rust_lint_attrs: Vec<RustLintAttr>,
}

impl Default for MethodSignature {
    fn default() -> Self {
        Self {
            name: String::new(),
            params: Vec::new(),
            return_type: RustType::Unknown,
            doc_comment: None,
            path: Vec::new(),
            rust_lint_attrs: Vec::new(),
        }
    }
}

/// 接口属性
#[derive(Debug, Clone)]
pub struct InterfaceProperty {
    /// 属性名
    pub name: String,
    /// 属性类型
    pub ts_type: RustType,
    /// 是否可选（`?`）
    pub optional: bool,
    /// 是否只读（`readonly`）
    pub readonly: bool,
    /// TS 源码中的 JSDoc 描述文本（不含 `@` 标签），`None` 表示无注释。
    /// 生成 Rust 代码时转为 `///` 文档注释。
    pub doc_comment: Option<String>,
}

/// 参数信息
#[derive(Debug, Clone)]
pub struct ParamInfo {
    /// 参数名
    pub name: String,
    /// 参数类型
    pub ts_type: RustType,
    /// 是否可选
    pub optional: bool,
}

/// 枚举信息
#[derive(Debug, Clone)]
pub struct EnumInfo {
    /// 枚举名
    pub name: String,
    /// 枚举成员
    pub members: Vec<EnumMember>,
}

/// 枚举成员
#[derive(Debug, Clone)]
pub struct EnumMember {
    /// 成员名
    pub name: String,
    /// 成员值（可能没有显式赋值）
    pub value: Option<String>,
}

/// 类型别名信息
#[derive(Debug, Clone)]
pub struct TypeAliasInfo {
    /// 别名名
    pub name: String,
}

/// `declare global` 块中的 Window 扩展信息
#[derive(Debug, Clone)]
pub struct GlobalDeclaration {
    /// 工厂对象上的方法声明（如 `__mtNote.createEditor: (config) => Editor`）
    pub factory_methods: Vec<FactoryMethodDecl>,
    /// Window 上的实例变量声明（如 `__editor?: Editor`）
    pub instance_vars: Vec<InstanceVarDecl>,
}

/// 工厂方法声明
#[derive(Debug, Clone)]
pub struct FactoryMethodDecl {
    /// 方法名（如 `"createEditor"`）
    pub name: String,
    /// 方法签名列表（工厂对象可能是内联对象类型，包含多个方法）
    pub methods: Vec<MethodSignature>,
}

/// 实例变量声明
#[derive(Debug, Clone)]
pub struct InstanceVarDecl {
    /// 变量名（如 `"__editor"`）
    pub name: String,
    /// 类型名（如 `"Editor"`）
    pub type_name: String,
    /// 是否可选（`?`）
    pub optional: bool,
}

/// Rust 类型表示
///
/// TS 类型到 Rust 类型的映射，用于代码生成。
#[derive(Debug, Clone, PartialEq, Default)]
pub enum RustType {
    #[default]
    /// 未知类型 → `serde_json::Value`
    Unknown,
    /// `void` → `()`
    Void,
    /// `string` → `String`
    String,
    /// `number` → `f64`
    F64,
    /// `boolean` → `bool`
    Bool,
    /// `null` → `Option<T>` 的一部分
    Null,
    /// `undefined` → `Option<T>` 的一部分
    Undefined,
    /// `never` → `!`
    Never,
    /// `Promise<T>` → 异步返回
    Promise(Box<RustType>),
    /// `Array<T>` → `Vec<T>`
    Vec(Box<RustType>),
    /// 联合类型 → 需要特殊处理
    Union(Vec<RustType>),
    /// 元组类型
    Tuple(Vec<RustType>),
    /// 类型引用（用户自定义类型）
    Reference(String),
    /// 字符串字面量类型
    StringLiteral(String),
    /// 数字字面量类型
    NumberLiteral(f64),
    /// 布尔字面量类型
    BoolLiteral(bool),
}

impl RustType {
    /// 转为 Rust 类型字符串
    pub fn to_rust_type_string(&self) -> String {
        match self {
            RustType::Void => "()".to_string(),
            RustType::String => "String".to_string(),
            RustType::F64 => "f64".to_string(),
            RustType::Bool => "bool".to_string(),
            RustType::Null | RustType::Undefined => "()".to_string(),
            RustType::Never => "!".to_string(),
            RustType::Unknown => "serde_json::Value".to_string(),
            RustType::Promise(inner) => {
                if inner.is_void() {
                    "()".to_string()
                } else {
                    inner.to_rust_type_string()
                }
            }
            RustType::Vec(inner) => {
                format!("Vec<{}>", inner.to_rust_type_string())
            }
            RustType::Union(variants) => {
                if self.is_string_or_promise_string() {
                    "String".to_string()
                } else if variants.len() == 2
                    && variants.iter().any(|v| matches!(v, RustType::Null))
                {
                    let non_null = variants.iter().find(|v| !matches!(v, RustType::Null));
                    if let Some(inner) = non_null {
                        format!("Option<{}>", inner.to_rust_type_string())
                    } else {
                        "serde_json::Value".to_string()
                    }
                } else {
                    "serde_json::Value".to_string()
                }
            }
            RustType::Tuple(elements) => {
                let parts: Vec<String> = elements.iter().map(|e| e.to_rust_type_string()).collect();
                format!("({})", parts.join(", "))
            }
            RustType::Reference(name) => name.clone(),
            RustType::StringLiteral(s) => format!("&str /* \"{s}\" */"),
            RustType::NumberLiteral(n) => format!("f64 /* {n} */"),
            RustType::BoolLiteral(b) => format!("bool /* {b} */"),
        }
    }

    /// 是否为 void 类型
    pub fn is_void(&self) -> bool {
        matches!(self, RustType::Void)
    }

    /// 是否为 Promise 类型（异步返回）
    pub fn is_promise(&self) -> bool {
        matches!(self, RustType::Promise(_))
    }

    /// 是否为 Promise<void>（异步但无返回值，fire-and-forget）
    pub fn is_promise_void(&self) -> bool {
        matches!(self, RustType::Promise(inner) if inner.is_void())
    }

    /// 是否为需要异步获取返回值的类型（非 void 返回值的方法）
    pub fn needs_async_return(&self) -> bool {
        !self.is_void() && !self.is_promise_void()
    }

    /// 是否为可序列化类型（可以跨 JS-Rust 边界传递）
    pub fn is_serializable(&self) -> bool {
        match self {
            RustType::Void
            | RustType::String
            | RustType::F64
            | RustType::Bool
            | RustType::Null
            | RustType::Undefined => true,
            RustType::Promise(inner) => inner.is_serializable(),
            RustType::Vec(inner) => inner.is_serializable(),
            RustType::Reference(_) => true,
            RustType::Union(variants) => variants.iter().all(|v| v.is_serializable()),
            RustType::Tuple(elements) => elements.iter().all(|e| e.is_serializable()),
            RustType::StringLiteral(_) | RustType::NumberLiteral(_) | RustType::BoolLiteral(_) => {
                true
            }
            RustType::Never | RustType::Unknown => false,
        }
    }

    /// 获取 Promise 内部类型
    pub fn promise_inner(&self) -> Option<&RustType> {
        match self {
            RustType::Promise(inner) => Some(inner),
            _ => None,
        }
    }

    /// 检查是否为 `string | Promise<string>` 模式
    pub fn is_string_or_promise_string(&self) -> bool {
        match self {
            RustType::Union(variants) => {
                let has_string = variants.contains(&RustType::String);
                let has_promise_string = variants
                    .iter()
                    .any(|v| matches!(v, RustType::Promise(inner) if **inner == RustType::String));
                variants.len() == 2 && has_string && has_promise_string
            }
            _ => false,
        }
    }
}

impl std::fmt::Display for RustType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_rust_type_string())
    }
}

impl Eq for RustType {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_rust_type_string() {
        assert_eq!(RustType::Void.to_rust_type_string(), "()");
        assert_eq!(RustType::String.to_rust_type_string(), "String");
        assert_eq!(RustType::F64.to_rust_type_string(), "f64");
        assert_eq!(RustType::Bool.to_rust_type_string(), "bool");
        assert_eq!(RustType::Unknown.to_rust_type_string(), "serde_json::Value");
        assert_eq!(
            RustType::Reference("Editor".into()).to_rust_type_string(),
            "Editor"
        );
        assert_eq!(
            RustType::Promise(Box::new(RustType::String)).to_rust_type_string(),
            "String"
        );
        assert_eq!(
            RustType::Vec(Box::new(RustType::F64)).to_rust_type_string(),
            "Vec<f64>"
        );
    }

    #[test]
    fn test_is_void() {
        assert!(RustType::Void.is_void());
        assert!(!RustType::String.is_void());
    }

    #[test]
    fn test_is_promise() {
        assert!(RustType::Promise(Box::new(RustType::String)).is_promise());
        assert!(!RustType::String.is_promise());
    }

    #[test]
    fn test_is_serializable() {
        assert!(RustType::Void.is_serializable());
        assert!(RustType::String.is_serializable());
        assert!(RustType::F64.is_serializable());
        assert!(RustType::Bool.is_serializable());
        assert!(RustType::Null.is_serializable());
        assert!(RustType::Undefined.is_serializable());
        assert!(RustType::Reference("Editor".into()).is_serializable());
        assert!(RustType::Promise(Box::new(RustType::String)).is_serializable());
        assert!(RustType::Vec(Box::new(RustType::F64)).is_serializable());
        assert!(RustType::StringLiteral("foo".into()).is_serializable());
        assert!(RustType::NumberLiteral(42.0).is_serializable());
        assert!(RustType::BoolLiteral(true).is_serializable());
        assert!(!RustType::Never.is_serializable());
        assert!(!RustType::Unknown.is_serializable());
        assert!(!RustType::Promise(Box::new(RustType::Unknown)).is_serializable());
        assert!(!RustType::Vec(Box::new(RustType::Unknown)).is_serializable());
    }

    #[test]
    fn test_promise_inner() {
        let promise = RustType::Promise(Box::new(RustType::String));
        assert_eq!(promise.promise_inner(), Some(&RustType::String));
        assert!(RustType::String.promise_inner().is_none());
    }

    #[test]
    fn test_is_string_or_promise_string() {
        let union = RustType::Union(vec![
            RustType::String,
            RustType::Promise(Box::new(RustType::String)),
        ]);
        assert!(union.is_string_or_promise_string());

        let wrong_inner = RustType::Union(vec![
            RustType::String,
            RustType::Promise(Box::new(RustType::F64)),
        ]);
        assert!(!wrong_inner.is_string_or_promise_string());

        assert!(!RustType::String.is_string_or_promise_string());
    }

    #[test]
    fn test_to_rust_type_string_advanced() {
        assert_eq!(RustType::Null.to_rust_type_string(), "()");
        assert_eq!(RustType::Undefined.to_rust_type_string(), "()");
        assert_eq!(RustType::Never.to_rust_type_string(), "!");

        assert_eq!(
            RustType::Tuple(vec![RustType::String, RustType::F64]).to_rust_type_string(),
            "(String, f64)"
        );
        assert_eq!(
            RustType::StringLiteral("hello".into()).to_rust_type_string(),
            "&str /* \"hello\" */"
        );
        assert_eq!(
            RustType::NumberLiteral(42.0).to_rust_type_string(),
            "f64 /* 42 */"
        );
        assert_eq!(
            RustType::BoolLiteral(true).to_rust_type_string(),
            "bool /* true */"
        );
    }

    #[test]
    fn test_union_to_rust_type_fallback() {
        let union = RustType::Union(vec![RustType::String, RustType::F64, RustType::Bool]);
        assert_eq!(union.to_rust_type_string(), "serde_json::Value");
    }

    #[test]
    fn test_union_null_generates_option() {
        let union = RustType::Union(vec![RustType::String, RustType::Null]);
        assert_eq!(union.to_rust_type_string(), "Option<String>");

        let union_undefined = RustType::Union(vec![RustType::F64, RustType::Undefined]);
        assert_eq!(union_undefined.to_rust_type_string(), "serde_json::Value");
    }

    #[test]
    fn test_union_only_null_fallback() {
        let union = RustType::Union(vec![RustType::Null, RustType::Null]);
        assert_eq!(union.to_rust_type_string(), "serde_json::Value");
    }

    #[test]
    fn test_is_serializable_advanced() {
        assert!(RustType::Tuple(vec![RustType::String, RustType::F64]).is_serializable());
        assert!(RustType::Union(vec![RustType::String, RustType::Null]).is_serializable());
        assert!(!RustType::Tuple(vec![RustType::String, RustType::Unknown]).is_serializable());
        assert!(!RustType::Union(vec![RustType::String, RustType::Unknown]).is_serializable());
    }

    #[test]
    fn test_display_trait() {
        assert_eq!(format!("{}", RustType::String), "String");
        assert_eq!(format!("{}", RustType::F64), "f64");
        assert_eq!(
            format!("{}", RustType::Reference("Editor".into())),
            "Editor"
        );
    }
}
