//! Typst 编译偏好设置。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Typst 编译偏好。
///
/// 持久化存储字体搜索路径等配置。
#[derive(Clone, Debug, Serialize, Deserialize, crate::Persistable)]
#[persistable(key = "typst_prefs")]
pub struct TypstPrefs {
    /// 额外的字体搜索路径。
    ///
    /// 在 Android 上默认包含 `/system/fonts`，
    /// 用户可添加自定义路径（如 `/sdcard/fonts`）。
    #[serde(default = "default_font_paths")]
    pub font_paths: Vec<String>,
}

impl Default for TypstPrefs {
    fn default() -> Self {
        Self {
            font_paths: default_font_paths(),
        }
    }
}

/// 默认字体搜索路径。
///
/// Android 上包含 `/system/fonts`，其他平台为空（依赖系统字体发现）。
fn default_font_paths() -> Vec<String> {
    #[cfg(target_os = "android")]
    {
        vec!["/system/fonts".to_string()]
    }
    #[cfg(not(target_os = "android"))]
    {
        vec![]
    }
}

impl TypstPrefs {
    /// 将字符串路径转换为 `PathBuf` 列表。
    pub fn font_path_bufs(&self) -> Vec<PathBuf> {
        self.font_paths.iter().map(PathBuf::from).collect()
    }
}
