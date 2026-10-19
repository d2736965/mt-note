//! Typst World 实现。
//!
//! [`TypstWorld`] 实现 `typst_library::World` trait，
//! 使用 `typst-kit` 的 `FontStore` 和 `FileStore` 管理字体和文件。
//!
//! 性能优化：
//! - **增量编译**：`TypstWorld` 可复用，`recompile` 重置时间后重新编译，
//!   comemo 缓存自动复用未变更的计算结果。
//! - **并行 SVG 渲染**：多页文档使用 rayon 并行生成 SVG 和文字层。
//! - **编译线程隔离**：前端应通过 `spawn_blocking` 调用编译函数，
//!   避免阻塞 tokio 异步运行时。

use std::any::Any;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use ecow::EcoVec;
use rayon::prelude::*;
use typst::LibraryExt;
use typst::diag::{FileResult, Warned};
use typst::utils::LazyHash;
use typst_layout::Page;
use typst_kit::files::{FileStore, FsRoot};
use typst_kit::fonts::FontStore;
use typst_kit::packages::{FsPackages, SystemPackages, UniversePackages};
use typst_layout::PagedDocument;
use typst_library::World;
use typst_library::foundations::{Bytes, Datetime, Duration};
use typst_library::text::{Font, FontBook};
use typst_syntax::{DiagSpanKind, FileId, RootedPath, Source, VirtualPath, VirtualRoot};

use crate::TypstEngineError;
use crate::text_layer::{FaceCollector, FontFace, TextLayerItem, render_text_layer_html};

pub struct TypstWorld {
    library: LazyHash<typst_library::Library>,
    fonts: FontStore,
    files: FileStore<TypstFileLoader>,
    main: FileId,
    now: typst_kit::datetime::Time,
    source_content: Option<String>,
}

impl TypstWorld {
    pub fn new(
        root: PathBuf,
        main_path: &Path,
        font_paths: &[PathBuf],
        cache_dir: Option<PathBuf>,
    ) -> Result<Self, TypstEngineError> {
        let library = typst_library::Library::default();
        let fonts = discover_fonts(font_paths);
        let vpath = VirtualPath::new(main_path.to_string_lossy())
            .map_err(|_| TypstEngineError::FileNotFound(main_path.to_path_buf()))?;
        let rooted = RootedPath::new(VirtualRoot::Project, vpath);
        let main = FileId::new(rooted);
        let loader = TypstFileLoader::new(root, cache_dir);
        let files = FileStore::new(loader);
        let now = typst_kit::datetime::Time::system();

        Ok(Self {
            library: LazyHash::new(library),
            fonts,
            files,
            main,
            now,
            source_content: None,
        })
    }

    pub fn main_id(&self) -> FileId {
        self.main
    }

    pub fn set_source_content(&mut self, content: String) {
        self.source_content = Some(content);
    }

    pub fn recompile(&mut self) -> Result<PagedDocument, TypstEngineError> {
        self.now.reset();
        compile_document(self)
    }
}

impl World for TypstWorld {
    fn library(&self) -> &LazyHash<typst_library::Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        self.fonts.book()
    }

    fn main(&self) -> FileId {
        self.main
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main
            && let Some(ref content) = self.source_content
        {
            return Ok(Source::new(id, content.clone()));
        }
        self.files.source(id)
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.files.file(id)
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.font(index)
    }

    fn today(&self, offset: Option<Duration>) -> Option<Datetime> {
        self.now.today(offset)
    }
}

pub fn compile_document(world: &TypstWorld) -> Result<PagedDocument, TypstEngineError> {
    let Warned {
        output,
        warnings: _,
    } = typst::compile::<PagedDocument>(world);
    let doc = output.map_err(|errors| {
        let msg = format_errors(&errors);
        TypstEngineError::Compile(msg)
    })?;

    Ok(doc)
}

pub fn compile_svg(world: &TypstWorld) -> Result<String, TypstEngineError> {
    let doc = compile_document(world)?;
    Ok(render_svg_string(doc.pages()))
}

pub fn compile_svg_with_text_layer(
    world: &TypstWorld,
) -> Result<(String, String, Option<ContentRect>), TypstEngineError> {
    let doc = compile_document(world)?;
    Ok(render_svg_with_text_layer(&doc))
}

pub fn render_svg_string(pages: &[typst_layout::Page]) -> String {
    let opts = typst_svg::SvgOptions::default();
    let multi = pages.len() > 1;

    let svgs: Vec<String> = pages
        .par_iter()
        .enumerate()
        .map(|(i, page)| {
            let svg = typst_svg::svg(page, &opts);
            if multi {
                format!(
                    r#"<div class="typst-page" data-page="{}">{svg}</div>"#,
                    i + 1
                )
            } else {
                svg
            }
        })
        .collect();

    svgs.join("\n")
}

