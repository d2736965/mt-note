// deny：Dioxus 宏内部生成 #[allow] 导致冲突，无法使用 forbid
#![deny(
    unused_must_use,   // #[component] 展开包含 #[allow(unused)]
    clippy::panic,     // #[derive(Props)] 展开包含 #[allow(clippy::panic)]
)]
// forbid：无法被 #[allow]/#[expect] 覆盖，零例外
#![forbid(
    clippy::let_underscore_must_use, // 禁止 let _ = 忽略 #[must_use]，必须用 if let Err
    clippy::unwrap_used,             // 禁止 .unwrap()，必须显式处理错误
    clippy::expect_used,             // 禁止 .expect()，必须显式处理错误
    clippy::disallowed_types,        // 启用 clippy.toml disallowed-types 清单
    clippy::disallowed_methods,      // 启用 clippy.toml disallowed-methods 清单
    clippy::allow_attributes,        // 禁止 #[allow]，强制 #[expect] 并附注原因
    unfulfilled_lint_expectations,   // #[expect] 不再触发时报错，提醒移除
)]

pub mod components;
pub mod hooks;
pub mod store;

use manganis::asset;

pub const STYLES: manganis::Asset = asset!("/assets/styles/dist/bundle-ui.scss");
