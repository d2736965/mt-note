//! 持久化偏好的加载与应用。
//!
//! 启动引导（app.rs）与备份恢复（设置页）共用的唯一入口：从 persist 存储
//! 读取全部偏好写入 AppState——所有 set_* 走 signal，主题/编辑器/浏览器等
//! UI 自动重渲染生效；最后按持久化语言偏好应用 i18n（Rust 词典 + TS 侧
//! 词典同步）。

use dioxus::prelude::*;
use dioxus_i18n::prelude::I18n;
use models::{BrowserPrefs, EditorPrefs, GeneralPrefs, OpenMode, StartupTab, Theme, TypstPrefs};
use ui::store::app_state::AppState;

use crate::frontend::i18n as i18n_support;
use crate::frontend::js_bridge::{DioxusBridge, js_api};
use crate::frontend::store::storage::android::AndroidStorage;

/// 从持久化存储加载全部偏好并应用到运行态。
pub fn apply_persisted_prefs(
    state: &mut Signal<AppState>,
    storage: &AndroidStorage,
    i18n_ctx: &mut I18n,
) {
    if let Ok(Some(persisted)) = storage.load::<Theme>() {
        state.write().set_theme(persisted);
    }
    if let Ok(Some(prefs)) = storage.load::<TypstPrefs>() {
        state.write().set_typst_prefs(prefs);
    }
    if let Ok(Some(prefs)) = storage.load::<EditorPrefs>() {
        state.write().set_editor_prefs(prefs);
    }
    if let Ok(Some(mode)) = storage.load::<OpenMode>() {
        state.write().set_open_mode(mode);
    }
    if let Ok(Some(prefs)) = storage.load::<BrowserPrefs>() {
        // 应用持久化的默认目录：仅当 current_dir 仍为初始默认值时覆盖，
        // 避免覆盖用户在本次会话中已导航到的新目录。
        if let Some(dir) = &prefs.default_dir
            && !dir.is_empty()
        {
            state.write().set_current_dir(dir.into());
        }
        state.write().set_browser_prefs(prefs);
    }
    if let Ok(Some(prefs)) = storage.load::<GeneralPrefs>() {
        // 启动页偏好仅在冷启动时应用；会话内切换 tab 不回写。
        let tab = match prefs.startup_tab {
            StartupTab::Files => 0,
            StartupTab::Favorites => 1,
        };
        state.write().set_active_tab(tab);
        state.write().set_general_prefs(prefs);
    }
    // 界面语言：显式选择优先，System 跟随系统（JNI 读取，进程内缓存），
    // 经协商匹配到已注册语言。
    let lang = i18n_support::resolve_language(&state.read().general_prefs().language);
    i18n_support::set_language(i18n_ctx, &lang);
    // TS 侧词典同步（markdown 工具条/灯箱等 JS 动态文案）
    js_api::i18n::set_locale::<DioxusBridge>(&lang);
}
