//! Typst 引擎错误类型。

use std::path::PathBuf;

/// Typst 编译引擎错误。
#[derive(thiserror::Error, Debug)]
pub enum TypstEngineError {
    /// 编译错误。
    #[error("Typst compilation failed: {0}")]
    Compile(String),

    /// 文件未找到。
    #[error("file not found: {0}")]
    FileNotFound(PathBuf),

    /// IO 错误。
    #[error("IO error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// 字体加载错误。
    #[error("failed to load fonts: {0}")]
    FontLoad(String),
}

impl TypstEngineError {
    #[expect(dead_code, reason = "IO 错误构造器，文件操作功能实现后使用")]
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
