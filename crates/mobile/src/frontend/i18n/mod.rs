//! i18n 引导与取词辅助。
//!
//! 基于 dioxus-i18n（Project Fluent），翻译文件为 `crates/mobile/locales/*.ftl`：
//! 翻译者只需维护这个目录，**新增语言放入 `<langid>.ftl`（BCP-47 完整标签，
//! 如 `zh-Hans.ftl`）即自动注册**——build.rs 生成 [`LOCALE_FILES`] 注册表并
//! 强制键集一致，设置页语言选项与各端取词随目录自动增减，各语言经文件内
//! `language-self-name` 键自名。
//!
//! 取词约定：组件渲染体内经 `dioxus_i18n::prelude::i18n()` 捕获 `I18n`（Copy，
//! 可入 onclick 闭包），统一走 [`translate`] 取词，不使用 `t!` 宏（缺 id 会
//! panic）；取词失败回退 id 本身并记日志，由 build.rs 键集门禁兜底。

pub mod localize;

pub use localize::Localize;

use std::sync::OnceLock;

use dioxus::logger::tracing;
use dioxus_i18n::fluent::{FluentArgs, FluentValue};
use dioxus_i18n::prelude::{I18n, I18nConfig, use_init_i18n};
use dioxus_i18n::unic_langid::{langid, LanguageIdentifier};
use fluent_langneg::{
    NegotiationStrategy, convert_vec_str_to_langids, convert_vec_str_to_langids_lossy,
    negotiate_languages,
};
use models::LangPref;

use crate::frontend::store::storage::jni;

// 语言注册表：build.rs 从 locales/ 目录生成（语言标识 + 编译期嵌入的翻译内容）
include!(concat!(env!("OUT_DIR"), "/locales.gen.rs"));

/// 回退基准语言标签（build.rs 门禁保证 `locales/en.ftl` 存在）：语言协商
/// 无匹配、取词缺键的最终兜底。
const FALLBACK_TAG: &str = "en";

/// 初始化 i18n Context（仅根组件调用一次）。
///
/// 初始语言为回退基准（en，即 [`FALLBACK_TAG`]；langid! 宏只收字面量，值须
/// 与之一致）；真实语言由启动 effect 按「持久化偏好 → 系统语言」协商解析后
/// 经 [`set_language`] 应用（JNI 必须在首帧之后，见 app.rs 注释）。
pub fn use_init_app_i18n() -> I18n {
    use_init_i18n(|| {
        let mut config = I18nConfig::new(langid!("en")).with_fallback(langid!("en"));
        for (id, content) in LOCALE_FILES.iter() {
            config = config.with_locale((id.clone(), *content));
        }
        config
    })
}

/// 应用语言到 i18n Context，`tag` 为 BCP-47 标签（应来自 [`resolve_language`]，
/// 保证是注册表内的语言）。切换后所有渲染期读过 bundle 的组件自动重渲染。
pub fn set_language(i18n: &mut I18n, tag: &str) {
    match LanguageIdentifier::from_bytes(tag.as_bytes()) {
        Ok(id) => {
            if let Err(e) = i18n.try_set_language(id) {
                tracing::error!("set language {tag}: {e}");
            }
        }
        Err(e) => tracing::error!("set language `{tag}`: {e}"),
    }
}

/// 取词（无参数）。失败回退 id 本身并记日志——键集一致性由 build.rs 门禁保证，
/// 运行期失败只可能是键拼写错误，回退保证界面不 panic。
pub fn translate(i18n: &I18n, id: &str) -> String {
    match i18n.try_translate(id) {
        Ok(text) => text,
        Err(e) => {
            tracing::error!("translate `{id}`: {e}");
            id.to_string()
        }
    }
}

/// 取词（带 Fluent 插值参数），如 `&[("count", 3.into())]` 对应译文 `{$count}`。
pub fn translate_with(i18n: &I18n, id: &str, args: &[(&str, FluentValue)]) -> String {
    let mut fluent_args = FluentArgs::new();
    for (name, value) in args {
        fluent_args.set(*name, value.clone());
    }
    match i18n.try_translate_with_args(id, Some(&fluent_args)) {
        Ok(text) => text,
        Err(e) => {
            tracing::error!("translate `{id}`: {e}");
            id.to_string()
        }
    }
}

/// 错误包装文案取词：译文约定含 `{$e}` 占位（如「删除失败: {$e}」），
/// 错误详情当前为底层 Display 原文（英文）。
pub fn translate_err(i18n: &I18n, id: &str, e: &str) -> String {
    translate_with(i18n, id, &[("e", e.into())])
}

/// 语言选项列表 `(BCP-47 标签, 语言自名)`，注册表顺序（langid 字典序）。
/// 自名取自各翻译文件的 `language-self-name` 键——每个语言自我声明、固定原文
/// 不翻译；缺失或为空时回退 langid 本身。设置页选项由本列表动态生成。
pub fn locale_options() -> &'static [(String, String)] {
    static OPTIONS: OnceLock<Vec<(String, String)>> = OnceLock::new();
    OPTIONS.get_or_init(|| {
        LOCALE_FILES
            .iter()
            .map(|(id, content)| {
                let self_name = content
                    .lines()
                    .filter(|line| !line.starts_with('#'))
                    .find_map(|line| {
                        let (key, value) = line.split_once('=')?;
                        (key.trim() == "language-self-name").then(|| value.trim().to_string())
                    })
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| id.to_string());
                (id.to_string(), self_name)
            })
            .collect()
    })
}

/// 系统语言标签缓存（首次 JNI 读取后进程内复用）。
fn system_language_tag() -> &'static OnceLock<String> {
    static CACHE: OnceLock<String> = OnceLock::new();
    &CACHE
}

/// 读取系统语言（如 "zh-CN"），JNI 失败返回空串。
fn load_system_language() -> String {
    system_language_tag()
        .get_or_init(|| jni::get_system_language().unwrap_or_default())
        .clone()
}

/// 解析持久化偏好为最终语言标签（注册表内语言，经协商就近匹配）。
///
/// 显式选择与系统语言走同一条协商路：带地区的标签（如系统 "zh-CN"、历史
/// 数据里的旧标签 "zh"）都能匹配到最合适的已注册语言（zh-Hans）；无匹配或
/// 系统语言读取失败回退 [`FALLBACK_TAG`]。
pub fn resolve_language(pref: &LangPref) -> String {
    let requested = if pref.is_system() {
        load_system_language()
    } else {
        pref.as_str().to_string()
    };
    negotiate_tag(&requested)
}

/// 语言协商：请求标签按 Fluent 生态规则（fluent-langneg，CLDR likely subtags）
/// 匹配已注册语言。fluent-langneg 0.14 基于 icu 类型而 dioxus-i18n 基于unic-langid，
/// 两侧字符串进出转换，不互持类型。
fn negotiate_tag(tag: &str) -> String {
    if tag.is_empty() {
        return FALLBACK_TAG.to_string();
    }
    let requested = match convert_vec_str_to_langids([tag]) {
        Ok(ids) => ids,
        Err(e) => {
            tracing::error!("negotiate `{tag}`: {e:?}");
            return FALLBACK_TAG.to_string();
        }
    };
    let available = convert_vec_str_to_langids_lossy(
        LOCALE_FILES.iter().map(|(id, _)| id.to_string()),
    );
    negotiate_languages(
        &requested,
        &available,
        None,
        NegotiationStrategy::Filtering,
    )
    .into_iter()
    .next()
    .map(|id| id.to_string())
    .unwrap_or_else(|| FALLBACK_TAG.to_string())
}
