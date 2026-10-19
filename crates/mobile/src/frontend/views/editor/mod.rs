use std::cell::Cell;
use std::path::PathBuf;

use dioxus::logger::tracing;
use dioxus::prelude::*;
use dioxus_i18n::prelude::i18n;
use models::data::editor_prefs::{OpenMode, ViewMode};
use mt_note_editor_lsp::LspContext;
use mt_note_fs_engine::{FileCategory, is_text_file, read_file, write_bytes, write_file};
use mt_note_typst_engine::{CompileTypstPdf, get_compiler};
use serde_json;
use ui::components::icons::Icon;
use ui::hooks::use_app_state::use_app_state;

use crate::frontend::components::file_changed_dialog::FileChangedDialog;
use crate::frontend::components::markdown::renderer::MarkdownRenderContext;
use crate::frontend::components::toast::{ToastItem, push_toast};
use crate::frontend::file_watcher::{self, FsEventKind};
use crate::frontend::hooks::use_back_interceptor::use_back_interceptor;
use crate::frontend::hooks::use_bottom_bar_state::use_bottom_bar_state;
use crate::frontend::hooks::use_top_app_bar_state::use_top_app_bar_state;
use crate::frontend::i18n::Localize;
use crate::frontend::i18n as i18n_support;
use crate::frontend::js_bridge::{DioxusBridge, js_api};
use crate::frontend::store::bottom_bar_state::{BottomAppBarAction, BottomAppBarConfig};
use crate::frontend::store::storage::android::{AndroidBackend, AndroidStorage};
use crate::frontend::store::top_app_bar_state::{MenuItem, TrailingButton};
use crate::frontend::views::editor::code_editor::CodeEditor;
use crate::frontend::views::editor::components::datetime_dialog::DatetimeDialog;
use crate::frontend::views::editor::components::save_dialog::SaveDialog;
use crate::frontend::views::editor::preview::Preview;

stylance::import_style!(styles, "editor.module.scss");

pub mod code_editor;
pub mod components;
pub mod preview;

/// 缩放模式，驱动双击三态循环。
#[derive(Clone, Copy, PartialEq)]
pub enum ZoomMode {
    /// 内容适应：内容区宽适应屏宽，margin 溢出可横滚。
    ContentFit,
    /// 100% 原始尺寸。
    Actual,
    /// 纸张适应：整页（含 margin）短边适应屏幕短边。
    PaperFit,
    /// 自由缩放：捏合缩放后的状态，不触发吸附。
    Free,
}

/// 编辑/预览模式切换时的滚动位置同步状态。
///
/// 通过 Context 在 Editor、CodeEditor、Preview 之间共享，
/// 实现切换模式时锚点同步。
///
/// 核心原则：锚点是编辑和预览的共同焦点——同一个源码行号。
/// 知道了一个状态的锚点行，就自然知道另一个状态的锚点行（同一个行号）。
/// 切换时只需确定锚点行号 + 锚点在视口中的位置比例，
/// 另一个状态根据同一个锚点行号定位即可。
///
/// 锚点选择策略：
/// - 编辑→预览：光标可见时以光标行为锚点；光标不可见时以视口中心行为锚点
/// - 预览→编辑：以视口中心最近的 `data-source-line` 元素对应的行为锚点
///
/// 锚点同步的效果：切换模式时，锚点行在视口中的相对位置不变，
/// 其他内容围绕锚点重新排列，类似图片缩放时的焦点不动效果。
#[derive(Clone, Copy)]
pub struct ScrollSyncState {
    /// 锚点源码行号（0-based），编辑和预览共享。
    ///
    /// 编辑→预览时：设为光标行或视口中心行
    /// 预览→编辑时：设为视口中心最近的 `data-source-line` 行号
    pub anchor_line: Signal<usize>,
    /// 编辑器内容总行数，Typst 比例回退方案使用。
    pub total_lines: Signal<usize>,
    /// 预览滚动比例（0.0~1.0），Typst 回退方案。
    pub preview_scroll_ratio: Signal<f64>,
    /// 锚点在视口中的相对位置（0.0~1.0），0=顶部，1=底部，0.5=中心。
    ///
    /// 切换前记录锚点在当前视口中的位置，切换后目标状态据此定位：
    /// targetScroll = anchorElementTop - anchor_viewport_ratio * viewportHeight
    pub anchor_viewport_ratio: Signal<f64>,
    /// 模式切换后需要同步滚动位置（递增计数器，每次切换 +1）。
    pub pending_sync: Signal<u32>,
}

