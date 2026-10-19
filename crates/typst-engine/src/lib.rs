#![forbid(
    unused_must_use,
    clippy::panic,
    clippy::let_underscore_must_use,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::allow_attributes,
    unfulfilled_lint_expectations,
)]

//! Typst 编译引擎。
//!
//! 封装 typst 编译和 SVG 输出功能，提供简洁的 API：
//!
//! - [`TypstWorld`]：实现 `typst_library::World` trait，管理字体、文件和包
//! - [`TypstWorld::recompile`]：增量重编译，复用 World 和 comemo 缓存
//! - [`compile_svg`]：编译 typst 源码并输出 SVG 字符串
//! - [`compile_document`]：编译 typst 源码并返回 `PagedDocument`
//! - [`compile_svg_with_text_layer`]：编译并输出 SVG + 透明文字层 HTML
//! - [`render_svg_string`]：从已有页面并行渲染 SVG
//! - [`render_svg_with_text_layer`]：从已有文档并行渲染 SVG + 文字层
//!
//! 性能优化：
//! - 多页文档使用 rayon 并行生成 SVG 和文字层
//! - `TypstWorld` 可复用实现增量编译
//! - 前端应通过 `spawn_blocking` 调用编译函数

mod compiler;
mod error;
mod text_layer;
mod world;

pub use compiler::{CompileTypst, CompileTypstContent, CompileTypstPdf, get_compiler};
pub use error::TypstEngineError;
pub use text_layer::{FaceCollector, FontFace, TextLayerItem};
pub use world::{
    ContentRect, TypstFileLoader, TypstWorld, compile_document, compile_svg,
    compile_svg_with_text_layer, render_svg_string, render_svg_with_text_layer,
};