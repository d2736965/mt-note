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

//! 类型安全持久化存储基础设施。
//!
//! 提供平台无关的 [`StorageBackend`] trait 和 JSON 序列化引擎 [`PersistStorage`]。
//! 数据类型和 [`Persistable`] trait 由 [`models`] 提供。

mod backend;
mod storage;

pub use backend::StorageBackend;
pub use storage::PersistStorage;