pub fn render_svg_with_text_layer(doc: &PagedDocument) -> (String, String, Option<ContentRect>) {
    let pages = doc.pages();
    let opts = typst_svg::SvgOptions::default();
    let multi = pages.len() > 1;

    // 每页并行提取 items + 局部运行时字体收集器，归并出文档级 @font-face
    // 子集（同字体跨页共享一个 face，base64 只随第一个 text-page 注入）。
    let extracted: Vec<(Vec<TextLayerItem>, FaceCollector)> = pages
        .par_iter()
        .map(|page| crate::text_layer::extract_frame_text(&page.frame))
        .collect();

    let mut merged = FaceCollector::default();
    for (_, c) in &extracted {
        merged.merge(c);
    }
    let faces = merged.finish();

    let results: Vec<(String, String)> = pages
        .par_iter()
        .enumerate()
        .map(|(i, page)| {
            let svg = typst_svg::svg(page, &opts);
            let page_faces: &[FontFace] = if i == 0 { &faces } else { &[] };
            let text_layer =
                render_text_layer_html(&extracted[i].0, page.frame.size(), page_faces);
            if multi {
                (
                    format!(
                        r#"<div class="typst-page" data-page="{}">{svg}</div>"#,
                        i + 1
                    ),
                    format!(
                        r#"<div class="typst-text-page" data-page="{}">{text_layer}</div>"#,
                        i + 1
                    ),
                )
            } else {
                (svg, text_layer)
            }
        })
        .collect();

    let svgs: Vec<String> = results.iter().map(|(s, _)| s.clone()).collect();
    let texts: Vec<String> = results.iter().map(|(_, t)| t.clone()).collect();
    // 跨所有页合成内容区：取每页内容包围盒的最小 left/top、最大 right/bottom。
    // 这样吸附目标基于"所有页中内容最靠左/右的边界"，不依赖单页内容稀疏程度，
    // 也不假设各页 margin 完全相同。
    let content_rect = {
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        let mut any = false;
        for c in pages.iter().filter_map(extract_content_rect) {
            min_x = min_x.min(c.left);
            min_y = min_y.min(c.top);
            max_x = max_x.max(c.left + c.width);
            max_y = max_y.max(c.top + c.height);
            any = true;
        }
        any.then_some(ContentRect {
            left: min_x,
            top: min_y,
            width: max_x - min_x,
            height: max_y - min_y,
        })
    };
    (svgs.join("\n"), texts.join("\n"), content_rect)
}

/// 内容区矩形（pt 坐标，原点为整页左上角）。
///
/// 用于"内容适应"缩放模式：纸张去掉 margin 后文字所在的区域。
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ContentRect {
    /// 内容区左上角 x（= margin.left）。
    pub left: f64,
    /// 内容区左上角 y（= margin.top）。
    pub top: f64,
    /// 内容区宽度（= 整页宽 - margin.left - margin.right）。
    pub width: f64,
    /// 内容区高度（= 整页高 - margin.top - margin.bottom）。
    pub height: f64,
}

