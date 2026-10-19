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

//! 共享数据模型与持久化类型标记。
//!
//! 提供 [`Theme`] 等数据类型，以及 [`Persistable`] trait。
//! 存储基础设施（[`StorageBackend`]、[`PersistStorage`]）由 `persist` crate 提供。

// 使 derive 宏生成的 `::models::...` 路径在 crate 内部也可解析
extern crate self as models;

pub mod data;
pub mod persistable;

pub use data::browser_prefs::BrowserPrefs;
pub use data::editor_prefs::{EditorPrefs, OpenMode, ViewMode, WordWrap};
pub use data::favorites::Favorites;
pub use data::general_prefs::{GeneralPrefs, StartupTab};
pub use data::language::LangPref;
pub use data::theme::Theme;
pub use data::typst_prefs::TypstPrefs;
pub use models_macros::Persistable;
pub use persistable::Persistable;
