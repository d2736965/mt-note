use dioxus::prelude::*;

use crate::frontend::components::markdown::Markdown;
use crate::frontend::js_bridge::{DioxusBridge, js_api};
use crate::frontend::views::editor::ScrollSyncState;

stylance::import_style!(styles, "markdown_preview.module.scss");

#[component]
pub fn MarkdownPreview(content: Signal<String>, file_path: Option<String>) -> Element {
    let mut scroll_sync: ScrollSyncState = use_context();
    let mut last_sync: Signal<u32> = use_signal(|| 0);

    use_effect(move || {
        let current = (scroll_sync.pending_sync)();
        if current > last_sync() {
            last_sync.set(current);
            let anchor_line = (scroll_sync.anchor_line)();
            let anchor_ratio = (scroll_sync.anchor_viewport_ratio)();
            let total = (scroll_sync.total_lines)();
            if anchor_line > 0 || total > 1 {
                js_api::editor::scroll_preview_to_anchor::<DioxusBridge>(
                    styles::preview.trim(),
                    anchor_line as f64,
                    anchor_ratio,
                );
            }
        }
    });

    let preview_class = styles::preview.trim().to_string();

    // 内容重新渲染后恢复已保存的滚动位置（从 markdown.ts 的 link-click 导航保存的）
    // 仅当保存的文件路径与当前文件路径一致时才恢复，避免错误应用到其他文件
    let pc_for_restore = preview_class.clone();
    let fp_for_restore = file_path.clone();
    use_effect(move || {
        content(); // subscribe to content changes
        let pc = pc_for_restore.clone();
        let fp = fp_for_restore.clone();
        spawn(async move {
            let saved = js_api::editor::get_saved_md_scroll::<DioxusBridge>().await;
            if saved > 0.0 {
                let saved_path =
                    js_api::editor::get_saved_md_scroll_path::<DioxusBridge>().await;
                if saved_path == fp.unwrap_or_default() {
                    js_api::editor::scroll_preview_to_ratio::<DioxusBridge>(&pc, saved);
                    js_api::editor::clear_saved_md_scroll::<DioxusBridge>();
                }
            }
        });
    });

    rsx! {
        div {
            class: styles::preview,
            onscroll: {
                let preview_class = preview_class.clone();
                move |_e: Event<ScrollData>| {
                    let preview_class = preview_class.clone();
                    spawn(async move {
                        let v = js_api::editor::get_preview_scroll_ratio::<
                            DioxusBridge,
                        >(&preview_class)
                            .await;
                        {
                            scroll_sync.preview_scroll_ratio.set(v);
                        }
                    });
                }
            },

            Markdown { content, file_path }
        }
    }
}
