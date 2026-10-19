use std::path::PathBuf;

use dioxus::prelude::*;
use kameo::actor::ActorRef;
use mt_note_fs_engine::{DirEntry, FileCategory};
use ui::components::icons::{
    CodeIcon, DescriptionIcon, FileIcon, FolderIcon, ImageIcon, PictureAsPdfIcon,
};

use crate::frontend::js_bridge::{DioxusBridge, js_api};
use crate::frontend::store::file_opener::{FileOpenerActor, OpenResult, ask_open, push_editor};

stylance::import_style!(styles, "entry_item.module.scss");

/// 文件条目组件。
///
/// 点击目录时触发 `on_dir_click`，点击文件时通过 FileOpenerActor 打开
/// （可预览/文本进 Editor，不支持的弹系统选择器）。
/// 长按（contextmenu）触发 `on_long_press`，传该条目路径。
#[component]
pub fn EntryItem(
    entry: DirEntry,
    on_dir_click: EventHandler<PathBuf>,
    on_long_press: EventHandler<PathBuf>,
    subtitle: Option<String>,
) -> Element {
    let name = entry.name().to_string();
    let is_dir = entry.is_dir();
    let category = entry.category();
    let path = entry.path().to_path_buf();
    let long_press_path = path.clone();

    let meta = subtitle.unwrap_or_else(|| {
        if is_dir {
            String::new()
        } else {
            match entry.size() {
                Some(sz) => format_size(sz),
                None => category.label().to_string(),
            }
        }
    });

    let dir_path = path.clone();
    let file_path = path.clone();
    let actor_ctx = use_context::<Signal<Option<ActorRef<FileOpenerActor>>>>();

    rsx! {
        button {
            class: styles::entry_item,
            onclick: move |_| {
                if is_dir {
                    on_dir_click.call(dir_path.clone());
                } else {
                    // 在 spawn 之前同步保存滚动位置，避免异步延迟导致 DOM 已卸载
                    js_api::scroll::save_home_scroll::<DioxusBridge>();
                    let p = file_path.clone();
                    spawn(async move {
                        if let Some(OpenResult::Editor(path)) = ask_open(&actor_ctx, p).await {
                            push_editor(&path);
                        }
                    });
                }
            },
            // 长按（Android WebView 长按触发 contextmenu）打开上下文菜单
            oncontextmenu: move |e: Event<MouseData>| {
                e.prevent_default();
                on_long_press.call(long_press_path.clone());
            },
            span { class: styles::entry_icon,
                if is_dir {
                    FolderIcon {}
                } else {
                    match category {
                        FileCategory::Typst => rsx! {
                            CodeIcon {}
                        },
                        FileCategory::Markdown => rsx! {
                            DescriptionIcon {}
                        },
                        FileCategory::Pdf => rsx! {
                            PictureAsPdfIcon {}
                        },
                        FileCategory::Svg => rsx! {
                            ImageIcon {}
                        },
                        FileCategory::Image => rsx! {
                            ImageIcon {}
                        },
                        FileCategory::Unknown => rsx! {
                            FileIcon {}
                        },
                    }
                }
            }
            div { class: styles::entry_info,
                div { class: styles::entry_name_wrapper,
                    span { class: styles::entry_name, "{name}" }
                }
                if !meta.is_empty() {
                    div { class: styles::entry_meta_wrapper,
                        span { class: styles::entry_meta, "{meta}" }
                    }
                }
            }
        }
    }
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
