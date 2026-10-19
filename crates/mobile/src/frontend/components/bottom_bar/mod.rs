//! 底部栏容器——按 BottomBarState.kind 渲染 NavBar / BottomAppBar / 无。
use dioxus::logger::tracing;
use dioxus::prelude::*;

use crate::frontend::hooks::use_bottom_bar_state::use_bottom_bar_state;
use crate::frontend::store::bottom_bar_state::BottomBar as BottomBarKind;

pub mod bottom_app_bar;
pub mod nav_bar;

use bottom_app_bar::BottomAppBar;
use nav_bar::NavBar;

#[component]
pub fn BottomBar() -> Element {
    let bottom_bar = match use_bottom_bar_state() {
        Ok(state) => state,
        Err(e) => {
            tracing::error!("BottomBar: {e}");
            return rsx! {};
        }
    };

    match bottom_bar.read().kind() {
        None => rsx! {},
        Some(BottomBarKind::NavigationBar(cfg)) => rsx! {
            NavBar { config: (**cfg).clone() }
        },
        Some(BottomBarKind::BottomAppBar(cfg)) => rsx! {
            BottomAppBar { config: cfg.clone() }
        },
    }
}
