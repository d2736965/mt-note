use dioxus::prelude::*;

use crate::frontend::store::top_app_bar_state::{TopAppBarState, TopAppBarStateError};

/// 从 Dioxus Context 获取 TopAppBar 专属状态。
pub fn use_top_app_bar_state() -> Result<Signal<TopAppBarState>, TopAppBarStateError> {
    try_use_context::<Signal<TopAppBarState>>().ok_or(TopAppBarStateError::NotInitialized)
}
