#![forbid(
    unused_must_use,
    clippy::panic,
    clippy::let_underscore_must_use,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::allow_attributes,
    unfulfilled_lint_expectations
)]
// unused_variables 用 deny 而非 forbid：thiserror derive 宏内部生成 #[allow(unused_variables)]，
// forbid 不允许被 allow 覆盖会导致编译失败，deny 则允许 allow 覆盖，效果等价
#![deny(unused_variables, clippy::unwrap_used, clippy::expect_used)]

//! # js-bridge-bindgen
//!
//! 从 TypeScript 源码自动生成 Rust 类型安全绑定代码。
//!
//! `js-bridge-bindgen` 是一个**通用库**，不耦合任何特定框架（如 Dioxus）。
//! 它通过 oxc_parser 解析 TS 源码的 AST，提取导出签名，
//! 然后生成类型安全的 Rust 函数，让 Rust 代码可以安全地调用 JS API。
//!
//! # 解耦设计
//!
//! 生成的代码使用 `JsBridge` trait，由使用者实现具体的 eval 和注入逻辑：
//!
//! - **`JsBridge::GLOBAL_OBJECT`** — JS 全局对象名（如 `"window"`、`"globalThis"`、`"self"`）
//! - **`JsBridge::eval_js`** — 在 JS 运行时中执行代码
//! - **`JsBridge::ensure_injected`** — 确保 JS bundle 已注入运行时
//!
//! # 工厂模式
//!
//! 本库要求 TS 源码遵循**工厂模式**：
//!
//! 1. TS 入口文件必须包含 `declare global` 块，扩展 `Window` 接口
//! 2. `Window` 中声明一个工厂对象（如 `__mtNote`），包含若干 `createXxx` 方法
//! 3. 每个工厂方法必须有对应的 `export function` 声明
//! 4. 工厂方法返回值类型必须有对应的 `export interface` 声明
//! 5. `Window` 中的实例变量（如 `__editor?: Editor`）类型必须与工厂方法返回类型一致
//!
//! # 示例 TS 源码
//!
//! ```ts
//! // src/index.ts
//! export interface EditorConfig {
//!   textareaId: string;
//!   highlightId: string;
//! }
//!
//! export interface Editor {
//!   takeVirtualRange(): string | Promise<string>;
//!   destroy(): void;
//! }
//!
//! export function createEditor(config: EditorConfig): Editor {
//!   // ... 实现
//! }
//!
//! declare global {
//!   interface Window {
//!     __mtNote: {
//!       createEditor: (config: EditorConfig) => Editor;
//!     };
//!     __editor?: Editor;
//!   }
//! }
//! ```
//!
//! # 生成的 Rust 代码
//!
//! ```rust,ignore
//! pub trait JsBridge {
//!     const GLOBAL_OBJECT: &str;
//!     fn eval_js(js: &str);
//!     fn ensure_injected();
//! }
//!
//! pub mod editor {
//!     pub fn create<B: super::JsBridge>(textarea_id: &str, highlight_id: &str) {
//!         B::ensure_injected();
//!         B::eval_js(&format!(
//!             "if(!{g}.__editor){g}.__editor={g}.__mtNote.createEditor({...})",
//!             g = B::GLOBAL_OBJECT
//!         ));
//!     }
//!
//!     pub fn take_virtual_range<B: super::JsBridge>() -> String {
//!         B::ensure_injected();
//!         B::eval_js(&format!(
//!             "if({g}.__editor)return {g}.__editor.takeVirtualRange()",
//!             g = B::GLOBAL_OBJECT
//!         ))
//!     }
//!
//!     pub fn destroy<B: super::JsBridge>() {
//!         B::ensure_injected();
//!         B::eval_js(&format!(
//!             "if({g}.__editor){{g}.__editor.destroy();{g}.__editor=undefined;}}",
//!             g = B::GLOBAL_OBJECT
//!         ));
//!     }
//! }
//! ```
//!
//! # build.rs 集成
//!
//! ```rust,no_run
//! #[allow(clippy::needless_doctest_main)]
//! fn main() {
//!     let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
//!     let out_dir = std::env::var("OUT_DIR").unwrap();
//!
//!     js_bridge_bindgen::generate(&js_bridge_bindgen::Config {
//!         entry: format!("{manifest_dir}/ts/src/index.ts").into(),
//!         out_dir: out_dir.into(),
//!         global_factory_name: "__mtNote".into(),
//!         ..Default::default()
//!     }).unwrap();
//! }
//! ```
//!
//! # 静态资源
//!
//! 静态资源（CSS、字体、JS 等）由使用者在 Rust 代码中通过 `asset!` 宏引用，
//! `js-bridge-bindgen` 不负责资源复制。`asset!` 会在编译时自动将引用的文件
//! 复制到输出目录，无需手动同步。
//!
//! **重要**：`asset!` 禁止直接引用 `node_modules/` 中的资源。所有资源必须经过
//! TS 项目（Vite）处理或筛选后输出到 `dist/`，Rust 侧只能引用 `dist/` 下的产物。
//!
//! ```rust,ignore
//! // ✅ 引用 TS 项目构建产物
//! asset!("/ts/dist/my-app-ts.css")
//! asset!("/ts/dist/cmaps", AssetOptions::folder())
//!
//! // ❌ 禁止直接引用 node_modules
//! asset!("/ts/node_modules/katex/dist/katex.min.css")
//! asset!("/ts/node_modules/pdfjs-dist/cmaps", AssetOptions::folder())
//! ```
//!
//! # Rust 侧使用
//!
//! ```rust,ignore
//! // 自动生成的绑定代码
//! include!(concat!(env!("OUT_DIR"), "/js_api.rs"));
//!
//! // 实现 JsBridge trait
//! struct DioxusBridge;
//! impl JsBridge for DioxusBridge {
//!     const GLOBAL_OBJECT: &str = "window";
//!     fn eval_js(js: &str) { dioxus::document::eval(js); }
//!     fn ensure_injected() { crate::frontend::js_bridge::ensure_bundle_injected(); }
//! }
//!
//! // 调用生成的 API（指定桥接实现）
//! js_api::editor::create::<DioxusBridge>("textarea", "highlight");
//! let range = js_api::editor::take_virtual_range::<DioxusBridge>();
//! js_api::editor::destroy::<DioxusBridge>();
//! ```
//!
//! # 模式校验
//!
//! 如果 TS 源码不符合工厂模式要求，`generate()` 会返回 `Err(Error::ValidationFailed)`，
//! 包含详细的错误信息：
//!
//! ```text
//! error[js-bridge-bindgen]: E001 — 缺少 `declare global` 块
//!   = hint: 添加 `declare global { interface Window { ... } }` 块声明全局变量
//!
//! error[js-bridge-bindgen]: E005 — 工厂方法 `createEditor` 在 `declare global` 中声明，但没有对应的 `export function`
//!   = hint: 添加 `export function createEditor(...): Editor { ... }`
//! ```