/// 缩放状态，通过 Context 在 Editor 和 TypstPreview 之间共享。
#[derive(Clone)]
pub struct ZoomState {
    pub scale: Signal<f64>,
    /// 纸张适应缩放值（整页短边适应屏幕）。
    pub fit_scale: Signal<f64>,
    /// 内容适应缩放值（内容区宽适应屏宽）。
    pub content_fit_scale: Signal<f64>,
    /// 当前缩放模式，驱动双击循环。
    pub zoom_mode: Signal<ZoomMode>,
    /// 进入内容适应模式时 +1，触发预览容器横向居中。
    pub center_request: Signal<u32>,
    pub refresh_trigger: Signal<u32>,
    /// 是否显示文字层（半透明调试模式）。
    pub show_text_layer: Signal<bool>,
}

/// 文件编辑器页面。
///
/// 支持编辑模式和预览模式切换。
#[component]
pub fn Editor(path: String) -> Element {
    let i18n_ctx = i18n();
    let mut path_signal = use_signal(|| path.clone());
    if path_signal() != path {
        path_signal.set(path.clone());
    }

    let decoded_path = urlencoding::decode(&path).unwrap_or_else(|_| path.clone().into());
    let file_path = PathBuf::from(decoded_path.as_ref());
    let file_path_display = file_path.to_string_lossy().to_string();
    let unnamed = i18n_support::translate(&i18n_ctx, "common-unnamed");
    let file_name = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&unnamed)
        .to_string();
    let category = FileCategory::from_path(&file_path);
    let is_previewable = category.is_previewable();

    // 异步判断是否文本文件（仅对不可预览的文件读取检测）。
    // 入口 FileOpenerActor 已保证只有支持的文件才进 Editor，这里作为兜底。
    let file_path_for_detect = file_path.clone();
    let is_text_resource = use_resource(move || {
        let fp = file_path_for_detect.clone();
        async move {
            if is_previewable {
                return false;
            }
            match is_text_file(&fp).await {
                Ok(v) => v,
                Err(e) => {
                    tracing::error!("is_text_file({}): {e}", fp.display());
                    false
                }
            }
        }
    });
    let is_text = is_text_resource().unwrap_or(false);
    let is_supported = is_previewable || is_text;

    let mut app_state = match use_app_state() {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Editor: AppState 未初始化: {e}");
            return rsx! { { i18n_support::translate(&i18n_ctx, "common-app-state-error") } };
        }
    };
    let is_editable = category.is_editable() || is_text;
    let initial_mode = if !is_editable {
        ViewMode::Preview
    } else {
        match app_state.read().open_mode() {
            OpenMode::Preview => ViewMode::Preview,
            OpenMode::Edit => ViewMode::Edit,
            OpenMode::Remember => app_state
                .read()
                .editor_prefs()
                .last_view_mode
                .unwrap_or(ViewMode::Edit),
        }
    };
    let initial_mode = if initial_mode == ViewMode::Preview && !is_previewable {
        ViewMode::Edit
    } else {
        initial_mode
    };

    let mut view_mode = use_signal(|| initial_mode);
    let mut content = use_signal(String::new);
    let mut dirty = use_signal(|| false);
    let mut show_save_dialog = use_signal(|| false);
    let mut file_changed = use_signal(|| false);
    let mut datetime_dialog_open = use_signal(|| false);
    let toast_signal: Signal<Vec<ToastItem>> = use_context();
    use_back_interceptor(
        show_save_dialog() || datetime_dialog_open() || file_changed(),
        move || {
            if show_save_dialog() {
                show_save_dialog.set(false);
            } else if file_changed() {
                file_changed.set(false);
            } else {
                datetime_dialog_open.set(false);
            }
        },
    );

    let file_path_for_save = file_path.clone();
    let file_path_for_save_bottom = file_path.clone();
    let file_path_for_read = file_path.clone();
    let file_path_for_watch = file_path.clone();

    let scroll_sync = ScrollSyncState {
        anchor_line: use_signal(|| 0),
        total_lines: use_signal(|| 1),
        preview_scroll_ratio: use_signal(|| 0.0),
        anchor_viewport_ratio: use_signal(|| 0.5),
        pending_sync: use_signal(|| 0u32),
    };
    use_context_provider(|| scroll_sync);

    let zoom_state = ZoomState {
        scale: use_signal(|| 1.0_f64),
        fit_scale: use_signal(|| 1.0_f64),
        content_fit_scale: use_signal(|| 1.0_f64),
        zoom_mode: use_signal(|| ZoomMode::ContentFit),
        center_request: use_signal(|| 0_u32),
        refresh_trigger: use_signal(|| 0_u32),
        show_text_layer: use_signal(|| false),
    };
    use_context_provider(|| zoom_state.clone());
    {
        let cache_dir = {
            #[cfg(target_os = "android")]
            {
                crate::frontend::store::storage::android::android_cache_dir().map(PathBuf::from)
            }
            #[cfg(not(target_os = "android"))]
            {
                None
            }
        };
        let font_paths = app_state.read().typst_prefs().font_path_bufs();
        use_context_provider(|| LspContext::new_with_defaults(cache_dir, font_paths));
    }
    MarkdownRenderContext::provide();

    use_effect(move || {
        let current_path = path_signal();
        // 不可预览文件订阅 is_text_resource，异步判断完成后 effect 重跑读取内容；
        // 可预览文件不订阅（首次 is_supported 已就绪，避免无谓重跑）
        if !is_previewable {
            is_text_resource();
        }
        tracing::debug!("Editor effect: path={current_path}");

        view_mode.set(initial_mode);
        dirty.set(false);
        file_changed.set(false);
        show_save_dialog.set(false);

        js_api::events::reset_all::<DioxusBridge>();

        if is_supported && !matches!(category, FileCategory::Pdf) {
            match read_file(&file_path_for_read) {
                Ok(text) => {
                    content.set(text.clone());
                    spawn(async move {
                        js_api::editor::set_textarea_value::<DioxusBridge>(&text).await;
                    });
                }
                Err(e) => {
                    push_toast(toast_signal, e.to_string());
                }
            }
        }
    });

    // 监听打开的文件的外部修改
    use_effect({
        let fp = file_path_for_watch.clone();
        move || {
            let fp = fp.clone();
            spawn(async move {
                file_watcher::watch_file(&fp).await;
            });
        }
    });

    use_effect({
        let fp = file_path_for_watch.clone();
        move || {
            let fp = fp.clone();
            spawn(async move {
                let rx = file_watcher::subscribe().await;
                if let Some(mut rx) = rx {
                    while rx.changed().await.is_ok() {
                        let event = rx.borrow().clone();
                        tracing::info!(
                            "editor: received event {:?} {}",
                            event.kind,
                            event.path.display()
                        );
                        if event.path == fp && event.kind == FsEventKind::FileModified {
                            file_changed.set(true);
                        }
                    }
                } else {
                    tracing::error!("editor: subscribe failed");
                }
            });
        }
    });

    let mut custom_snap_active = use_signal(|| false);
    let prev_scale = use_hook(|| Cell::new(0.0_f64));
    {
        let scale = zoom_state.scale;
        use_effect(move || {
            let s = scale();
            let prev = prev_scale.get();
            if prev != 0.0 && (s - prev).abs() > f64::EPSILON {
                let element_id = match category {
                    FileCategory::Pdf => "pdf-zoom-root",
                    _ => "zoom-root",
                };
                spawn(async move {
                    js_api::pinch_zoom::clear_custom_snap_target::<DioxusBridge>(element_id);
                });
                custom_snap_active.set(false);
            }
            prev_scale.set(s);
        });
    }
    {
        let zoom_mode = zoom_state.zoom_mode;
        use_effect(move || {
            let mode = zoom_mode();
            if mode == ZoomMode::ContentFit {
                let element_id = match category {
                    FileCategory::Pdf => "pdf-zoom-root",
                    _ => "zoom-root",
                };
                spawn(async move {
                    js_api::pinch_zoom::clear_custom_snap_target::<DioxusBridge>(element_id);
                });
                custom_snap_active.set(false);
            }
        });
    }

    if let Ok(mut app_bar) = use_top_app_bar_state() {
        let nav = navigator();
        app_bar.write().set_title(&file_name);

        if is_supported {
            let mut scale = zoom_state.scale;
            let fit_scale = zoom_state.fit_scale;
            let content_fit_scale = zoom_state.content_fit_scale;
            let mut zoom_mode = zoom_state.zoom_mode;
            let mut center_request = zoom_state.center_request;
            let mut refresh_trigger = zoom_state.refresh_trigger;
            let show_text_layer = zoom_state.show_text_layer;

            let is_edit = view_mode() == ViewMode::Edit;
            let show_hover = app_state.read().editor_prefs().show_hover_info;
            let show_diag = app_state.read().editor_prefs().show_diagnostics;

            let mut buttons: Vec<TrailingButton> = Vec::new();

            if is_edit && matches!(category, FileCategory::Typst | FileCategory::Markdown) {
                buttons.push(TrailingButton::simple_toggle(
                    Icon::HoverInfo,
                    show_hover,
                    move || {
                        let current = app_state.read().editor_prefs().show_hover_info;
                        app_state.write().editor_prefs_mut().show_hover_info = !current;
                        if let Ok(backend) = AndroidBackend::new() {
                            let storage = AndroidStorage::new(backend);
                            if let Err(e) = storage.save(app_state.read().editor_prefs()) {
                                tracing::error!("persist editor_prefs: {e:?}");
                            }
                        }
                    },
                ));
                buttons.push(TrailingButton::simple_toggle(
                    Icon::BugReport,
                    show_diag,
                    move || {
                        let current = app_state.read().editor_prefs().show_diagnostics;
                        app_state.write().editor_prefs_mut().show_diagnostics = !current;
                        if let Ok(backend) = AndroidBackend::new() {
                            let storage = AndroidStorage::new(backend);
                            if let Err(e) = storage.save(app_state.read().editor_prefs()) {
                                tracing::error!("persist editor_prefs: {e:?}");
                            }
                        }
                    },
                ));
            }

            if is_editable && is_previewable {
                buttons.push(TrailingButton::toggle(
                    Icon::Visibility,
                    Icon::Edit,
                    is_edit,
                    {
                        let mut scroll_sync = scroll_sync;
                        move || {
                            let current_mode = view_mode();
                            let new_mode = current_mode.toggle();
                            if current_mode == ViewMode::Edit {
                                spawn(async move {
                                    let json_str = js_api::editor::get_cursor_info::<DioxusBridge>().await;
                                    let v: serde_json::Value = serde_json::from_str(&json_str).unwrap_or_default();
                                    {
                                        tracing::debug!("Edit→Preview: JS cursor result: {v:?}");
                                        if let Some(line) = v["line"].as_u64() {
                                            scroll_sync.anchor_line.set(line as usize);
                                        }
                                        if let Some(total) = v["total"].as_u64() {
                                            scroll_sync.total_lines.set(total as usize);
                                        }
                                        if let Some(ratio) = v["ratio"].as_f64() {
                                            scroll_sync.anchor_viewport_ratio.set(ratio);
                                        }
                                    }
                                    scroll_sync.pending_sync.set((scroll_sync.pending_sync)() + 1);
                                    tracing::debug!("Edit→Preview: pending_sync set to {}, anchor_line={}, anchor_ratio={:.3}", (scroll_sync.pending_sync)(), (scroll_sync.anchor_line)(), (scroll_sync.anchor_viewport_ratio)());
                                    view_mode.set(new_mode);
                                    app_state.write().editor_prefs_mut().last_view_mode = Some(new_mode);
                                    if let Ok(backend) = AndroidBackend::new() {
                                        let storage = AndroidStorage::new(backend);
                                        if let Err(e) = storage.save(app_state.read().editor_prefs()) {
                                            tracing::error!("persist editor_prefs: {e:?}");
                                        }
                                    }
                                });
                            } else {
                                spawn(async move {
                                    let json_str = js_api::editor::get_preview_anchor_info::<DioxusBridge>("[class^=\"preview-\"]").await;
                                    let v: serde_json::Value = serde_json::from_str(&json_str).unwrap_or_default();
                                    {
                                        tracing::debug!("Preview→Edit: JS anchor result: {v:?}");
                                        if let Some(ratio) = v["ratio"].as_f64() {
                                            scroll_sync.preview_scroll_ratio.set(ratio);
                                        }
                                        if let Some(line) = v["line"].as_u64() {
                                            scroll_sync.anchor_line.set(line as usize);
                                        }
                                        if let Some(anchor_ratio) = v["anchorRatio"].as_f64() {
                                            scroll_sync.anchor_viewport_ratio.set(anchor_ratio);
                                        }
                                    }
                                    scroll_sync.pending_sync.set((scroll_sync.pending_sync)() + 1);
                                    tracing::debug!("Preview→Edit: pending_sync set to {}, anchor_line={}, anchor_ratio={:.3}", (scroll_sync.pending_sync)(), (scroll_sync.anchor_line)(), (scroll_sync.anchor_viewport_ratio)());
                                    view_mode.set(new_mode);
                                    app_state.write().editor_prefs_mut().last_view_mode = Some(new_mode);
                                    if let Ok(backend) = AndroidBackend::new() {
                                        let storage = AndroidStorage::new(backend);
                                        if let Err(e) = storage.save(app_state.read().editor_prefs()) {
                                            tracing::error!("persist editor_prefs: {e:?}");
                                        }
                                    }
                                });
                            }
                        }
                    },
                ));
            }

            if !is_edit
                && matches!(category, FileCategory::Pdf | FileCategory::Typst)
                && zoom_mode() != ZoomMode::ContentFit
            {
                let element_id = match category {
                    FileCategory::Pdf => "pdf-zoom-root".to_string(),
                    _ => "zoom-root".to_string(),
                };
                buttons.push(TrailingButton::simple_toggle(
                    Icon::PushPin,
                    custom_snap_active(),
                    move || {
                        let eid = element_id.clone();
                        spawn(async move {
                            if custom_snap_active() {
                                js_api::pinch_zoom::clear_custom_snap_target::<DioxusBridge>(&eid);
                                custom_snap_active.set(false);
                            } else {
                                js_api::pinch_zoom::set_custom_snap_target::<DioxusBridge>(&eid);
                                custom_snap_active.set(true);
                            }
                        });
                    },
                ));
            }

            app_bar.write().set_trailing_buttons(buttons);

            let mut items = Vec::new();

            if is_editable {
                items.push(MenuItem::item(i18n_support::translate(&i18n_ctx, "common-save"), move || {
                    let fp = file_path_for_save.clone();
                    file_watcher::ignore_file_blocking(&fp, 1500);
                    match write_file(&fp, &content()) {
                        Ok(()) => {
                            dirty.set(false);
                            push_toast(toast_signal, i18n_support::translate(&i18n_ctx, "editor-saved"));
                        }
                        Err(e) => {
                            push_toast(toast_signal, i18n_support::translate_err(&i18n_ctx, "err-save", &e.localize(&i18n_ctx)));
                        }
                    }
                }));
            }

            let mut zoom_items = vec![
                MenuItem::item(i18n_support::translate(&i18n_ctx, "editor-zoom-in"), move || {
                    let s = (scale() + 0.2).min(10.0);
                    scale.set(s);
                }),
                MenuItem::item(i18n_support::translate(&i18n_ctx, "editor-zoom-out"), move || {
                    let s = (scale() - 0.2).max(0.3);
                    scale.set(s);
                }),
            ];
            if !matches!(category, FileCategory::Svg) {
                zoom_items.push(MenuItem::item(i18n_support::translate(&i18n_ctx, "editor-fit-content"), move || {
                    scale.set(content_fit_scale());
                    zoom_mode.set(ZoomMode::ContentFit);
                    center_request.set(center_request() + 1);
                }));
                zoom_items.push(MenuItem::item("100%", move || {
                    scale.set(1.0);
                    zoom_mode.set(ZoomMode::Actual);
                }));
                zoom_items.push(MenuItem::item(i18n_support::translate(&i18n_ctx, "editor-fit-paper"), move || {
                    scale.set(fit_scale());
                    zoom_mode.set(ZoomMode::PaperFit);
                }));
            }
            zoom_items.push(MenuItem::item(i18n_support::translate(&i18n_ctx, "common-refresh"), move || {
                refresh_trigger.set(refresh_trigger() + 1);
            }));
            if !is_edit && !matches!(category, FileCategory::Markdown) {
                items.push(MenuItem::submenu(i18n_support::translate(&i18n_ctx, "editor-zoom"), zoom_items));
            }
            items.push(MenuItem::submenu(i18n_support::translate(&i18n_ctx, "common-more"), {
                let mut more_items = vec![MenuItem::item(i18n_support::translate(&i18n_ctx, "common-close"), move || {
                    nav.go_back();
                })];
                if !is_edit && matches!(category, FileCategory::Typst | FileCategory::Pdf) {
                    let mut show_tl = show_text_layer;
                    let label = if show_tl() {
                        i18n_support::translate(&i18n_ctx, "editor-hide-text")
                    } else {
                        i18n_support::translate(&i18n_ctx, "editor-show-text")
                    };
                    more_items.push(MenuItem::item(label, move || {
                        show_tl.set(!show_tl());
                    }));
                }
                more_items
            }));
            // 仅 typst 文件提供"另存为 PDF"
            if category == FileCategory::Typst {
                let mut show_save_dialog = show_save_dialog;
                items.insert(
                    1,
                    MenuItem::submenu(
                        i18n_support::translate(&i18n_ctx, "editor-save-as"),
                        vec![MenuItem::item("PDF", move || {
                            show_save_dialog.set(true);
                        })],
                    ),
                );
            }
            app_bar.write().set_menu_items(items);
        } else {
            app_bar
                .write()
                .set_menu_items(vec![MenuItem::item(i18n_support::translate(&i18n_ctx, "common-close"), move || {
                    nav.go_back();
                })]);
        }
    }

    if let Ok(mut bottom_bar) = use_bottom_bar_state() {
        let mut actions = Vec::new();
        let is_edit = view_mode() == ViewMode::Edit;

        if is_editable {
            let mut dirty = dirty;
            let content = content;
            let file_path = file_path_for_save_bottom.clone();
            actions.push(BottomAppBarAction {
                icon: Icon::Save,
                on_click: std::rc::Rc::new(std::cell::RefCell::new(move || {
                    let fp = file_path.clone();
                    file_watcher::ignore_file_blocking(&fp, 1500);
                    match write_file(&fp, &content()) {
                        Ok(()) => {
                            dirty.set(false);
                            push_toast(toast_signal, i18n_support::translate(&i18n_ctx, "editor-saved"));
                        }
                        Err(e) => {
                            push_toast(toast_signal, i18n_support::translate_err(&i18n_ctx, "err-save", &e.localize(&i18n_ctx)));
                        }
                    }
                })),
            });

            if is_edit {
                actions.push(BottomAppBarAction {
                    icon: Icon::Delete,
                    on_click: std::rc::Rc::new(std::cell::RefCell::new(move || {
                        let mut content = content;
                        let mut dirty = dirty;
                        let mut scroll_sync = scroll_sync;
                        spawn(async move {
                            let new_content = js_api::editor::delete_line::<DioxusBridge>().await;
                            content.set(new_content);
                            dirty.set(true);
                            scroll_sync.total_lines.set(content().split('\n').count());
                        });
                    })),
                });

                let mut datetime_dialog_open = datetime_dialog_open;
                actions.push(BottomAppBarAction {
                    icon: Icon::Schedule,
                    on_click: std::rc::Rc::new(std::cell::RefCell::new(move || {
                        datetime_dialog_open.set(true);
                    })),
                });
            }
        }

        if is_editable && is_previewable {
            let mut view_mode = view_mode;
            let mut scroll_sync = scroll_sync;
            let mut app_state = app_state;
            actions.push(BottomAppBarAction {
                icon: if is_edit { Icon::Visibility } else { Icon::Edit },
                on_click: std::rc::Rc::new(std::cell::RefCell::new(move || {
                    let current_mode = view_mode();
                    let new_mode = current_mode.toggle();
                    if current_mode == ViewMode::Edit {
                        spawn(async move {
                            let json_str = js_api::editor::get_cursor_info::<DioxusBridge>().await;
                            let v: serde_json::Value = serde_json::from_str(&json_str).unwrap_or_default();
                            {
                                tracing::debug!("Edit→Preview: JS cursor result: {v:?}");
                                if let Some(line) = v["line"].as_u64() {
                                    scroll_sync.anchor_line.set(line as usize);
                                }
                                if let Some(total) = v["total"].as_u64() {
                                    scroll_sync.total_lines.set(total as usize);
                                }
                                if let Some(ratio) = v["ratio"].as_f64() {
                                    scroll_sync.anchor_viewport_ratio.set(ratio);
                                }
                            }
                            scroll_sync.pending_sync.set((scroll_sync.pending_sync)() + 1);
                            tracing::debug!("Edit→Preview: pending_sync set to {}, anchor_line={}, anchor_ratio={:.3}", (scroll_sync.pending_sync)(), (scroll_sync.anchor_line)(), (scroll_sync.anchor_viewport_ratio)());
                            view_mode.set(new_mode);
                            app_state.write().editor_prefs_mut().last_view_mode = Some(new_mode);
                            if let Ok(backend) = AndroidBackend::new() {
                                let storage = AndroidStorage::new(backend);
                                if let Err(e) = storage.save(app_state.read().editor_prefs()) {
                                    tracing::error!("persist editor_prefs: {e:?}");
                                }
                            }
                        });
                    } else {
                        spawn(async move {
                            let json_str = js_api::editor::get_preview_anchor_info::<DioxusBridge>("[class^=\"preview-\"]").await;
                            let v: serde_json::Value = serde_json::from_str(&json_str).unwrap_or_default();
                            {
                                tracing::debug!("Preview→Edit: JS anchor result: {v:?}");
                                if let Some(ratio) = v["ratio"].as_f64() {
                                    scroll_sync.preview_scroll_ratio.set(ratio);
                                }
                                if let Some(line) = v["line"].as_u64() {
                                    scroll_sync.anchor_line.set(line as usize);
                                }
                                if let Some(anchor_ratio) = v["anchorRatio"].as_f64() {
                                    scroll_sync.anchor_viewport_ratio.set(anchor_ratio);
                                }
                            }
                            scroll_sync.pending_sync.set((scroll_sync.pending_sync)() + 1);
                            tracing::debug!("Preview→Edit: pending_sync set to {}, anchor_line={}, anchor_ratio={:.3}", (scroll_sync.pending_sync)(), (scroll_sync.anchor_line)(), (scroll_sync.anchor_viewport_ratio)());
                            view_mode.set(new_mode);
                            app_state.write().editor_prefs_mut().last_view_mode = Some(new_mode);
                            if let Ok(backend) = AndroidBackend::new() {
                                let storage = AndroidStorage::new(backend);
                                if let Err(e) = storage.save(app_state.read().editor_prefs()) {
                                    tracing::error!("persist editor_prefs: {e:?}");
                                }
                            }
                        });
                    }
                })),
            });
        }

        if actions.is_empty() {
            bottom_bar.write().reset();
        } else {
            bottom_bar
                .write()
                .set_bottom_app_bar(BottomAppBarConfig { actions });
        }
    }

    // 入口 FileOpenerActor 已保证只有支持的文件才进 Editor，
    // is_supported 仅作为 use_resource 异步加载期间的中间状态判断，
    // 加载完成前 is_previewable 为 false 的文件视为不支持——但此类文件
    // 不会经 Actor 进入 Editor，所以无需 unsupported 兜底分支。

    rsx! {
        div { class: styles::editor_view,
            match view_mode() {
                ViewMode::Edit => rsx! {
                    div { key: "{file_path_display}", class: styles::edit_area,
                        CodeEditor {
                            content,
                            file_path: Some(file_path.clone()),
                            on_change: move |_value| {
                                dirty.set(true);
                            },
                        }
                    }
                },
                ViewMode::Preview => rsx! {
                    div { key: "{file_path_display}", class: styles::preview_area,
                        Preview {
                            category,
                            file_path: file_path.to_string_lossy().to_string(),
                            content,
                        }
                    }
                },
            }

            // "另存为 PDF" 对话框
            if show_save_dialog() && category == FileCategory::Typst {
                {
                    let initial_dir = file_path
                        .parent()
                        .map(|p| p.to_path_buf())
                        .unwrap_or_default();
                    let unnamed = i18n_support::translate(&i18n_ctx, "common-unnamed");
                    let default_name = file_path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or(&unnamed)
                        .to_string();
                    rsx! {
                        SaveDialog {
                            initial_dir,
                            default_name,
                            extension: "pdf".to_string(),
                            on_save: move |target: PathBuf| {
                                show_save_dialog.set(false);
                                push_toast(toast_signal, i18n_support::translate(&i18n_ctx, "editor-exporting-pdf"));
                                let font_paths = app_state.read().typst_prefs().font_path_bufs();
                                let cache_dir = {
                                    #[cfg(target_os = "android")]
                                    {
                                        crate::frontend::store::storage::android::android_cache_dir()
                                            .map(PathBuf::from)
                                    }
                                    #[cfg(not(target_os = "android"))]
                                    {
                                        None
                                    }
                                };
                                let fp = file_path.clone();
                                let content_clone = content.read().clone();
                                let ts = toast_signal;
                                spawn(async move {
                                    let compiler = get_compiler();
                                    match compiler
                                        .ask(CompileTypstPdf {
                                            file_path: fp.to_string_lossy().to_string(),
                                            font_paths,
                                            content: content_clone,
                                            cache_dir,
                                        })
                                        .await
                                    {
                                        Ok(bytes) => {
                                            match write_bytes(&target, &bytes) {
                                                Ok(()) => {
                                                    push_toast(
                                                    ts,
                                                    i18n_support::translate_with(
                                                        &i18n_ctx,
                                                        "editor-exported",
                                                        &[("path", target.display().to_string().into())],
                                                    ),
                                                );
                                                }
                                                Err(e) => {
                                                    push_toast(ts, i18n_support::translate_err(&i18n_ctx, "err-write-file", &e.localize(&i18n_ctx)));
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            push_toast(ts, i18n_support::translate_err(&i18n_ctx, "err-export", &e.to_string()));
                                        }
                                    }
                                });
                            },
                            on_cancel: move |_| show_save_dialog.set(false),
                        }
                    }
                }
            }

            DatetimeDialog { open: datetime_dialog_open, content, dirty }

            if file_changed() {
                FileChangedDialog {
                    file_name: file_name.clone(),
                    on_reload: {
                        let fp = file_path_for_watch.clone();
                        let mut dirty = dirty;
                        let mut content = content;
                        let mut file_changed = file_changed;
                        move |_| {
                            file_changed.set(false);
                            match read_file(&fp) {
                                Ok(text) => {
                                    content.set(text.clone());
                                    dirty.set(false);
                                    let text_clone = text;
                                    spawn(async move {
                                        js_api::editor::set_textarea_value::<DioxusBridge>(&text_clone)
                                            .await;
                                    });
                                }
                                Err(e) => {
                                    push_toast(toast_signal, i18n_support::translate_err(&i18n_ctx, "err-reload", &e.localize(&i18n_ctx)));
                                }
                            }
                        }
                    },
                    on_cancel: move |_| {
                        file_changed.set(false);
                    },
                }
            }
        }
    }
}
