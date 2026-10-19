use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use kameo::prelude::*;
use tokio::task;

use crate::world::{TypstWorld, render_svg_with_text_layer};

static COMPILER: OnceLock<ActorRef<TypstCompiler>> = OnceLock::new();

/// 获取全局 `TypstCompiler` Actor 引用。
///
/// 使用 `OnceLock` 确保只初始化一次，Actor 在应用生命周期内持续存活。
/// 首次调用必须在 tokio runtime 上下文中（`compile_typst` 为 async fn，满足此条件）。
pub fn get_compiler() -> ActorRef<TypstCompiler> {
    COMPILER
        .get_or_init(|| {
            TypstCompiler::spawn(TypstCompiler {
                world: None,
                current_file: None,
            })
        })
        .clone()
}

/// Typst 编译 Actor。
///
/// 持有 `TypstWorld` 实例，通过消息驱动的 `compile` 方法执行编译。
/// Actor 串行处理消息，天然无锁。`TypstWorld` 在消息间持续存活，
/// comemo 缓存累积，实现真正的增量编译。
#[derive(Actor)]
pub struct TypstCompiler {
    world: Option<TypstWorld>,
    current_file: Option<String>,
}

/// 编译请求消息（由 kameo `#[message]` 宏自动生成）。
// 注意：消息结构体由 `#[message]` 宏根据方法参数自动生成，无需手动定义。

#[messages]
impl TypstCompiler {
    /// 编译 Typst 文件，返回 SVG、文字层 HTML 和内容区矩形。
    ///
    /// 若文件路径与上次编译相同，复用 `TypstWorld` 执行增量编译；
    /// 否则创建新的 `TypstWorld` 实例。
    /// 编译在 `spawn_blocking` 中执行，不阻塞 tokio worker 线程。
    #[message]
    pub async fn compile_typst(
        &mut self,
        file_path: String,
        font_paths: Vec<PathBuf>,
    ) -> Result<(String, String, Option<crate::ContentRect>), String> {
        self.compile_impl(file_path, font_paths, None, None).await
    }

    /// 编译 Typst 内容（直接传入源码字符串），返回 SVG、文字层 HTML 和内容区矩形。
    #[message]
    pub async fn compile_typst_content(
        &mut self,
        file_path: String,
        font_paths: Vec<PathBuf>,
        content: String,
        cache_dir: Option<PathBuf>,
    ) -> Result<(String, String, Option<crate::ContentRect>), String> {
        self.compile_impl(file_path, font_paths, Some(content), cache_dir)
            .await
    }

    /// 编译 Typst 内容并导出 PDF，返回 PDF 字节流。
    #[message]
    pub async fn compile_typst_pdf(
        &mut self,
        file_path: String,
        font_paths: Vec<PathBuf>,
        content: String,
        cache_dir: Option<PathBuf>,
    ) -> Result<Vec<u8>, String> {
        self.compile_pdf_impl(file_path, font_paths, content, cache_dir)
            .await
    }
}

impl TypstCompiler {
    async fn compile_impl(
        &mut self,
        file_path: String,
        font_paths: Vec<PathBuf>,
        content: Option<String>,
        cache_dir: Option<PathBuf>,
    ) -> Result<(String, String, Option<crate::ContentRect>), String> {
        let path = PathBuf::from(&file_path);
        let root = path
            .parent()
            .ok_or_else(|| "failed to get parent directory".to_string())?
            .to_path_buf();
        let main = path
            .file_name()
            .ok_or_else(|| "failed to get file name".to_string())?;
        let main_path = Path::new(main);

        let need_new = match &self.current_file {
            Some(current) => *current != file_path,
            None => true,
        };

        if need_new {
            let new_world = TypstWorld::new(root, main_path, &font_paths, cache_dir)
                .map_err(|e| format!("failed to create Typst world: {e}"))?;
            self.world = Some(new_world);
            self.current_file = Some(file_path);
        }

        let mut world = self
            .world
            .take()
            .ok_or_else(|| "Typst world not initialized".to_string())?;

        if let Some(ref c) = content {
            world.set_source_content(c.clone());
        }

        let (returned_world, result) = task::spawn_blocking(move || {
            let doc_result = world.recompile();
            match doc_result {
                Ok(doc) => {
                    let output = render_svg_with_text_layer(&doc);
                    (world, Ok(output))
                }
                Err(e) => (world, Err(format!("Typst compilation failed: {e}"))),
            }
        })
        .await
        .map_err(|e| format!("compile task failed: {e}"))?;

        self.world = Some(returned_world);
        result
    }

    /// 编译并导出 PDF 的实现。复用 compile_impl 的 World 创建逻辑，
    /// 但在 spawn_blocking 里调用 typst_pdf::pdf 而非 render_svg_with_text_layer。
    async fn compile_pdf_impl(
        &mut self,
        file_path: String,
        font_paths: Vec<PathBuf>,
        content: String,
        cache_dir: Option<PathBuf>,
    ) -> Result<Vec<u8>, String> {
        let path = PathBuf::from(&file_path);
        let root = path
            .parent()
            .ok_or_else(|| "failed to get parent directory".to_string())?
            .to_path_buf();
        let main = path
            .file_name()
            .ok_or_else(|| "failed to get file name".to_string())?;
        let main_path = Path::new(main);

        let need_new = match &self.current_file {
            Some(current) => *current != file_path,
            None => true,
        };

        if need_new {
            let new_world = TypstWorld::new(root, main_path, &font_paths, cache_dir)
                .map_err(|e| format!("failed to create Typst world: {e}"))?;
            self.world = Some(new_world);
            self.current_file = Some(file_path);
        }

        let mut world = self
            .world
            .take()
            .ok_or_else(|| "Typst world not initialized".to_string())?;
        world.set_source_content(content);

        let (returned_world, result) = task::spawn_blocking(move || {
            let doc_result = world.recompile();
            match doc_result {
                Ok(doc) => {
                    let opts = typst_pdf::PdfOptions::default();
                    let pdf_result = typst_pdf::pdf(&doc, &opts);
                    match pdf_result {
                        Ok(bytes) => (world, Ok(bytes)),
                        Err(e) => (world, Err(format!("PDF export failed: {}", crate::world::format_errors(&e)))),
                    }
                }
                Err(e) => (world, Err(format!("Typst compilation failed: {e}"))),
            }
        })
        .await
        .map_err(|e| format!("compile task failed: {e}"))?;

        self.world = Some(returned_world);
        result
    }
}
