use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};

use dioxus::prelude::*;
use kameo::prelude::*;
use pulldown_cmark::{CodeBlockKind, CowStr, Event, Options, Parser, Tag, TagEnd};
use tokio::task;

#[derive(Actor)]
pub struct MarkdownRenderer {
    cache: HashMap<(String, Option<String>), String>,
}

#[messages]
impl MarkdownRenderer {
    #[message]
    pub async fn render(&mut self, content: String, file_path: Option<String>) -> String {
        let key = (content.clone(), file_path.clone());
        if let Some(cached) = self.cache.get(&key) {
            return cached.clone();
        }

        let content_clone = content.clone();
        let file_path_clone = file_path.clone();
        let safe_html = task::spawn_blocking(move || {
            render_markdown_to_html(&content_clone, file_path_clone.as_deref())
        })
        .await
        .unwrap_or_default();

        self.cache.insert(key, safe_html.clone());
        safe_html
    }
}

fn render_markdown_to_html(content: &str, file_path: Option<&str>) -> String {
    let base_dir = file_path.and_then(|p| Path::new(p).parent().map(PathBuf::from));

    // GFM 脚注模式（ENABLE_FOOTNOTES 但不含 ENABLE_OLD_FOOTNOTES）。
    // - 脚注内容需 4 空格缩进，块级元素（列表、代码块、引用、表格）正常解析
    // - 脚注正确结束：非缩进、非空行即退出脚注
    // 旧脚注模式（ENABLE_FOOTNOTES + ENABLE_OLD_FOOTNOTES）有缺陷：
    // 块级元素内嵌时 peek_up() 只查最近祖先，找不到脚注定义，脚注永不结束
    // 详见 https://github.com/raphlinus/pulldown-cmark，has_gfm_footnotes() 逻辑
    let options = (Options::all() - Options::ENABLE_OLD_FOOTNOTES) | Options::ENABLE_FOOTNOTES;
    let parser = Parser::new_ext(content, options);
    let offset_pairs: Vec<(Event<'_>, Range<usize>)> = parser.into_offset_iter().collect();

    let line_offsets = build_line_offsets(content);
    let processed = process_events(offset_pairs, base_dir, &line_offsets);

    let mut raw_html = String::new();
    pulldown_cmark::html::push_html(&mut raw_html, processed.into_iter());

    raw_html
}

fn build_line_offsets(content: &str) -> Vec<usize> {
    let mut offsets = vec![0];
    for (i, ch) in content.char_indices() {
        if ch == '\n' {
            offsets.push(i + 1);
        }
    }
    offsets
}

fn offset_to_line(offset: usize, line_offsets: &[usize]) -> usize {
    match line_offsets.binary_search(&offset) {
        Ok(line) => line,
        Err(line) => line.saturating_sub(1),
    }
}

/// 处理脚注等嵌套内容中的事件，使其与正文渲染一致：
/// - InlineMath / DisplayMath → $...$ / $$...$$ 文本，供 KaTeX 渲染
/// - Image → 解析相对路径
fn process_footnote_events<'a>(
    events: Vec<Event<'a>>,
    base_dir: Option<&PathBuf>,
) -> Vec<Event<'a>> {
    events
        .into_iter()
        .flat_map(|e| match e {
            Event::InlineMath(text) => {
                vec![
                    Event::Html("$".into()),
                    Event::Text(text),
                    Event::Html("$".into()),
                ]
            }
            Event::DisplayMath(text) => {
                vec![
                    Event::Html("$$".into()),
                    Event::Text(text),
                    Event::Html("$$".into()),
                ]
            }
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                let resolved = base_dir.and_then(|dir| resolve_image_url(&dest_url, dir));
                let new_url = resolved.map(CowStr::from).unwrap_or(dest_url);
                vec![Event::Start(Tag::Image {
                    link_type,
                    dest_url: new_url,
                    title,
                    id,
                })]
            }
            other => vec![other],
        })
        .collect()
}

