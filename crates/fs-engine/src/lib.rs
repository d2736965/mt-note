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

//! 文件系统引擎。
//!
//! 提供目录遍历、文件读写、文件类型检测等功能。
//! 使用 `ignore` crate 遍历目录，自动遵循 `.gitignore` 规则。

mod dir;
mod display;
mod error;
mod file_type;

pub use dir::{
    ContentMatch, DirEntry, DirLister, SearchOptions, SearchResult, create_dir, create_file,
    read_file, remove_path, rename_path, write_bytes, write_file,
};
pub use display::entry_display_name;
pub use error::FsError;
pub use file_type::{FileCategory, is_text_file};