/// 反推页面的内容区（inner frame）。
///
/// `typst` 的 `Page` 不直接暴露 margin，只能从顶层 item 的包围盒反推。
/// 排版产物中段落文字行与图片都不是 Group，文档唯一的 Group 可能是宽度
/// 只有标题文字实宽的 heading——只统计 Group 会把内容区推断成标题宽度，
/// 令「内容适应」放大到异常倍数。这里统计全部可见 item（Group/Text/Image/
/// Shape）的并集：段落行两端对齐撑满内容区时，并集宽度即内容区宽度，
/// 正是「内容适应」应对齐的目标。覆盖整页的无描边填充矩形是页面背景，
/// 不是内容，予以排除；Link/Tag 无可见边界，跳过。
/// 推断不出（空白页/异常结构）时返回 `None`，调用方应退化为整页尺寸。
fn extract_content_rect(page: &Page) -> Option<ContentRect> {
    use typst_library::layout::FrameItem;

    let page_w = page.frame.size().x.to_pt();
    let page_h = page.frame.size().y.to_pt();
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut any = false;
    for (pos, item) in page.frame.items() {
        // 各类 item 的局部包围盒（相对 pos 的 pt 端点坐标）
        let local = match item {
            FrameItem::Group(group) => {
                let size = group.frame.size();
                Some((0.0, 0.0, size.x.to_pt(), size.y.to_pt()))
            }
            // pos 是基线左端，高度用字号近似（消费方只使用宽度）
            FrameItem::Text(text) => {
                Some((0.0, 0.0, text.width().to_pt(), text.size.to_pt()))
            }
            FrameItem::Image(_, size, _) => {
                Some((0.0, 0.0, size.x.to_pt(), size.y.to_pt()))
            }
            FrameItem::Shape(shape, _) => {
                let bb = shape.bbox(false);
                let full_page = bb.min.x.to_pt().abs() < 0.5
                    && bb.min.y.to_pt().abs() < 0.5
                    && (bb.max.x.to_pt() - page_w).abs() < 0.5
                    && (bb.max.y.to_pt() - page_h).abs() < 0.5;
                if full_page {
                    None
                } else {
                    Some((
                        bb.min.x.to_pt(),
                        bb.min.y.to_pt(),
                        bb.max.x.to_pt(),
                        bb.max.y.to_pt(),
                    ))
                }
            }
            FrameItem::Link(..) | FrameItem::Tag(_) => None,
        };
        if let Some((lx0, ly0, lx1, ly1)) = local {
            min_x = min_x.min(pos.x.to_pt() + lx0);
            min_y = min_y.min(pos.y.to_pt() + ly0);
            max_x = max_x.max(pos.x.to_pt() + lx1);
            max_y = max_y.max(pos.y.to_pt() + ly1);
            any = true;
        }
    }
    if !any {
        return None;
    }
    Some(ContentRect {
        left: min_x,
        top: min_y,
        width: max_x - min_x,
        height: max_y - min_y,
    })
}

fn discover_fonts(font_paths: &[PathBuf]) -> FontStore {
    let mut store = FontStore::new();

    #[cfg(feature = "embedded-fonts")]
    {
        store.extend(typst_kit::fonts::embedded());
    }

    for path in font_paths {
        store.extend(typst_kit::fonts::scan(path));
    }

    #[cfg(feature = "scan-fonts")]
    {
        #[cfg(target_os = "linux")]
        store.extend(typst_kit::fonts::system());
        #[cfg(target_os = "macos")]
        store.extend(typst_kit::fonts::system());
        #[cfg(target_os = "windows")]
        store.extend(typst_kit::fonts::system());
    }

    store
}

pub(crate) fn format_errors(errors: &EcoVec<typst::diag::SourceDiagnostic>) -> String {
    errors
        .iter()
        .map(|e| {
            let msg = e.message.to_string();
            match e.span.get() {
                DiagSpanKind::Detached => msg,
                DiagSpanKind::Number { id, num, .. } => {
                    format!("{id:?}:{num:?}: {msg}")
                }
                DiagSpanKind::Range { id, range } => {
                    format!("{id:?}:{}-{}: {msg}", range.start, range.end)
                }
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub struct TypstFileLoader {
    root: FsRoot,
    packages: SystemPackages,
}

impl TypstFileLoader {
    pub fn new(root: PathBuf, cache_dir: Option<PathBuf>) -> Self {
        let fs_root = FsRoot::new(root);
        let packages = {
            let downloader = HttpDownloader;
            let cache = cache_dir
                .or_else(dirs::cache_dir)
                .unwrap_or_else(std::env::temp_dir);
            SystemPackages::from_parts(
                None,
                Some(FsPackages::new(cache.join("typst").join("packages"))),
                UniversePackages::new(downloader),
            )
        };
        Self {
            root: fs_root,
            packages,
        }
    }
}

impl typst_kit::files::FileLoader for TypstFileLoader {
    fn load(&self, id: FileId) -> FileResult<Bytes> {
        match id.root() {
            VirtualRoot::Project => self.root.load(id.vpath()),
            VirtualRoot::Package(spec) => {
                let root = self.packages.obtain(spec)?;
                root.load(id.vpath())
            }
        }
    }
}

struct HttpDownloader;

impl typst_kit::downloader::Downloader for HttpDownloader {
    fn stream(&self, _key: &dyn Any, url: &str) -> io::Result<(Option<usize>, Box<dyn Read>)> {
        let response = ureq::get(url).call().map_err(|err| match &err {
            ureq::Error::Status(404, _) => io::Error::new(io::ErrorKind::NotFound, err),
            _ => io::Error::other(err),
        })?;
        let content_len: Option<usize> = response
            .header("Content-Length")
            .and_then(|h| h.parse().ok());
        let reader: Box<dyn Read> = Box::new(response.into_reader());
        Ok((content_len, reader))
    }
}
