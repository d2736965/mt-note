use dioxus::prelude::*;

use crate::store::app_state::{AppState, AppStateError};

/// 从 Dioxus Context 获取全局状态 [`AppState`] 的响应式 [`Signal`]。
///
/// 需先在根组件调用 [`AppState::provide`](crate::store::app_state::AppState::provide)。
pub fn use_app_state() -> Result<Signal<AppState>, AppStateError> {
    try_use_context::<Signal<AppState>>().ok_or(AppStateError::NotInitialized)
}
