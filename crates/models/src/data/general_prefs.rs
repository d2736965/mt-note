//! 通用偏好设置。

use serde::{Deserialize, Serialize};

use super::language::LangPref;

/// 启动时进入的首页 tab。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupTab {
    /// 文件。
    #[default]
    Files,
    /// 收藏。
    Favorites,
}

/// 通用偏好。
///
/// 持久化存储启动页等应用级通用配置。
#[derive(Clone, Debug, Default, Serialize, Deserialize, crate::Persistable)]
#[persistable(key = "general_prefs")]
pub struct GeneralPrefs {
    /// 启动时进入的首页 tab。
    #[serde(default)]
    pub startup_tab: StartupTab,
    /// 界面语言偏好。
    #[serde(default)]
    pub language: LangPref,
}
