//! 语言偏好。
//!
//! 语言集合完全数据驱动：**`crates/mobile/locales/` 有几个翻译文件，设置页
//! 就有几个语言选项**，此处不做语言枚举登记。新增语言只需放入
//! `<langid>.ftl`（BCP-47 完整标签命名，如 `zh-Hans.ftl`、`pt-BR.ftl`），
//! 文件内用 `language-self-name` 键自名（如 `language-self-name = 简体中文`）；
//! 注册、键集门禁、设置页选项、各端取词全部自动跟随。
//!
//! `en` 是回退基准语言（缺失时 mobile/build.rs 构建失败）：语言协商无匹配、
//! 各端取词缺键都回退到它。

use serde::{Deserialize, Serialize};

/// 语言偏好（持久化值）。
///
/// `"system"` 表示跟随系统语言（默认值，未显式选择时随系统变化）；其余取值
/// 是 BCP-47 语言标签，与 `locales/<标签>.ftl` 文件名一致，如 `"zh-Hans"`。
/// 历史数据里的旧标签（如早期的 `"zh"`）无需迁移——语言协商会就近匹配到
/// 现有语言。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LangPref(String);

impl LangPref {
    /// 跟随系统语言。
    pub fn system() -> Self {
        Self("system".to_string())
    }

    /// 显式语言，`tag` 为 BCP-47 标签（与 `locales/<tag>.ftl` 文件名一致）。
    pub fn from_tag(tag: &str) -> Self {
        Self(tag.to_string())
    }

    /// 偏好值的字符串形式（`"system"` 或 BCP-47 标签）。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 是否跟随系统（未显式选择）。
    pub fn is_system(&self) -> bool {
        self.0 == "system"
    }
}

impl Default for LangPref {
    fn default() -> Self {
        Self::system()
    }
}
