//! 文件系统错误类型。

use std::path::PathBuf;

/// 文件系统操作错误。
#[derive(thiserror::Error, Debug)]
pub enum FsError {
    /// 目录遍历错误。
    #[error("failed to walk directory {path}: {source}")]
    Walk {
        path: PathBuf,
        #[source]
        source: ignore::Error,
    },
    /// IO 错误。
    #[error("IO error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// 路径不存在。
    #[error("path not found: {0}")]
    NotFound(PathBuf),
    /// 路径不是目录。
    #[error("not a directory: {0}")]
    NotDir(PathBuf),
    /// 路径已存在。
    #[error("path already exists: {0}")]
    AlreadyExists(PathBuf),
    /// 无效输入。
    #[error("invalid input: {0}")]
    InvalidInput(String),
}

impl FsError {
    pub(crate) fn walk(path: impl Into<PathBuf>, source: ignore::Error) -> Self {
        Self::Walk {
            path: path.into(),
            source,
        }
    }

    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    pub(crate) fn invalid_input(msg: impl Into<String>) -> Self {
        Self::InvalidInput(msg.into())
    }
}
