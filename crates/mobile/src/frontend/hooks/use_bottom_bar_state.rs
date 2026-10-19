//! 从 Dioxus Context 获取 BottomBar 专属状态。

use dioxus::prelude::*;

use crate::frontend::store::bottom_bar_state::{BottomBarState, BottomBarStateError};

pub fn use_bottom_bar_state() -> Result<Signal<BottomBarState>, BottomBarStateError> {
    try_use_context::<Signal<BottomBarState>>().ok_or(BottomBarStateError::NotInitialized)
}