mod codegen;
mod config;
mod error;
mod parser;
mod types;
mod validation;

pub use codegen::generate_code;
pub use config::Config;
pub use error::{Error, ValidationError, ValidationLevel, ValidationResult};
pub use parser::parse_from_str;
pub use types::ParsedModule;
pub use validation::validate;

/// JS 桥接 trait，由使用者实现具体的 eval、注入和全局对象逻辑。
///
/// 生成的绑定代码通过此 trait 与 JS 运行时交互，不耦合任何特定框架。
///
/// # 示例
///
/// ```rust,ignore
/// use js_bridge_bindgen::JsBridge;
///
/// struct DioxusBridge;
/// impl JsBridge for DioxusBridge {
///     const GLOBAL_OBJECT: &str = "window";
///     fn eval_js(js: &str) {
///         dioxus::document::eval(js);
///     }
///     fn ensure_injected() {
///         crate::frontend::js_bridge::ensure_bundle_injected();
///     }
/// }
/// ```
pub trait JsBridge {
    /// JS 全局对象名（如 `"window"`、`"globalThis"`、`"self"`）。
    ///
    /// 生成的 JS 代码通过此对象访问工厂命名空间和实例变量。
    /// 浏览器环境用 `"window"`，Web Worker 用 `"self"`，
    /// Node.js 用 `"globalThis"` 等。
    const GLOBAL_OBJECT: &str;

