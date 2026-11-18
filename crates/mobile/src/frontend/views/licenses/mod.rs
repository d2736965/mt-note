//! 开源许可页：本应用依赖的第三方开源库清单与授权信息。
//!
//! 数据由工作区 `scripts/gen_licenses.py` 在构建期扫描（mobile/build.rs
//! 调用，白名单外许可门禁失败即编译失败），写入 `OUT_DIR/third_party.json`
//! 供本页 include_str!。上游附带全文的条目内嵌全文；未附带的条目给出
//! 公共许可证原文链接（spdx.org，经系统浏览器打开）。

use dioxus::logger::tracing;
use dioxus::prelude::*;
use dioxus_i18n::prelude::i18n;
use serde::Deserialize;

use crate::frontend::hooks::use_back_interceptor::use_back_interceptor;
use crate::frontend::hooks::use_top_app_bar_state::use_top_app_bar_state;
use crate::frontend::i18n as i18n_support;
use crate::frontend::store::storage::jni;
use ui::components::icons::ChevronRightIcon;

stylance::import_style!(styles, "licenses.module.scss");

/// 第三方许可清单（构建产物，由 build.rs 生成到 OUT_DIR，不入源码）。
const THIRD_PARTY_JSON: &str = include_str!(concat!(env!("OUT_DIR"), "/third_party.json"));

/// 一条依赖的许可信息：`text_idx` 指向全文池（上游附带文本的条目，数百
/// 依赖共享同一份文本）；未附带全文的条目以 `license_ids` 列出许可 id，
/// UI 据此给出公共许可证原文链接。
#[derive(Clone, Deserialize)]
struct LicenseEntry {
    name: String,
    version: String,
    license: String,
    #[serde(default)]
    repository: String,
    #[serde(default)]
    text_idx: Option<usize>,
    #[serde(default)]
    license_ids: Vec<String>,
}

/// 内嵌清单文档：全文池 + 条目列表。
#[derive(Clone, Deserialize)]
struct LicenseDoc {
    licenses: Vec<String>,
    entries: Vec<LicenseEntry>,
}

/// 解析内嵌的许可清单 JSON。
fn parse_entries() -> Result<LicenseDoc, String> {
    serde_json::from_str(THIRD_PARTY_JSON).map_err(|e| e.to_string())
}

/// 开源许可页：列表 → 点条目看授权全文；系统返回键先退出全文回列表。
#[component]
pub fn Licenses() -> Element {
    let i18n_ctx = i18n();
    if let Ok(mut app_bar) = use_top_app_bar_state() {
        app_bar.write().set_title(i18n_support::translate(&i18n_ctx, "about-oss-licenses"));
    }

    // 当前查看全文的条目下标；None = 列表态
    let mut selected: Signal<Option<usize>> = use_signal(|| None);
    let entries = use_resource(move || async move { parse_entries() });

    use_back_interceptor(selected().is_some(), move || selected.set(None));

    let state = entries();

    rsx! {
        div { class: styles::licenses_view,
            match &state {
                // 首帧解析中
                None => rsx! {
                    div { class: styles::hint, { i18n_support::translate(&i18n_ctx, "licenses-loading") } }
                },
                Some(Err(e)) => rsx! {
                    div { class: styles::hint, { i18n_support::translate_with(&i18n_ctx, "err-licenses-load", &[("e", e.clone().into())]) } }
                },
                Some(Ok(doc)) => match selected() {
                    // 授权全文视图
                    Some(idx) => match doc.entries.get(idx) {
                        Some(entry) => {
                            let entry = entry.clone();
                            let text = entry
                                .text_idx
                                .and_then(|i| doc.licenses.get(i))
                                .cloned()
                                .unwrap_or_default();
                            rsx! {
                                div { class: styles::detail,
                                    div { class: styles::detail_header,
                                        span { class: styles::detail_name, "{entry.name}" }
                                        span { class: styles::detail_version, "v{entry.version}" }
                                        span { class: styles::detail_license, "{entry.license}" }
                                    }
                                    if !entry.repository.is_empty() {
                                        button {
                                            class: styles::repo_row,
                                            onclick: move |_| {
                                                // 无浏览器等异常由原生层捕获记日志
                                                if let Err(e) = jni::open_url(&entry.repository) {
                                                    tracing::error!("open_url({}): {e}", entry.repository);
                                                }
                                            },
                                            span { class: styles::repo_value, "{entry.repository}" }
                                            ChevronRightIcon { class: styles::repo_chevron }
                                        }
                                    }
                                    if !text.is_empty() {
                                        pre { class: styles::license_text, "{text}" }
                                    } else if !entry.license_ids.is_empty() {
                                        // 上游未附带全文：给公共许可证原文链接
                                        div { class: styles::license_links,
                                            div { class: styles::link_hint,
                                                { i18n_support::translate(&i18n_ctx, "licenses-view-online") }
                                            }
                                            for id in entry.license_ids.iter().cloned() {
                                                button {
                                                    key: "{id}",
                                                    class: styles::license_link,
                                                    onclick: move |_| {
                                                        let url = format!("https://spdx.org/licenses/{id}.html");
                                                        // 无浏览器等异常由原生层捕获记日志
                                                        if let Err(e) = jni::open_url(&url) {
                                                            tracing::error!("open_url({url}): {e}");
                                                        }
                                                    },
                                                    span { class: styles::license_link_name, "{id}" }
                                                    ChevronRightIcon { class: styles::license_link_chevron }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        // 下标越界（数据更新后残留的旧选中态）：回列表
                        None => rsx! {
                            div { class: styles::hint,
                                { i18n_support::translate(&i18n_ctx, "licenses-empty") }
                            }
                        },
                    },
                    // 列表视图
                    None => rsx! {
                        div { class: styles::entry_list,
                            { i18n_support::translate(&i18n_ctx, "licenses-desc") }
                            for (idx, entry) in doc.entries.iter().enumerate() {
                                button {
                                    key: "{entry.name}-{entry.version}",
                                    class: styles::entry,
                                    onclick: move |_| selected.set(Some(idx)),
                                    div { class: styles::entry_main,
                                        span { class: styles::entry_name, "{entry.name}" }
                                        span { class: styles::entry_license, "{entry.license}" }
                                    }
                                    span { class: styles::entry_version, "v{entry.version}" }
                                    ChevronRightIcon { class: styles::entry_chevron }
                                }
                            }
                        }
                    },
                },
            }
        }
    }
}
