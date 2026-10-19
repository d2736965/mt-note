//! 文件浏览器偏好设置。

use serde::{Deserialize, Serialize};

/// 文件浏览器偏好。
///
/// 持久化存储文件浏览器的默认起始目录等配置。
#[derive(Clone, Debug, Default, Serialize, Deserialize, crate::Persistable)]
#[persistable(key = "browser_prefs")]
pub struct BrowserPrefs {
    /// 默认打开目录（绝对路径）。
    ///
    /// 为 `None` 时使用平台默认目录（Android 上为 `/sdcard/Documents`）。
    #[serde(default)]
    pub default_dir: Option<String>,
}