fn process_events<'a>(
    pairs: Vec<(Event<'a>, Range<usize>)>,
    base_dir: Option<PathBuf>,
    line_offsets: &[usize],
) -> Vec<Event<'a>> {
    let mut result = Vec::with_capacity(pairs.len());
    let mut heading_id_counter: HashMap<String, u32> = HashMap::new();
    let mut footnote_defs: Vec<(String, Vec<Event<'_>>)> = Vec::new();
    let mut footnote_order: HashMap<String, usize> = HashMap::new();
    let mut footnote_ref_counts: HashMap<String, usize> = HashMap::new();
    let mut footnote_counter: usize = 0;
    let mut i = 0;

    while i < pairs.len() {
        let line = offset_to_line(pairs[i].1.start, line_offsets);
        match &pairs[i].0 {
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                let resolved = base_dir
                    .as_deref()
                    .and_then(|dir| resolve_image_url(dest_url, dir));
                let new_url = resolved
                    .map(CowStr::from)
                    .unwrap_or_else(|| dest_url.clone());
                result.push(Event::Start(Tag::Image {
                    link_type: *link_type,
                    dest_url: new_url,
                    title: title.clone(),
                    id: id.clone(),
                }));
                i += 1;
                continue;
            }
            Event::FootnoteReference(name) => {
                let name_str = name.to_string();
                let idx = *footnote_order.entry(name_str.clone()).or_insert_with(|| {
                    footnote_counter += 1;
                    footnote_counter
                });
                let count = footnote_ref_counts.entry(name_str.clone()).or_insert(0);
                *count += 1;
                let ref_id = if *count == 1 {
                    format!("fnref-{name_str}")
                } else {
                    format!("fnref-{name_str}-{count}")
                };
                result.push(Event::Html(
                    format!(
                        r##"<sup class="footnote-reference"><a href="#fn-{name_str}" id="{ref_id}">[{idx}]</a></sup>"##
                    )
                    .into(),
                ));
                i += 1;
                continue;
            }
            Event::Start(Tag::FootnoteDefinition(name)) => {
                let footnote_name = name.to_string();
                let mut inner_events: Vec<Event<'_>> = Vec::new();
                let mut depth = 1;
                i += 1;
                while i < pairs.len() && depth > 0 {
                    if let Event::Start(Tag::FootnoteDefinition(_)) = &pairs[i].0 {
                        depth += 1;
                    } else if let Event::End(TagEnd::FootnoteDefinition) = &pairs[i].0 {
                        depth -= 1;
                    }
                    if depth > 0 {
                        inner_events.push(pairs[i].0.clone());
                    }
                    i += 1;
                }
                footnote_defs.push((footnote_name, inner_events));
                continue;
            }
            Event::Start(Tag::Heading {
                level,
                id,
                classes,
                attrs,
            }) => {
                result.push(Event::Html(
                    format!(r#"<span data-source-line="{line}"></span>"#).into(),
                ));

                if id.is_none() {
                    let mut text_content = String::new();
                    let mut j = i + 1;
                    while j < pairs.len() {
                        if let Event::Text(t) | Event::Code(t) = &pairs[j].0 {
                            text_content.push_str(t);
                        }
                        if matches!(&pairs[j].0, Event::End(TagEnd::Heading(_))) {
                            break;
                        }
                        j += 1;
                    }

                    let slug = slugify(&text_content);
                    let unique_id = make_unique_slug(&slug, &mut heading_id_counter);

                    result.push(Event::Start(Tag::Heading {
                        level: *level,
                        id: Some(CowStr::from(unique_id)),
                        classes: classes.clone(),
                        attrs: attrs.clone(),
                    }));
                } else {
                    result.push(pairs[i].0.clone());
                }
            }
            Event::InlineMath(text) => {
                result.push(Event::Html("$".into()));
                result.push(Event::Text(text.clone()));
                result.push(Event::Html("$".into()));
            }
            Event::DisplayMath(text) => {
                result.push(Event::Html("$$".into()));
                result.push(Event::Text(text.clone()));
                result.push(Event::Html("$$".into()));
            }
            Event::Start(Tag::Paragraph) => {
                result.push(Event::Html(
                    format!(r#"<span data-source-line="{line}"></span>"#).into(),
                ));
                result.push(pairs[i].0.clone());
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                result.push(Event::Html(
                    format!(r#"<span data-source-line="{line}"></span>"#).into(),
                ));
                match kind {
                    CodeBlockKind::Fenced(lang)
                        if lang.as_ref().eq_ignore_ascii_case("mermaid") =>
                    {
                        let mut code_content = String::new();
                        i += 1;
                        while i < pairs.len() {
                            match &pairs[i].0 {
                                Event::Text(text) => code_content.push_str(text),
                                Event::End(TagEnd::CodeBlock) => break,
                                _ => {
                                    continue;
                                }
                            }
                            i += 1;
                        }
                        result.push(Event::Html(
                            format!(r#"<div class="mermaid">{code_content}</div>"#).into(),
                        ));
                        i += 1;
                        continue;
                    }
                    _ => {
                        result.push(pairs[i].0.clone());
                    }
                }
            }
            Event::Start(Tag::List(_)) => {
                result.push(Event::Html(
                    format!(r#"<span data-source-line="{line}"></span>"#).into(),
                ));
                result.push(pairs[i].0.clone());
            }
            Event::Start(Tag::BlockQuote(_)) => {
                result.push(Event::Html(
                    format!(r#"<span data-source-line="{line}"></span>"#).into(),
                ));
                result.push(pairs[i].0.clone());
            }
            Event::Start(Tag::Table(_)) => {
                result.push(Event::Html(
                    format!(r#"<span data-source-line="{line}"></span>"#).into(),
                ));
                result.push(pairs[i].0.clone());
            }
            other => {
                result.push(other.clone());
            }
        }
        i += 1;
    }

    if !footnote_defs.is_empty() {
        result.push(Event::Html("<hr>".into()));
        result.push(Event::Html(r##"<section class="footnotes"><ol>"##.into()));
        for (name, inner) in footnote_defs {
            result.push(Event::Html(format!(r##"<li id="fn-{name}">"##).into()));
            // 脚注内的公式、图片等也需要和正文一样处理
            result.extend(process_footnote_events(inner, base_dir.as_ref()));
            result.push(Event::Html(
                format!(r##"<a href="#fnref-{name}" class="footnote-back">↩</a>"##).into(),
            ));
            result.push(Event::Html("</li>".into()));
        }
        result.push(Event::Html("</ol></section>".into()));
    }

    result
}

fn slugify(text: &str) -> String {
    let slug: String = text
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c
            } else if c.is_whitespace() {
                '-'
            } else {
                '\0'
            }
        })
        .filter(|c| *c != '\0')
        .collect();

    if slug.is_empty() {
        return "heading".to_string();
    }

    slug
}

fn make_unique_slug(base: &str, counter: &mut HashMap<String, u32>) -> String {
    let count = counter.entry(base.to_string()).or_insert(0);
    *count += 1;
    if *count == 1 {
        base.to_string()
    } else {
        format!("{}-{}", base, count)
    }
}

fn resolve_image_url(url: &str, base_dir: &Path) -> Option<String> {
    if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("data:") {
        return None;
    }

    let resolved = if url.starts_with('/') {
        PathBuf::from(url)
    } else {
        base_dir.join(url)
    };

    Some(format!(
        "/local-file/{}",
        resolved.to_string_lossy().trim_start_matches('/')
    ))
}

#[derive(Clone)]
pub struct MarkdownRenderContext {
    pub renderer: ActorRef<MarkdownRenderer>,
}

impl MarkdownRenderContext {
    pub fn provide() -> Signal<Self> {
        use_context_provider(|| {
            let renderer = MarkdownRenderer::spawn(MarkdownRenderer {
                cache: HashMap::new(),
            });
            Signal::new(Self { renderer })
        })
    }
}
