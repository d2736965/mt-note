//! 文件类型分类。

use std::path::Path;

/// 文件分类，决定用哪种引擎打开。
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileCategory {
    /// Typst 文档 (.typ)
    Typst,
    /// Markdown 文档 (.md, .markdown)
    Markdown,
    /// PDF 文档 (.pdf)，仅预览
    Pdf,
    /// SVG 矢量图 (.svg)，仅预览
    Svg,
    /// 光栅图片文件 (png/jpg/jpeg/gif/webp/bmp)
    Image,
    /// 不支持的文件类型
    Unknown,
}

impl FileCategory {
    /// 根据文件扩展名推断分类。
    pub fn from_path(path: &Path) -> Self {
        match path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref()
        {
            Some("typ") => Self::Typst,
            Some("md" | "markdown") => Self::Markdown,
            Some("pdf") => Self::Pdf,
            Some("svg") => Self::Svg,
            Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp") => Self::Image,
            _ => Self::Unknown,
        }
    }

    /// 是否为可编辑的文档类型。
    pub fn is_editable(self) -> bool {
        matches!(self, Self::Typst | Self::Markdown | Self::Svg)
    }

    /// 是否为可预览的文档类型。
    pub fn is_previewable(self) -> bool {
        matches!(self, Self::Typst | Self::Markdown | Self::Pdf | Self::Svg)
    }

    /// 显示名称。
    pub fn label(self) -> &'static str {
        match self {
            Self::Typst => "Typst",
            Self::Markdown => "Markdown",
            Self::Pdf => "PDF",
            Self::Svg => "SVG",
            Self::Image => "Image",
            Self::Unknown => "Unknown",
        }
    }
}

/// 根据文件内容判断是否为文本文件。
///
/// 读取文件前 8KB，使用 `infer` 基于内容（magic bytes）检测已知格式。
/// `infer` 仅用于正向确认 `text/*` 类型；对于其他格式（包括
/// `application/json`、`application/vnd.apple.mpegurl` 等），
/// 继续通过 null 字节检测判断是否为文本。
/// 不会将已知二进制格式（图片、PDF、压缩包等）误判为文本。
pub async fn is_text_file(path: &Path) -> Result<bool, std::io::Error> {
    use tokio::io::AsyncReadExt;
    let mut file = tokio::fs::File::open(path).await?;
    let mut buf = vec![0u8; 8192];
    let n = file.read(&mut buf).await?;
    buf.truncate(n);

    // infer 只用于正向确认 text/* 类型
    if let Some(kind) = infer::get(&buf)
        && kind.mime_type().starts_with("text/")
    {
        return Ok(true);
    }

    // 不包含 null 字节 → 文本文件
    Ok(!buf.contains(&0))
}