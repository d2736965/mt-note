//! 错误展示翻译：[`Localize`] trait 按错误变体取词。
//!
//! 底层 crate（fs-engine / typst-engine）的错误 Display 统一英文（开发者
//! 语言/兜底语言，actor 层只传 `to_string()` 原文，无 i18n 依赖）；UI 展示
//! 点对本 crate 外来类型实现 [`Localize`]（孤儿规则允许），渲染期经 I18n
//! 按变体翻译，未覆盖的变体回退 Display 原文，双语界面均无混排。

use std::fmt::Display;

use dioxus_i18n::prelude::I18n;
use mt_note_fs_engine::FsError;
use mt_note_typst_engine::TypstEngineError;

use super::translate_with;

/// 可翻译为界面语言的错误。任何 `Display` 类型自动获得回退能力
/// （原文英文），需要本地化的错误类型逐变体覆盖 [`Localize::localize`]。
pub trait Localize: Display {
    /// 翻译为当前界面语言；默认回退 Display 原文。
    fn localize(&self, _i18n: &I18n) -> String {
        self.to_string()
    }
}

impl Localize for FsError {
    fn localize(&self, i18n: &I18n) -> String {
        match self {
            Self::Walk { path, source } => translate_with(
                i18n,
                "err-fs-walk",
                &[
                    ("path", path.display().to_string().into()),
                    ("source", source.to_string().into()),
                ],
            ),
            Self::Io { path, source } => translate_with(
                i18n,
                "err-fs-io",
                &[
                    ("path", path.display().to_string().into()),
                    ("source", source.to_string().into()),
                ],
            ),
            Self::NotFound(path) => translate_with(
                i18n,
                "err-fs-not-found",
                &[("path", path.display().to_string().into())],
            ),
            Self::NotDir(path) => translate_with(
                i18n,
                "err-fs-not-dir",
                &[("path", path.display().to_string().into())],
            ),
            Self::AlreadyExists(path) => translate_with(
                i18n,
                "err-fs-already-exists",
                &[("path", path.display().to_string().into())],
            ),
            Self::InvalidInput(msg) => {
                translate_with(i18n, "err-fs-invalid-input", &[("msg", msg.clone().into())])
            }
        }
    }
}

impl Localize for TypstEngineError {
    fn localize(&self, i18n: &I18n) -> String {
        match self {
            Self::Compile(msg) => {
                translate_with(i18n, "err-typst-compile-failed", &[("msg", msg.clone().into())])
            }
            Self::FileNotFound(path) => translate_with(
                i18n,
                "err-typst-file-not-found",
                &[("path", path.display().to_string().into())],
            ),
            Self::Io { path, source } => translate_with(
                i18n,
                "err-typst-io",
                &[
                    ("path", path.display().to_string().into()),
                    ("source", source.to_string().into()),
                ],
            ),
            Self::FontLoad(msg) => {
                translate_with(i18n, "err-typst-font-load", &[("msg", msg.clone().into())])
            }
        }
    }
}
