//! 编辑器偏好设置。

use serde::{Deserialize, Serialize};

/// 文件打开时的默认视图模式。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, crate::Persistable)]
#[serde(rename_all = "snake_case")]
#[persistable(key = "open_mode")]
pub enum OpenMode {
    /// 默认预览模式打开。
    #[default]
    Preview,
    /// 默认编辑模式打开。
    Edit,
    /// 记住上次使用的模式。
    Remember,
}

/// 单个文件的视图模式。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewMode {
    /// 编辑模式。
    Edit,
    /// 预览模式。
    Preview,
}

impl ViewMode {
    /// 切换到另一种模式。
    pub fn toggle(self) -> Self {
        match self {
            Self::Edit => Self::Preview,
            Self::Preview => Self::Edit,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Edit => "编辑",
            Self::Preview => "预览",
        }
    }
}

/// 代码编辑器折行模式。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WordWrap {
    /// 不折行，水平滚动。
    #[default]
    Off,
    /// 折行显示，带续行符。
    On,
}

impl WordWrap {
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "不折行",
            Self::On => "折行",
        }
    }
}

/// 编辑器偏好设置。
///
/// 持久化存储编辑器的显示配置，如折行模式、行号显示等。
#[derive(Clone, Debug, Serialize, Deserialize, crate::Persistable)]
#[persistable(key = "editor_prefs")]
pub struct EditorPrefs {
    /// 折行模式。
    #[serde(default)]
    pub word_wrap: WordWrap,
    /// 是否显示行号。
    #[serde(default = "default_show_line_numbers")]
    pub show_line_numbers: bool,
    /// 折行列数（每行最大字符数）。
    ///
    /// 仅在 `word_wrap == On` 时生效。
    /// 默认 40，适合手机屏幕宽度。
    #[serde(default = "default_wrap_column")]
    pub wrap_column: usize,
    /// 上次使用的视图模式，用于 `OpenMode::Remember`。
    #[serde(default)]
    pub last_view_mode: Option<ViewMode>,
    /// 是否在光标位置显示 hover 类型信息。
    #[serde(default)]
    pub show_hover_info: bool,
    /// 是否在光标位置显示诊断信息。
    #[serde(default = "default_show_diagnostics")]
    pub show_diagnostics: bool,
}

impl Default for EditorPrefs {
    fn default() -> Self {
        Self {
            word_wrap: WordWrap::default(),
            show_line_numbers: default_show_line_numbers(),
            wrap_column: default_wrap_column(),
            last_view_mode: None,
            show_hover_info: false,
            show_diagnostics: default_show_diagnostics(),
        }
    }
}

fn default_show_line_numbers() -> bool {
    true
}

fn default_wrap_column() -> usize {
    40
}

fn default_show_diagnostics() -> bool {
    true
}
