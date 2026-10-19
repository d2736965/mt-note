use std::collections::HashMap;
use std::path::PathBuf;

/// 代码生成配置
///
/// `js-bridge-bindgen` 是通用库，不耦合任何特定框架。
/// 生成的代码使用 `JsBridge` trait，由使用者实现具体的 eval 和注入逻辑。
///
/// 静态资源（CSS、字体、JS 等）由使用者在 Rust 代码中通过 `asset!` 宏直接引用，
/// `js-bridge-bindgen` 不负责资源复制。`asset!` 会在编译时自动将引用的文件
/// 复制到输出目录，无需手动同步。
///
/// # 示例
///
/// ```rust,no_run
/// use js_bridge_bindgen::Config;
///
/// let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
/// let out_dir = std::env::var("OUT_DIR").unwrap();
///
/// let config = Config {
///     entry: format!("{manifest_dir}/ts/src/index.ts").into(),
///     out_dir: out_dir.into(),
///     global_factory_name: "__mtNote".into(),
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone)]
pub struct Config {
    /// TS 入口文件路径（通常是 `ts/src/index.ts`）
    ///
    /// 此文件必须包含 `declare global` 块，声明 `Window` 接口扩展，
    /// 以及对应的 `export function` 工厂函数。
    pub entry: PathBuf,

    /// 输出目录（通常是 `OUT_DIR` 环境变量）
    ///
    /// 生成的 Rust 绑定代码将写入 `out_dir / output_file`。
    pub out_dir: PathBuf,

    /// 生成的 Rust 文件名（默认 `"js_api.rs"`）
    pub output_file: String,

    /// Rust 侧命名空间（生成的最外层 `pub mod` 名，默认 `"js_api"`）
    ///
    /// 例如 `"js_api"` → `crate::frontend::js_api::editor::create::<B>(...)`
    pub rust_namespace: String,

    /// 全局工厂对象名（如 `"__mtNote"`）
    ///
    /// 生成的代码通过 `window.{global_factory_name}` 访问工厂对象。
    /// TS 源码中 `declare global` 块必须声明此属性。
    pub global_factory_name: String,

    /// 实例变量名映射（可选，默认从 `declare global` 自动检测）
    ///
    /// 键为工厂函数名（如 `"createEditor"`），值为 Window 上的实例变量名（如 `"__editor"`）。
    /// 设为 `None` 时，自动从 `declare global` 中检测：
    /// 工厂函数 `createFoo` 对应实例变量 `__foo`（首字母小写 + 前缀 `__`）。
    ///
    /// 仅在需要覆盖自动检测结果时使用。
    pub instance_vars: Option<HashMap<String, String>>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            entry: PathBuf::from("index.ts"),
            out_dir: PathBuf::from("."),
            output_file: String::from("js_api.rs"),
            rust_namespace: String::from("js_api"),
            global_factory_name: String::from("__mtNote"),
            instance_vars: None,
        }
    }
}

impl Config {
    /// 获取实例变量名
    ///
    /// 优先使用 `instance_vars` 中的显式映射，否则按约定自动推导：
    /// `createFoo` → `__foo`（去掉 `create` 前缀，首字母小写，加 `__` 前缀）
    pub fn resolve_instance_var(&self, factory_name: &str) -> String {
        if let Some(vars) = &self.instance_vars
            && let Some(var_name) = vars.get(factory_name)
        {
            return var_name.clone();
        }
        auto_instance_var(factory_name)
    }
}

fn auto_instance_var(factory_name: &str) -> String {
    let name = factory_name.strip_prefix("create").unwrap_or(factory_name);
    let mut chars = name.chars();
    match chars.next() {
        Some(c) => {
            let lower = c.to_ascii_lowercase();
            format!("__{lower}{}", chars.as_str())
        }
        None => format!("__{name}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_instance_var() {
        assert_eq!(auto_instance_var("createEditor"), "__editor");
        assert_eq!(auto_instance_var("createPreview"), "__preview");
        assert_eq!(auto_instance_var("createPdfPreview"), "__pdfPreview");
        assert_eq!(
            auto_instance_var("createBackInterceptor"),
            "__backInterceptor"
        );
    }

    #[test]
    fn test_resolve_instance_var_explicit() {
        let config = Config {
            instance_vars: Some(HashMap::from([(
                "createEditor".into(),
                "__myEditor".into(),
            )])),
            ..Default::default()
        };
        assert_eq!(config.resolve_instance_var("createEditor"), "__myEditor");
        assert_eq!(config.resolve_instance_var("createPreview"), "__preview");
    }

    #[test]
    fn test_default_values() {
        let config = Config::default();
        assert_eq!(config.entry, PathBuf::from("index.ts"));
        assert_eq!(config.out_dir, PathBuf::from("."));
        assert_eq!(config.output_file, "js_api.rs");
        assert_eq!(config.rust_namespace, "js_api");
        assert_eq!(config.global_factory_name, "__mtNote");
        assert!(config.instance_vars.is_none());
    }

    #[test]
    fn test_struct_update_syntax() {
        let config = Config {
            entry: PathBuf::from("ts/src/index.ts"),
            out_dir: PathBuf::from("/tmp"),
            global_factory_name: "__myFactory".into(),
            ..Default::default()
        };
        assert_eq!(config.entry, PathBuf::from("ts/src/index.ts"));
        assert_eq!(config.output_file, "js_api.rs");
        assert_eq!(config.rust_namespace, "js_api");
        assert_eq!(config.global_factory_name, "__myFactory");
    }
}