    /// 在 JS 运行时中执行代码（fire-and-forget，无返回值）。
    fn eval_js(js: &str);

    /// 在 JS 运行时中执行代码并异步获取返回值。
    ///
    /// 返回 `serde_json::Value`，由生成的代码反序列化为具体类型。
    /// 实现 should 使用 JS 运行时的异步 eval 机制（如 `eval().join().await`）。
    fn eval_async(js: &str) -> impl std::future::Future<Output = serde_json::Value>;

    /// 确保 JS bundle 已注入运行时。
    fn ensure_injected();
}

/// 解析 TS 源码并生成 Rust 绑定代码
///
/// 这是 `js-bridge-bindgen` 的核心入口函数。
/// 它会：
/// 1. 使用 oxc_parser 解析 TS 源码
/// 2. 从 AST 中提取导出签名（函数、接口、枚举等）
/// 3. 校验 TS 源码是否符合工厂模式要求
/// 4. 生成 Rust 代码到 `config.out_dir / config.output_file`
///
/// 静态资源（CSS、字体、JS 等）由使用者在 Rust 代码中通过 `asset!` 宏直接引用，
/// `js-bridge-bindgen` 不负责资源复制。
///
/// # 错误
///
/// - `Error::EntryNotFound` — TS 入口文件不存在
/// - `Error::Io` — 文件读取失败
/// - `Error::ParseFailed` — TS 源码解析失败（语法错误）
/// - `Error::ValidationFailed` — TS 源码不符合工厂模式要求
/// - `Error::CodegenFailed` — 代码生成失败
/// - `Error::OutputWriteFailed` — 输出文件写入失败
///
/// # 示例
///
/// ```rust,no_run
/// use js_bridge_bindgen::{Config, generate};
///
/// let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
/// let out_dir = std::env::var("OUT_DIR").unwrap();
///
/// let result = generate(&Config {
///     entry: format!("{manifest_dir}/ts/src/index.ts").into(),
///     out_dir: out_dir.into(),
///     global_factory_name: "__mtNote".into(),
///     ..Default::default()
/// });
///
/// match result {
///     Ok(()) => println!("绑定代码生成成功"),
///     Err(e) => eprintln!("错误: {e}"),
/// }
/// ```
pub fn generate(config: &Config) -> Result<(), Error> {
    if !config.entry.exists() {
        return Err(Error::EntryNotFound(config.entry.clone()));
    }

    let module = parser::parse(config)?;

    let validation_result = validation::validate(&module, config);
    if !validation_result.is_valid() {
        return Err(Error::ValidationFailed(
            validation_result.to_error_message(),
        ));
    }

    codegen::generate(&module, config)?;

    Ok(())
}

/// 仅解析 TS 源码，不生成代码
///
/// 适用于需要自定义代码生成逻辑或仅做模式校验的场景。
///
/// # 示例
///
/// ```rust,no_run
/// use js_bridge_bindgen::{Config, parse_only};
///
/// let module = parse_only(&Config {
///     entry: "ts/src/index.ts".into(),
///     out_dir: ".".into(),
///     global_factory_name: "__mtNote".into(),
///     ..Default::default()
/// }).unwrap();
///
/// for func in &module.factory_functions {
///     println!("工厂函数: {}", func.name);
/// }
/// ```
pub fn parse_only(config: &Config) -> Result<ParsedModule, Error> {
    if !config.entry.exists() {
        return Err(Error::EntryNotFound(config.entry.clone()));
    }
    parser::parse(config)
}

