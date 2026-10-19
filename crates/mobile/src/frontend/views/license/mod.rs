//! 开源许可证全文页。

use dioxus::prelude::*;
use dioxus_i18n::prelude::i18n;

use crate::frontend::hooks::use_top_app_bar_state::use_top_app_bar_state;
use crate::frontend::i18n as i18n_support;

stylance::import_style!(styles, "license.module.scss");

const LICENSE_MIT: &str = include_str!("../../../../../../LICENSE-MIT");
const LICENSE_APACHE: &str = include_str!("../../../../../../LICENSE-APACHE");

/// 许可证类别。
#[derive(Clone, Copy, Debug, PartialEq)]
enum LicenseKind {
    Mit,
    Apache,
}

impl LicenseKind {
    /// 切换按钮标签。
    fn label(self) -> &'static str {
        match self {
            Self::Mit => "MIT",
            Self::Apache => "Apache-2.0",
        }
    }

    /// 许可证全文。
    fn text(self) -> &'static str {
        match self {
            Self::Mit => LICENSE_MIT,
            Self::Apache => LICENSE_APACHE,
        }
    }
}

const ALL_KINDS: [LicenseKind; 2] = [LicenseKind::Mit, LicenseKind::Apache];

/// 许可证全文页：MIT / Apache-2.0 双标签切换。
#[component]
pub fn License() -> Element {
    let i18n_ctx = i18n();
    if let Ok(mut app_bar) = use_top_app_bar_state() {
        app_bar.write().set_title(i18n_support::translate(&i18n_ctx, "common-license"));
    }

    let mut current = use_signal(|| LicenseKind::Mit);

    rsx! {
        div { class: styles::license_view,
            div { class: styles::kind_switcher,
                for kind in ALL_KINDS {
                    button {
                        key: "{kind:?}",
                        class: if current() == kind { stylance::classes!(styles::kind_btn, styles::active) } else { stylance::classes!(styles::kind_btn) },
                        onclick: move |_| current.set(kind),
                        "{kind.label()}"
                    }
                }
            }
            pre { class: styles::license_text, "{current().text()}" }
        }
    }
}
