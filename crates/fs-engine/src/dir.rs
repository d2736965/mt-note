//! 目录遍历与文件条目。

use std::path::{Path, PathBuf};

use crate::{FileCategory, FsError, is_text_file};

/// 目录条目，轻量级，不持有文件内容。
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DirEntry {
    /// 绝对路径。
    path: PathBuf,
    /// 文件名。
    name: String,
    /// 是否为目录。
    is_dir: bool,
    /// 文件分类（仅文件有意义）。
    category: FileCategory,
    /// 文件大小（字节），目录为 None。
    size: Option<u64>,
}

impl DirEntry {
    fn from_path(path: PathBuf, is_dir: bool) -> Self {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?")
            .to_string();
        let category = if is_dir {
            FileCategory::Unknown
        } else {
            FileCategory::from_path(&path)
        };
        Self {
            path,
            name,
            is_dir,
            category,
            size: None,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn is_dir(&self) -> bool {
        self.is_dir
    }

    pub fn category(&self) -> FileCategory {
        self.category
    }

    pub fn size(&self) -> Option<u64> {
        self.size
    }

    /// 排序权重：目录优先，然后按名称字母序。
    pub fn sort_key(&self) -> (bool, &str) {
        (!self.is_dir, &self.name)
    }
}

/// 高级搜索选项。
#[derive(Clone, Debug)]
pub struct SearchOptions {
    /// 搜索查询字符串。
    pub query: String,
    /// 是否使用正则表达式匹配。
    pub is_regex: bool,
    /// 是否区分大小写。
    pub case_sensitive: bool,
    /// 是否搜索文件内容（全文搜索）。
    pub search_content: bool,
    /// 内容搜索时每文件仅匹配一次。
    pub only_first_match: bool,
    /// 是否显示隐藏文件。
    pub show_hidden: bool,
    /// 最大递归深度。
    pub max_depth: usize,
}

/// 内容匹配行。
#[derive(Clone, Debug, PartialEq)]
pub struct ContentMatch {
    /// 行号（从 1 开始）。
    pub line_number: usize,
    /// 匹配行预览（截断到 100 字符）。
    pub preview: String,
}

/// 搜索单条结果。
#[derive(Clone, Debug, PartialEq)]
pub struct SearchResult {
    /// 文件/目录条目。
    pub entry: DirEntry,
    /// 内容匹配行（仅 search_content 时有值）。
    pub matches: Vec<ContentMatch>,
}

/// 目录列表器，遍历指定目录的**直接子项**。
///
/// 使用 `ignore` crate，自动遵循 `.gitignore` 规则。
/// 仅列出第一层，不递归。
pub struct DirLister {
    root: PathBuf,
}

impl DirLister {
    /// 创建目录列表器。
    ///
    /// 不立即遍历，调用 [`list`](Self::list) 时执行。
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// 递归搜索目录下文件名匹配 query 的条目（大小写不敏感）。
    ///
    /// 使用 `tokio::fs` 异步 IO，最大深度 16 层。
    /// 目录优先、文件在后，各自按名称字母序。
    pub async fn search(&self, query: &str, show_hidden: bool) -> Result<Vec<DirEntry>, FsError> {
        if query.is_empty() {
            return Ok(Vec::new());
        }
        if !self.root.exists() {
            return Err(FsError::NotFound(self.root.clone()));
        }
        if !self.root.is_dir() {
            return Err(FsError::NotDir(self.root.clone()));
        }

        const MAX_DEPTH: usize = 16;
        let query_lower = query.to_lowercase();
        let mut entries: Vec<DirEntry> = Vec::new();
        let mut stack: Vec<(PathBuf, usize)> = vec![(self.root.clone(), 0)];

        while let Some((dir, depth)) = stack.pop() {
            if depth >= MAX_DEPTH {
                continue;
            }

            let mut read_dir = tokio::fs::read_dir(&dir)
                .await
                .map_err(|e| FsError::io(&dir, e))?;

            while let Ok(Some(entry)) = read_dir.next_entry().await {
                let path = entry.path();
                let file_name = entry.file_name();
                let name_str = file_name.to_str().unwrap_or("");

                if !show_hidden && name_str.starts_with('.') {
                    continue;
                }

                let is_dir = entry
                    .file_type()
                    .await
                    .map_err(|e| FsError::io(&path, e))?
                    .is_dir();

                if name_str.to_lowercase().contains(&query_lower) {
                    let mut dir_entry = DirEntry::from_path(path.clone(), is_dir);
                    if !is_dir {
                        dir_entry.size = entry.metadata().await.ok().map(|m| m.len());
                    }
                    entries.push(dir_entry);
                }

                if is_dir {
                    stack.push((path, depth + 1));
                }
            }
        }

        entries.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
        Ok(entries)
    }

    /// 列出目录下的直接子项。
    ///
    /// 目录排前、文件排后，各自按名称字母序。
    /// 隐藏文件（以 `.` 开头）默认不列出，除非 `show_hidden` 为 true。
    pub fn list(&self, show_hidden: bool) -> Result<Vec<DirEntry>, FsError> {
        if !self.root.exists() {
            return Err(FsError::NotFound(self.root.clone()));
        }
        if !self.root.is_dir() {
            return Err(FsError::NotDir(self.root.clone()));
        }

        let mut entries: Vec<DirEntry> = Vec::new();

        let walker = ignore::WalkBuilder::new(&self.root)
            .max_depth(Some(1))
            .hidden(!show_hidden)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .sort_by_file_name(|a, b| a.cmp(b))
            .build();

        for result in walker {
            let entry = match result {
                Ok(e) => e,
                Err(err) => return Err(FsError::walk(&self.root, err)),
            };

            if entry.path() == self.root {
                continue;
            }

            let is_dir = entry.file_type().is_some_and(|ft| ft.is_dir());

            let mut dir_entry = DirEntry::from_path(entry.path().to_path_buf(), is_dir);

            if !is_dir {
                dir_entry.size = entry.metadata().ok().map(|m| m.len());
            }

            entries.push(dir_entry);
        }

        entries.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
        Ok(entries)
    }

    /// 高级搜索：递归搜索目录，支持正则、大小写、内容搜索。
    ///
    /// 结果按目录优先、文件在后，各自按名称字母序。
    pub async fn search_advanced(
        &self,
        opts: &SearchOptions,
    ) -> Result<Vec<SearchResult>, FsError> {
        if opts.query.is_empty() {
            return Ok(Vec::new());
        }
        if !self.root.exists() {
            return Err(FsError::NotFound(self.root.clone()));
        }
        if !self.root.is_dir() {
            return Err(FsError::NotDir(self.root.clone()));
        }

        let matcher = NameMatcher::new(&opts.query, opts.is_regex, opts.case_sensitive)?;

        walk_entries(
            &self.root,
            opts.show_hidden,
            opts.max_depth,
            |path, name, is_dir, size| {
                let matcher = matcher.clone();
                let search_content = opts.search_content;
                let only_first = opts.only_first_match;
                async move {
                    if !matcher.matches(&name) {
                        return None;
                    }

                    let mut entry = DirEntry::from_path(path.clone(), is_dir);
                    if !is_dir {
                        entry.size = size;
                    }

                    let content_matches = if search_content && !is_dir {
                        if is_text_file(&path).await.unwrap_or(false) {
                            search_file_content(&path, &matcher, only_first).await
                        } else {
                            Vec::new()
                        }
                    } else {
                        Vec::new()
                    };

                    Some(SearchResult {
                        entry,
                        matches: content_matches,
                    })
                }
            },
        )
        .await
        .map(|mut results| {
            results.sort_by(|a, b| a.entry.sort_key().cmp(&b.entry.sort_key()));
            results
        })
    }

    /// 流式高级搜索：每搜到一个结果立刻通过回调通知，通过回调返回值控制是否继续。
    ///
    /// 回调返回 `true` 表示继续搜索，`false` 表示停止搜索。
    pub async fn search_advanced_streaming(
        &self,
        opts: SearchOptions,
        on_result: impl Fn(SearchResult) -> bool + Send + 'static,
    ) -> Result<(), FsError> {
        if opts.query.is_empty() {
            return Ok(());
        }
        if !self.root.exists() {
            return Err(FsError::NotFound(self.root.clone()));
        }
        if !self.root.is_dir() {
            return Err(FsError::NotDir(self.root.clone()));
        }

        let matcher = NameMatcher::new(&opts.query, opts.is_regex, opts.case_sensitive)?;

        let mut stack: Vec<(PathBuf, usize)> = vec![(self.root.clone(), 0)];
        let show_hidden = opts.show_hidden;
        let max_depth = opts.max_depth;
        let search_content = opts.search_content;
        let only_first = opts.only_first_match;

        while let Some((dir, depth)) = stack.pop() {
            if depth >= max_depth {
                continue;
            }

            let mut read_dir = tokio::fs::read_dir(&dir)
                .await
                .map_err(|e| FsError::io(&dir, e))?;

            while let Ok(Some(entry)) = read_dir.next_entry().await {
                let path = entry.path();
                let file_name = entry.file_name();
                let name_str = file_name.to_str().unwrap_or("");

                if !show_hidden && name_str.starts_with('.') {
                    continue;
                }

                let is_dir = entry
                    .file_type()
                    .await
                    .map_err(|e| FsError::io(&path, e))?
                    .is_dir();

                if matcher.matches(name_str) {
                    let mut dir_entry = DirEntry::from_path(path.clone(), is_dir);
                    if !is_dir && let Some(s) = entry.metadata().await.ok().map(|m| m.len()) {
                        dir_entry.size = Some(s);
                    }

                    let content_matches = if search_content && !is_dir {
                        if is_text_file(&path).await.unwrap_or(false) {
                            search_file_content(&path, &matcher, only_first).await
                        } else {
                            Vec::new()
                        }
                    } else {
                        Vec::new()
                    };

                    let should_continue = on_result(SearchResult {
                        entry: dir_entry,
                        matches: content_matches,
                    });

                    if !should_continue {
                        return Ok(());
                    }
                }

                if is_dir {
                    stack.push((path, depth + 1));
                }
            }
        }

        Ok(())
    }
}

/// 递归遍历目录树，对每个条目调用 `on_entry`，收集返回 `Some(T)` 的结果。
async fn walk_entries<T, F, Fut>(
    root: &Path,
    show_hidden: bool,
    max_depth: usize,
    mut on_entry: F,
) -> Result<Vec<T>, FsError>
where
    F: FnMut(PathBuf, String, bool, Option<u64>) -> Fut,
    Fut: std::future::Future<Output = Option<T>>,
{
    let mut results: Vec<T> = Vec::new();
    let mut stack: Vec<(PathBuf, usize)> = vec![(root.to_path_buf(), 0)];

    while let Some((dir, depth)) = stack.pop() {
        if depth >= max_depth {
            continue;
        }

        let mut read_dir = tokio::fs::read_dir(&dir)
            .await
            .map_err(|e| FsError::io(&dir, e))?;

        while let Ok(Some(entry)) = read_dir.next_entry().await {
            let path = entry.path();
            let file_name = entry.file_name();
            let name_str = file_name.to_str().unwrap_or("");

            if !show_hidden && name_str.starts_with('.') {
                continue;
            }

            let is_dir = entry
                .file_type()
                .await
                .map_err(|e| FsError::io(&path, e))?
                .is_dir();

            let size = if !is_dir {
                entry.metadata().await.ok().map(|m| m.len())
            } else {
                None
            };

            if let Some(item) = on_entry(path.clone(), name_str.to_string(), is_dir, size).await {
                results.push(item);
            }

            if is_dir {
                stack.push((path, depth + 1));
            }
        }
    }

    Ok(results)
}

/// 读取文件全部内容为字符串。
pub fn read_file(path: &Path) -> Result<String, FsError> {
    if !path.exists() {
        return Err(FsError::NotFound(path.to_path_buf()));
    }
    std::fs::read_to_string(path).map_err(|e| FsError::io(path, e))
}

/// 写入文件内容。
pub fn write_file(path: &Path, content: &str) -> Result<(), FsError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| FsError::io(parent, e))?;
    }
    std::fs::write(path, content).map_err(|e| FsError::io(path, e))
}