/// 解析并校验 TS 源码，不生成代码
///
/// 返回校验结果，适用于 IDE 集成或 CI 检查。
pub fn validate_only(config: &Config) -> Result<ValidationResult, Error> {
    let module = parse_only(config)?;
    Ok(validation::validate(&module, config))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("create temp dir")
    }

    fn write_ts_file(dir: &std::path::Path, content: &str) -> std::path::PathBuf {
        let path = dir.join("test.ts");
        let mut f = std::fs::File::create(&path).expect("create ts file");
        f.write_all(content.as_bytes()).expect("write ts file");
        path
    }

    fn make_config(entry: std::path::PathBuf, out_dir: std::path::PathBuf) -> Config {
        Config {
            entry,
            out_dir,
            global_factory_name: "__mtNote".into(),
            ..Default::default()
        }
    }

    #[test]
    fn test_generate_entry_not_found() {
        let dir = tmp_dir();
        let config = make_config(dir.path().join("nonexistent.ts"), dir.path().to_path_buf());
        let result = generate(&config);
        assert!(matches!(result, Err(Error::EntryNotFound(_))));
    }

    #[test]
    fn test_parse_only_entry_not_found() {
        let dir = tmp_dir();
        let config = make_config(dir.path().join("nonexistent.ts"), dir.path().to_path_buf());
        let result = parse_only(&config);
        assert!(matches!(result, Err(Error::EntryNotFound(_))));
    }

    #[test]
    fn test_parse_only_valid_file() {
        let dir = tmp_dir();
        let ts_path = write_ts_file(
            dir.path(),
            "export function createEditor(): Editor { return {} as Editor; }",
        );
        let config = make_config(ts_path, dir.path().to_path_buf());
        let result = parse_only(&config);
        assert!(result.is_ok());
        let module = result.expect("parse_only");
        assert_eq!(module.factory_functions.len(), 1);
    }

    #[test]
    fn test_validate_only_valid() {
        let dir = tmp_dir();
        let ts_content = r#"
            export function createEditor(): Editor { return {} as Editor; }
            export interface Editor { destroy(): void; }
            declare global {
                interface Window {
                    __mtNote: { createEditor: () => Editor; };
                    __editor?: Editor;
                }
            }
        "#;
        let ts_path = write_ts_file(dir.path(), ts_content);
        let config = make_config(ts_path, dir.path().to_path_buf());
        let result = validate_only(&config);
        assert!(result.is_ok());
        let validation = result.expect("validate_only");
        assert!(validation.is_valid());
    }

    #[test]
    fn test_validate_only_missing_global() {
        let dir = tmp_dir();
        let ts_path = write_ts_file(
            dir.path(),
            "export function createEditor(): Editor { return {} as Editor; }",
        );
        let config = make_config(ts_path, dir.path().to_path_buf());
        let result = validate_only(&config);
        assert!(result.is_ok());
        let validation = result.expect("validate_only");
        assert!(!validation.is_valid());
    }

    #[test]
    fn test_generate_writes_file() {
        let dir = tmp_dir();
        let ts_content = r#"
            export function createEditor(): Editor { return {} as Editor; }
            export interface Editor { destroy(): void; }
            declare global {
                interface Window {
                    __mtNote: { createEditor: () => Editor; };
                    __editor?: Editor;
                }
            }
        "#;
        let ts_path = write_ts_file(dir.path(), ts_content);
        let config = make_config(ts_path, dir.path().to_path_buf());
        let result = generate(&config);
        assert!(result.is_ok());

        let output_path = dir.path().join("js_api.rs");
        assert!(output_path.exists(), "输出文件应被创建");
        let content = std::fs::read_to_string(&output_path).expect("read output");
        assert!(content.contains("此文件由 js-bridge-bindgen 自动生成"));
    }

    #[test]
    fn test_generate_validation_fails() {
        let dir = tmp_dir();
        let ts_path = write_ts_file(dir.path(), "export function foo(): void {}");
        let config = make_config(ts_path, dir.path().to_path_buf());
        let result = generate(&config);
        assert!(matches!(result, Err(Error::ValidationFailed(_))));
    }
}
