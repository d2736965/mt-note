use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::*;
use getset::Getters;
use ui::components::icons::Icon;

/// MD3 TopAppBar 专属状态，通过 Dioxus Context 跨组件共享。
///
/// 与 [`ui::store::app_state::AppState`] 分离：
/// AppBar 的渲染是 MD3 专属的（仅 mobile），其状态不应放在通用 `ui` crate 中。
#[derive(Clone, Default, Getters)]
pub struct TopAppBarState {
    /// 当前页面标题。
    #[getset(get = "pub")]
    title: String,
    /// 当前页面的菜单项列表。
    #[getset(get = "pub")]
    menu_items: Vec<MenuItem>,
    /// 子菜单导航历史（栈）。
    menu_history: Vec<Vec<MenuItem>>,
    /// 是否处于搜索模式。
    #[getset(get = "pub")]
    search_mode: bool,
    /// 搜索查询字符串。
    #[getset(get = "pub")]
    search_query: String,
    /// AppBar trailing 区域的自定义按钮。
    #[getset(get = "pub")]
    trailing_buttons: Vec<TrailingButton>,
    /// 是否显示返回箭头（None 表示根据路由自动判断）。
    #[getset(get = "pub")]
    show_back: Option<bool>,
}

impl TopAppBarState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn provide(initial: Self) -> Signal<Self> {
        use_context_provider(move || Signal::new(initial))
    }

    // ---- 页面标题 ----

    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
    }

    // ---- 菜单项 ----

    /// 设置当前页面的菜单项，同时清空子菜单历史。
    pub fn set_menu_items(&mut self, items: Vec<MenuItem>) {
        self.menu_items = items;
        self.menu_history.clear();
    }

    pub fn menu_history_depth(&self) -> usize {
        self.menu_history.len()
    }

    pub fn open_submenu(&mut self, sub_items: Vec<MenuItem>) {
        self.menu_history.push(self.menu_items.clone());
        self.menu_items = sub_items;
    }

    pub fn close_submenu(&mut self) -> Result<(), TopAppBarStateError> {
        match self.menu_history.pop() {
            Some(parent_items) => {
                self.menu_items = parent_items;
                Ok(())
            }
            None => Err(TopAppBarStateError::MenuHistoryEmpty),
        }
    }

    pub fn reset_menu_navigation(&mut self) {
        if let Some(original) = self.menu_history.first().cloned() {
            self.menu_items = original;
        }
        self.menu_history.clear();
    }

    // ---- 搜索模式 ----

    pub fn enter_search(&mut self) {
        self.search_mode = true;
        self.search_query.clear();
    }

    pub fn exit_search(&mut self) {
        self.search_mode = false;
        self.search_query.clear();
    }

    pub fn set_search_query(&mut self, query: String) {
        self.search_query = query;
    }

    // ---- Trailing 按钮 ----

    pub fn set_trailing_buttons(&mut self, buttons: Vec<TrailingButton>) {
        self.trailing_buttons = buttons;
    }

    // ---- 返回箭头 ----

    #[expect(dead_code, reason = "各页面通过此方法设置返回箭头显示，尚未有页面实现")]
    pub fn set_show_back(&mut self, show: bool) {
        self.show_back = Some(show);
    }

    #[expect(dead_code, reason = "重置返回箭头状态功能尚未使用")]
    pub fn reset_show_back(&mut self) {
        self.show_back = None;
    }

    /// 重置所有状态到默认值，用于页面切换时清理上一页残留。
    /// 不清理搜索状态（search_mode / search_query），搜索由 TopAppBar 独立管理。
    pub fn reset(&mut self) {
        self.title.clear();
        self.menu_items.clear();
        self.menu_history.clear();
        self.trailing_buttons.clear();
        self.show_back = None;
    }
}

// ---- 菜单项 ----

#[derive(Clone)]
pub struct MenuItem {
    pub label: String,
    pub on_click: Option<Rc<RefCell<dyn FnMut()>>>,
    pub children: Option<Vec<MenuItem>>,
}

impl MenuItem {
    pub fn item(label: impl Into<String>, handler: impl FnMut() + 'static) -> Self {
        Self {
            label: label.into(),
            on_click: Some(Rc::new(RefCell::new(handler))),
            children: None,
        }
    }

    pub fn submenu(label: impl Into<String>, children: Vec<MenuItem>) -> Self {
        Self {
            label: label.into(),
            on_click: None,
            children: Some(children),
        }
    }
}

impl std::fmt::Debug for MenuItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MenuItem")
            .field("label", &self.label)
            .field("on_click", &self.on_click.as_ref().map(|_| "..."))
            .field("children", &self.children)
            .finish()
    }
}

impl PartialEq for MenuItem {
    fn eq(&self, other: &Self) -> bool {
        self.label == other.label && self.children == other.children
    }
}

// ---- Trailing 按钮 ----

#[derive(Clone)]
pub struct TrailingButton {
    pub icon_on: Icon,
    pub icon_off: Option<Icon>,
    pub active: bool,
    pub on_click: Rc<RefCell<dyn FnMut()>>,
}

impl TrailingButton {
    pub fn toggle(
        icon_on: Icon,
        icon_off: Icon,
        active: bool,
        on_click: impl FnMut() + 'static,
    ) -> Self {
        Self {
            icon_on,
            icon_off: Some(icon_off),
            active,
            on_click: Rc::new(RefCell::new(on_click)),
        }
    }

    pub fn simple_toggle(icon: Icon, active: bool, on_click: impl FnMut() + 'static) -> Self {
        Self {
            icon_on: icon,
            icon_off: None,
            active,
            on_click: Rc::new(RefCell::new(on_click)),
        }
    }

    pub fn current_icon(&self) -> &Icon {
        if self.active {
            &self.icon_on
        } else {
            self.icon_off.as_ref().unwrap_or(&self.icon_on)
        }
    }
}

impl PartialEq for TrailingButton {
    fn eq(&self, other: &Self) -> bool {
        self.icon_on == other.icon_on
            && self.icon_off == other.icon_off
            && self.active == other.active
    }
}

// ---- 错误类型 ----

#[derive(Debug, thiserror::Error)]
pub enum TopAppBarStateError {
    #[error("TopAppBarState 未初始化，必须在根组件中先调用 TopAppBarState::provide")]
    NotInitialized,
    #[error("菜单历史为空，无法返回上级菜单")]
    MenuHistoryEmpty,
}
