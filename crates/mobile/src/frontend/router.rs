use dioxus::prelude::*;

use crate::frontend::app::App;
use crate::frontend::views::about::About;
use crate::frontend::views::editor::Editor;
use crate::frontend::views::editor_error::EditorError;
use crate::frontend::views::home::Home;
use crate::frontend::views::license::License;
use crate::frontend::views::settings::Settings;

/// 应用路由。
#[derive(Clone, Routable, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    #[layout(App)]
        /// 首页（文件浏览器）
        #[route("/")]
        Home {},
        /// 编辑器，path 为文件绝对路径（URL 编码）
        #[route("/editor/:path")]
        Editor { path: String },
        /// 编辑器错误页面
        #[route("/editor-error")]
        EditorError {},
        /// 设置
        #[route("/settings")]
        Settings {},
        /// 关于
        #[route("/about")]
        About {},
        /// 许可证
        #[route("/license")]
        License {},
}
