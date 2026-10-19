use serde::{Deserialize, Serialize};

/// 应用主题。
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, crate::Persistable)]
#[serde(rename_all = "snake_case")]
#[persistable(key = "theme")]
pub enum Theme {
    /// 亮色主题
    #[default]
    Light,
    /// 暗色主题
    Dark,
}

impl Theme {
    /// 在亮/暗之间切换。
    pub fn toggle(&mut self) {
        *self = match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::Light,
        };
    }

    /// 返回主题的展示标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Light => "\u{2600}  亮色",
            Self::Dark => "\u{263d}  暗色",
        }
    }

    /// 是否为暗色主题。
    pub fn is_dark(&self) -> bool {
        matches!(self, Self::Dark)
    }
}