/// 写入二进制文件内容（如 PDF、图片）。
pub fn write_bytes(path: &Path, content: &[u8]) -> Result<(), FsError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| FsError::io(parent, e))?;
    }
    std::fs::write(path, content).map_err(|e| FsError::io(path, e))
}

/// 创建空文件。若文件已存在则返回 [`FsError::AlreadyExists`]。
pub fn create_file(path: &Path) -> Result<(), FsError> {
    if path.exists() {
        return Err(FsError::AlreadyExists(path.to_path_buf()));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| FsError::io(parent, e))?;
    }
    std::fs::File::create(path).map_err(|e| FsError::io(path, e))?;
    Ok(())
}

/// 创建目录。若目录已存在则返回 [`FsError::AlreadyExists`]。
pub fn create_dir(path: &Path) -> Result<(), FsError> {
    if path.exists() {
        return Err(FsError::AlreadyExists(path.to_path_buf()));
    }
    std::fs::create_dir_all(path).map_err(|e| FsError::io(path, e))?;
    Ok(())
}

/// 删除文件或目录（目录递归删除）。
pub fn remove_path(path: &Path) -> Result<(), FsError> {
    if path.is_dir() {
        std::fs::remove_dir_all(path).map_err(|e| FsError::io(path, e))
    } else {
        std::fs::remove_file(path).map_err(|e| FsError::io(path, e))
    }
}

