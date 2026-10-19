use std::path::PathBuf;

use dioxus::prelude::*;
use getset::{Getters, MutGetters, Setters};
use models::{BrowserPrefs, EditorPrefs, GeneralPrefs, OpenMode, Theme, TypstPrefs};

/// 高级搜索参数（用于持久化搜索状态）。
#[derive(Clone, Default, Debug, PartialEq, Getters, Setters)]
#[getset(get = "pub", set = "pub")]
pub struct SearchParams {
    /// 搜索关键词。
    pub query: String,
    /// 是否正则表达式。
    pub is_regex: bool,
    /// 是否大小写敏感。
    pub case_sensitive: bool,
    /// 是否搜索文件内容。
    pub search_content: bool,
    /// 是否仅首个匹配。
    pub only_first_match: bool,
    /// 是否显示隐藏文件。
    pub show_hidden: bool,
    /// 搜索根目录。
    pub root: PathBuf,
}

/// 跨平台应用全局状态，通过 Dioxus Context 跨组件共享。
#[derive(Clone, Default, Getters, MutGetters, Setters)]
#[getset(get = "pub", get_mut = "pub", set = "pub")]
pub struct AppState {
    /// 当前主题（亮色/暗色）。
    theme: Theme,
    /// Typst 编译偏好（字体路径等）。
    typst_prefs: TypstPrefs,
    /// 编辑器偏好（折行、行号等）。
    editor_prefs: EditorPrefs,
    /// 文件打开模式偏好（预览/编辑/记住上次）。
    open_mode: OpenMode,
    /// 文件浏览器偏好（默认打开目录等）。
    browser_prefs: BrowserPrefs,
    /// 通用偏好（启动页、语言等）。
    general_prefs: GeneralPrefs,
    /// 文件浏览器当前目录，跨页面保持。
    current_dir: PathBuf,
    /// 首页当前激活的 tab（0=文件, 1=收藏, 2=更多），跨页面保持。
    active_tab: u8,
    /// 收藏夹中当前打开的目录，跨页面保持。
    fav_open_dir: Option<PathBuf>,
    /// 收藏夹目录浏览的根目录（从入口进入时记录），跨页面保持。
    fav_root_dir: Option<PathBuf>,
    /// 首页高级搜索对话框是否可见，跨页面保持。
    show_search_dialog: bool,
    /// 高级搜索对话框的最后搜索参数，跨页面保持。
    search_params: Option<SearchParams>,
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_theme(theme: Theme) -> Self {
        Self {
            theme,
            ..Default::default()
        }
    }

    /// 默认文件浏览起始目录。
    pub fn default_dir() -> PathBuf {
        #[cfg(target_os = "android")]
        {
            PathBuf::from("/sdcard/Documents")
        }
        #[cfg(not(target_os = "android"))]
        {
            dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
        }
    }

    /// 将初始状态注入 Dioxus Context，返回响应式 [`Signal`]。
    ///
    /// **仅应在根组件中调用一次**。重复调用会因同类型 Context 已存在而 panic。
    pub fn provide(initial: Self) -> Signal<Self> {
        use_context_provider(move || Signal::new(initial))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AppStateError {
    #[error("AppState 未初始化，必须在根组件中先调用 AppState::provide")]
    NotInitialized,
}
