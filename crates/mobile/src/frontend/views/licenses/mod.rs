//! 开源许可页：本应用依赖的第三方开源库清单与授权全文。
//!
//! 数据由 `cargo about`（配置见工作区根 `about.toml`）对 Rust 依赖生成，
//! 并入 JS 运行时依赖（pdf.js、katex 等）后固化为 `third_party.json`，
//! 依赖变化时重新生成即可，构建不依赖 cargo-about。

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

/// 第三方许可清单（构建期固化的 JSON 数组，随源码提交）。
const THIRD_PARTY_JSON: &str = include_str!("third_party.json");

/// 一条依赖的许可信息（全文在 `licenses[text_idx]`，数百依赖共享同一份文本）。
#[derive(Clone, Deserialize)]
struct LicenseEntry {
    name: String,
    version: String,
    license: String,
    #[serde(default)]
    repository: String,
    text_idx: usize,
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
                            let text = doc
                                .licenses
                                .get(entry.text_idx)
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
                                    pre { class: styles::license_text, "{text}" }
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
