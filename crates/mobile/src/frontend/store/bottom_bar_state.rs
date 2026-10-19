//! BottomBar 状态——底部栏（NavBar/BottomAppBar）的互斥类型与配置，
//! 通过 Dioxus Context 跨组件共享。与 TopAppBarState 平行独立。
//!
//! # Dead Code 说明
//!
//! 本模块中所有 public 类型和函数将在 Task 4-6 中使用：
//! - Task 4: NavBar 容器组件使用 `NavBarItems`/`NavBarConfig`/`NavBarItem`
//! - Task 5: BottomAppBar 容器组件使用 `BottomAppBarConfig`/`BottomAppBarAction`
//! - Task 6: app.rs 集成使用 `BottomBarState::provide`/`use_bottom_bar_state` hook
#![expect(dead_code, reason = "Task 4-6 会使用本模块所有 public 类型和方法")]

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::*;
use getset::Getters;
use ui::components::icons::Icon;

/// 底部栏类型（单值枚举，硬互斥）。通过 `Option<BottomBar>` 表达有无底栏。
#[derive(Clone)]
pub enum BottomBar {
    NavigationBar(Box<NavBarConfig>),
    BottomAppBar(BottomAppBarConfig),
}

/// 底部栏专属状态，通过 Dioxus Context 共享。与 [`TopAppBarState`] 分离。
#[derive(Clone, Default, Getters)]
pub struct BottomBarState {
    #[getset(get = "pub")]
    kind: Option<BottomBar>,
}

impl BottomBarState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn provide(initial: Self) -> Signal<Self> {
        use_context_provider(move || Signal::new(initial))
    }

    pub fn set_nav_bar(&mut self, cfg: NavBarConfig) {
        self.kind = Some(BottomBar::NavigationBar(Box::new(cfg)));
    }

    pub fn set_bottom_app_bar(&mut self, cfg: BottomAppBarConfig) {
        self.kind = Some(BottomBar::BottomAppBar(cfg));
    }

    /// 重置为 `None`，用于路由切换时清理上一页残留（与 TopAppBarState::reset 同处调用）。
    pub fn reset(&mut self) {
        self.kind = None;
    }
}

// ---- NavBar ----

/// NavBar 目的地数量，枚举限定 3/4/5（M3 规范），类型层面保证，无需运行时边界处理。
#[derive(Clone)]
pub enum NavBarItems {
    Three([NavBarItem; 3]),
    Four([NavBarItem; 4]),
    Five([NavBarItem; 5]),
}

#[derive(Clone)]
pub struct NavBarConfig {
    pub items: NavBarItems,
}

#[derive(Clone)]
pub struct NavBarItem {
    pub icon: Icon,
    pub label: String,
    pub active: bool,
    pub on_click: Rc<RefCell<dyn FnMut()>>,
}

impl PartialEq for NavBarItem {
    fn eq(&self, other: &Self) -> bool {
        self.label == other.label && self.active == other.active
    }
}
impl PartialEq for NavBarItems {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Three(a), Self::Three(b)) => a == b,
            (Self::Four(a), Self::Four(b)) => a == b,
            (Self::Five(a), Self::Five(b)) => a == b,
            _ => false,
        }
    }
}
impl PartialEq for NavBarConfig {
    fn eq(&self, other: &Self) -> bool {
        self.items == other.items
    }
}

// ---- BottomAppBar ----

#[derive(Clone)]
pub struct BottomAppBarConfig {
    pub actions: Vec<BottomAppBarAction>,
}

#[derive(Clone)]
pub struct BottomAppBarAction {
    pub icon: Icon,
    pub on_click: Rc<RefCell<dyn FnMut()>>,
}

impl PartialEq for BottomAppBarAction {
    fn eq(&self, other: &Self) -> bool {
        self.icon == other.icon
    }
}
impl PartialEq for BottomAppBarConfig {
    fn eq(&self, other: &Self) -> bool {
        self.actions == other.actions
    }
}

// ---- BottomBar PartialEq（Signal 需要；不比较闭包） ----

impl PartialEq for BottomBar {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::NavigationBar(a), Self::NavigationBar(b)) => a == b,
            (Self::BottomAppBar(a), Self::BottomAppBar(b)) => a == b,
            _ => false,
        }
    }
}

impl std::fmt::Debug for BottomBar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NavigationBar(_) => write!(f, "BottomBar::NavigationBar(..)"),
            Self::BottomAppBar(_) => write!(f, "BottomBar::BottomAppBar(..)"),
        }
    }
}

impl std::fmt::Debug for BottomBarState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BottomBarState")
            .field("kind", &self.kind)
            .finish()
    }
}

// ---- 错误 ----

#[derive(Debug, thiserror::Error)]
pub enum BottomBarStateError {
    #[error("BottomBarState 未初始化，必须在根组件中先调用 BottomBarState::provide")]
    NotInitialized,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui::components::icons::Icon;

    fn dummy_item(label: &str) -> NavBarItem {
        NavBarItem {
            icon: Icon::ArrowBack, // 任一现有变体；若不存在用实际可用的
            label: label.to_string(),
            active: false,
            on_click: std::rc::Rc::new(std::cell::RefCell::new(|| {})),
        }
    }

    #[test]
    fn default_is_none() {
        let s = BottomBarState::default();
        assert!(s.kind().is_none());
    }

    #[test]
    fn set_nav_bar_then_reset() {
        let mut s = BottomBarState::default();
        s.set_nav_bar(NavBarConfig {
            items: NavBarItems::Three([dummy_item("a"), dummy_item("b"), dummy_item("c")]),
        });
        assert!(matches!(s.kind(), Some(BottomBar::NavigationBar(_))));
        s.reset();
        assert!(s.kind().is_none());
    }

    #[test]
    fn set_bottom_app_bar_overwrites() {
        let mut s = BottomBarState::default();
        s.set_bottom_app_bar(BottomAppBarConfig { actions: vec![] });
        assert!(matches!(s.kind(), Some(BottomBar::BottomAppBar(_))));
    }
}
