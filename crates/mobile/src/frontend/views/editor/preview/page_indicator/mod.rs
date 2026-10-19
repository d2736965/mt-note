//! 页码标签组件。
//!
//! 为每个纸张页面左下角添加独立的页码标签。
//! 页码是纸张自身的属性，跟随纸张滚动。
//! 默认隐藏，交互（触摸、滚动、缩放）时短暂显示后自动消失。

use dioxus::prelude::*;
use dioxus_core::Task;

use crate::frontend::js_bridge::{DioxusBridge, js_api};

const HIDE_DELAY_MS: u64 = 1500;

/// 注入页码标签到 PDF 预览的每个页面。
///
/// 在 JS 端为每个 `pageDiv` 添加一个 `.page-num-label` 子元素，
/// 内容为页码数字，定位在页面左下角。
pub fn inject_pdf_page_numbers(container_id: &str) {
    js_api::page_indicator::inject_pdf::<DioxusBridge>(container_id);
}

/// 注入页码标签到 Typst 预览的每个页面。
///
/// 在 JS 端为每个 `.typst-page` 元素添加一个 `.page-num-label` 子元素，
/// 内容为页码数字，定位在页面左下角。
pub fn inject_typst_page_numbers(container_id: &str) {
    js_api::page_indicator::inject_typst::<DioxusBridge>(container_id);
}

/// 显示所有页码标签，并在延迟后自动隐藏。
///
/// 交互事件（触摸、滚动、缩放）触发时调用。
/// 先移除所有标签的 `hidden` class，然后设置定时器添加回 `hidden` class。
pub fn show_page_numbers(mut hide_task: Signal<Option<Task>>) {
    js_api::page_indicator::show::<DioxusBridge>();
    if let Some(t) = hide_task.take() {
        t.cancel();
    }
    hide_task.set(Some(spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(HIDE_DELAY_MS)).await;
        js_api::page_indicator::hide::<DioxusBridge>();
    })));
}

/// 隐藏所有页码标签（初始状态）。
pub fn hide_all_page_numbers() {
    js_api::page_indicator::hide::<DioxusBridge>();
}

/// 交互事件处理：显示页码标签。
///
/// 用于 ontouchstart / ontouchend / onscroll / ondoubleclick 等事件。
pub fn on_preview_interact(hide_task: Signal<Option<Task>>) {
    show_page_numbers(hide_task);
}
