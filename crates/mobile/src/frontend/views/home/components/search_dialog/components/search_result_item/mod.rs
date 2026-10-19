//! 搜索结果项组件。

use dioxus::prelude::*;
use mt_note_fs_engine::{FileCategory, SearchResult};
use ui::components::icons::{
    CodeIcon, DescriptionIcon, FileIcon, FolderIcon, ImageIcon, PictureAsPdfIcon,
};

stylance::import_style!(styles, "search_result_item.module.scss");

/// 搜索结果项组件的 Props。
#[derive(Props, Clone, PartialEq)]
pub struct SearchResultItemProps {
    /// 搜索结果。
    pub result: SearchResult,
    /// 根目录（用于计算相对路径）。
    pub root: std::path::PathBuf,
    /// 打开文件回调。
    pub on_open: EventHandler<std::path::PathBuf>,
    /// 打开目录回调（原地浏览）。
    pub on_open_dir: EventHandler<std::path::PathBuf>,
    /// 是否展开内容匹配。
    pub is_expanded: bool,
    /// 切换展开状态回调。
    pub on_toggle_expand: EventHandler<()>,
}

/// 搜索结果项组件。
#[component]
pub fn SearchResultItem(props: SearchResultItemProps) -> Element {
    let entry = props.result.entry.clone();
    let has_content = !props.result.matches.is_empty();
    let category = entry.category();

    // 计算相对路径作为 meta
    let meta = entry
        .path()
        .strip_prefix(&props.root)
        .ok()
        .and_then(|p| {
            let s = p.to_string_lossy();
            if s.is_empty() { None } else { Some(s.to_string()) }
        });

    // 选择图标组件
    let icon = if entry.is_dir() {
        rsx! { FolderIcon {} }
    } else {
        match category {
            FileCategory::Typst => rsx! { CodeIcon {} },
            FileCategory::Markdown => rsx! { DescriptionIcon {} },
            FileCategory::Pdf => rsx! { PictureAsPdfIcon {} },
            FileCategory::Svg => rsx! { ImageIcon {} },
            FileCategory::Image => rsx! { ImageIcon {} },
            FileCategory::Unknown => rsx! { FileIcon {} },
        }
    };

    rsx! {
        button {
            class: styles::result_item,
            onclick: {
                let path = entry.path().to_path_buf();
                let is_dir = entry.is_dir();
                move |_| {
                    if is_dir {
                        props.on_open_dir.call(path.clone());
                    } else {
                        props.on_open.call(path.clone());
                    }
                }
            },
            span { class: styles::entry_icon, {icon} }
            div { class: styles::entry_info,
                div { class: styles::entry_name_wrapper,
                    span { class: styles::entry_name, "{entry.name()}" }
                }
                if let Some(ref path) = meta {
                    div { class: styles::entry_meta_wrapper,
                        span { class: styles::entry_meta, "{path}" }
                    }
                }
                if has_content {
                    button {
                        class: styles::expand_btn,
                        onclick: move |_| props.on_toggle_expand.call(()),
                        if props.is_expanded {
                            "▾"
                        } else {
                            "▸"
                        }
                    }
                }
            }
        }
        if has_content && props.is_expanded {
            div { class: styles::match_lines,
                for m in &props.result.matches {
                    div { class: styles::match_line,
                        span { class: styles::line_num, "{m.line_number}" }
                        span { class: styles::line_preview, "{m.preview}" }
                    }
                }
            }
        }
    }
}