/// 重命名/移动文件或目录到新路径。若目标已存在则返回 [`FsError::AlreadyExists`]。
pub fn rename_path(from: &Path, to: &Path) -> Result<(), FsError> {
    if to.exists() {
        return Err(FsError::AlreadyExists(to.to_path_buf()));
    }
    std::fs::rename(from, to).map_err(|e| FsError::io(from, e))
}

/// 文件名匹配器：支持子串和正则两种模式。
#[derive(Clone)]
struct NameMatcher {
    regex: Option<regex::Regex>,
    literal: String,
    case_sensitive: bool,
}

impl NameMatcher {
    fn new(query: &str, is_regex: bool, case_sensitive: bool) -> Result<Self, FsError> {
        if is_regex {
            let mut builder = regex::RegexBuilder::new(query);
            builder.case_insensitive(!case_sensitive);
            let regex = builder
                .build()
                .map_err(|e| FsError::invalid_input(format!("invalid regex: {e}")))?;
            Ok(Self {
                regex: Some(regex),
                literal: String::new(),
                case_sensitive,
            })
        } else {
            Ok(Self {
                regex: None,
                literal: if case_sensitive {
                    query.to_string()
                } else {
                    query.to_lowercase()
                },
                case_sensitive,
            })
        }
    }

    fn matches(&self, text: &str) -> bool {
        if let Some(ref regex) = self.regex {
            regex.is_match(text)
        } else if self.case_sensitive {
            text.contains(&self.literal)
        } else {
            text.to_lowercase().contains(&self.literal)
        }
    }
}

/// 在文件内容中搜索匹配行。
async fn search_file_content(
    path: &Path,
    matcher: &NameMatcher,
    only_first: bool,
) -> Vec<ContentMatch> {
    let content = match tokio::fs::read_to_string(path).await {
        Ok(c) => c,
        Err(_e) => return Vec::new(),
    };

    let mut matches = Vec::new();
    for (i, line) in content.lines().enumerate() {
        if matcher.matches(line) {
            let preview = if line.len() > 100 {
                format!("{}...", &line[..100])
            } else {
                line.to_string()
            };
            matches.push(ContentMatch {
                line_number: i + 1,
                preview,
            });
            if only_first {
                break;
            }
        }
    }
    matches
}
