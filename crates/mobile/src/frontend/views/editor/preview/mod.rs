//! 文件预览组件。
//!
//! 根据文件类型选择对应的预览引擎渲染内容。

use dioxus::prelude::*;
use mt_note_fs_engine::FileCategory;

use crate::frontend::views::editor::preview::markdown_preview::MarkdownPreview;
use crate::frontend::views::editor::preview::pdf_preview::PdfPreview;
use crate::frontend::views::editor::preview::svg_preview::SvgPreview;
use crate::frontend::views::editor::preview::typst_preview::TypstPreview;
use crate::frontend::views::editor::preview::unsupported_preview::UnsupportedPreview;

pub mod markdown_preview;
pub mod page_indicator;
pub mod pdf_preview;
pub mod svg_preview;
pub mod typst_preview;
pub mod unsupported_preview;

/// 「内容适应」viewport 策略（Typst/PDF 预览共享）。
pub(crate) struct ContentFitViewport;

impl ContentFitViewport {
    /// viewport 宽度下限（占纸宽比例，任何纸张通用）：内容正常宽度
    /// = 纸宽 × (1 − 2×2.5/21)（默认边距为纸宽的 2.5/21），下限取其
    /// 95% ≈ 0.7238。暂定值，调整只改这里。
    const MIN_VIEWPORT_FRACTION: f64 = 0.723_8;

    /// 内容包围盒对应的 viewport 宽度（与 page_w 同单位）：
    /// 内容宽达下限视为撑满内容区，viewport 即内容宽；偏窄时取
    /// 「内容宽 + 较小页边距」（小边距侧纸边到内容远缘），仍不足下限
    /// 则取下限，保证内容不越出 viewport 且不过度放大。
    pub(crate) fn width(content_w: f64, content_left: f64, page_w: f64) -> f64 {
        let floor_w = Self::MIN_VIEWPORT_FRACTION * page_w;
        if content_w >= floor_w {
            content_w
        } else {
            let min_margin = content_left.min(page_w - content_left - content_w);
            (content_w + min_margin).max(floor_w)
        }
    }
}

/// 文件预览组件。
///
/// 根据文件类型选择对应的预览引擎：
/// - Typst：编译为 SVG 渲染
/// - Markdown：渲染 HTML + 代码高亮 + KaTeX
/// - PDF：pdf.js 渲染到 Canvas
/// - SVG：内联 DOM 渲染，支持缩放手势
#[component]
pub fn Preview(category: FileCategory, file_path: String, content: Signal<String>) -> Element {
    match category {
        FileCategory::Typst => rsx! {
            TypstPreview { file_path, content }
        },
        FileCategory::Markdown => rsx! {
            MarkdownPreview { content, file_path: Some(file_path) }
        },
        FileCategory::Pdf => rsx! {
            PdfPreview { file_path }
        },
        FileCategory::Svg => rsx! {
            SvgPreview { file_path }
        },
        _ => rsx! {
            UnsupportedPreview {}
        },
    }
}
