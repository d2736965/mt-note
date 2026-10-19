//! Typst 预览组件。
//!
//! 编译 Typst 源码并渲染 SVG，支持双指捏合缩放和双击切换缩放。
//! 缩放以焦点为基准：双指捏合以两指中心为焦点，
//! 双击以视口中心为焦点。
//! 通过显式设置容器宽度（而非 CSS transform）实现缩放，
//! 让浏览器原生处理滚动，放大后可滚动到四个边。
//! 叠加透明文字层支持文字选择。
//!
//! 初始缩放策略：智能选择 fit-width 或 fit-height，
//! 取 min(scale_w, scale_h) 保证 paper 完整可见、填满视口、无滚动条。
//! 双击在 fit 和 100% 之间切换。
//!
//! 缩放、适配、刷新操作通过 AppBar 菜单控制，菜单项通过 Context 共享 ZoomState。
//!
//! 性能优化：
//! - 全局 `TypstCompiler` Actor 复用 `TypstWorld`，实现增量编译
//! - 多页文档并行渲染 SVG 和文字层（rayon）
//! - 编译在 `spawn_blocking` 中执行，不阻塞异步运行时

use std::path::PathBuf;
use std::rc::Rc;

use dioxus::html::geometry::PixelsVector2D;
use dioxus::logger::tracing;
use dioxus::prelude::*;
use dioxus_i18n::prelude::i18n;
use dioxus_core::Task;
use models::TypstPrefs;
use mt_note_typst_engine::{CompileTypstContent, ContentRect, get_compiler};
use ui::hooks::use_app_state::use_app_state;

use crate::frontend::i18n as i18n_support;
use crate::frontend::js_bridge::{DioxusBridge, js_api};
use crate::frontend::views::editor::preview::ContentFitViewport;
use crate::frontend::views::editor::preview::page_indicator::{
    hide_all_page_numbers, inject_typst_page_numbers, on_preview_interact,
};
use crate::frontend::views::editor::{ScrollSyncState, ZoomMode, ZoomState};

stylance::import_style!(styles, "typst_preview.module.scss");

type CompileOutput = Result<(String, String, Option<ContentRect>), String>;

#[derive(Clone, PartialEq)]
struct PinchState {
    initial_distance: f64,
    initial_scale: f64,
    last_dist: f64,
}

#[component]
pub fn TypstPreview(file_path: String, content: Signal<String>) -> Element {
    let i18n_ctx = i18n();
    let fp = file_path.clone();
    let font_paths = match use_app_state() {
        Ok(state) => state.read().typst_prefs().font_path_bufs(),
        Err(e) => {
            tracing::error!("TypstPreview: AppState 未初始化: {e}");
            TypstPrefs::default().font_path_bufs()
        }
    };
    let mut svg_resource: Resource<CompileOutput> = use_resource(move || {
        let fp = fp.clone();
        let font_paths = font_paths.clone();
        let content = content.read().clone();
        compile_typst(fp, font_paths, content)
    });

    let mut scale = use_context::<ZoomState>().scale;
    let mut fit_scale = use_context::<ZoomState>().fit_scale;
    let mut content_fit_scale = use_context::<ZoomState>().content_fit_scale;
    let mut zoom_mode = use_context::<ZoomState>().zoom_mode;
    let mut center_request = use_context::<ZoomState>().center_request;
    let refresh_trigger = use_context::<ZoomState>().refresh_trigger;
    let show_text_layer = use_context::<ZoomState>().show_text_layer;
    let mut scroll_sync: ScrollSyncState = use_context();
    let mut pinch_start = use_signal(|| None::<PinchState>);
    let mut mounted_el: Signal<Option<Rc<MountedData>>> = use_signal(|| None);
    let mut page_dims = use_signal(|| (1.0_f64, 1.0_f64));
    let mut viewport_rect = use_signal(|| (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64));
    let mut content_rect = use_signal(|| None::<ContentRect>);
    let mut needs_fit_svg = use_signal(|| 0_usize);
    let mut last_refresh = use_signal(|| 0_u32);
    let page_indicator_hide_task: Signal<Option<Task>> = use_signal(|| None);

    use_effect(move || {
        let trigger = refresh_trigger();
        if trigger != last_refresh() {
            last_refresh.set(trigger);
            svg_resource.restart();
        }
    });

    use_effect(move || {
        let s = scale();
        let mode = zoom_mode();
        let crect = *content_rect.peek();
        inject_snap(page_dims().0, s, mode == ZoomMode::ContentFit, crect);
        align_typst_text_layer("zoom-root");
    });

    use_effect(move || {
        let visible = show_text_layer();
        toggle_text_layer_debug(visible);
    });

    {
        let mut last_sync: Signal<u32> = use_signal(|| 0);
        use_effect(move || {
            let current = (scroll_sync.pending_sync)();
            let svg_ready = svg_resource().is_some();
            if current > last_sync() && svg_ready {
                last_sync.set(current);
                let anchor_line = (scroll_sync.anchor_line)();
                let total = (scroll_sync.total_lines)().max(1);
                if total > 1 {
                    let ratio = anchor_line as f64 / total as f64;
                    js_api::editor::scroll_preview_to_ratio::<DioxusBridge>(
                        styles::preview.trim(),
                        ratio,
                    );
                }
            }
        });
    }

    use_effect(move || {
        center_request();
        let s = *scale.peek();
        let crect = *content_rect.peek();
        scroll_to_content_center(page_dims().0, s, crect);
    });

    let (page_w, _page_h) = page_dims();

    rsx! {
        div {
            class: styles::preview,
            onmounted: move |e: MountedEvent| {
                let data = e.data();
                mounted_el.set(Some(data.clone()));
                spawn(async move {
                    if let Ok(r) = data.get_client_rect().await {
                        viewport_rect.set((r.origin.x, r.origin.y, r.size.width, r.size.height));
                    }
                });
            },
            ontouchstart: move |e: Event<TouchData>| {
                let touches = e.data.touches();
                if touches.len() == 2 {
                    let p0 = touches[0].client_coordinates();
                    let p1 = touches[1].client_coordinates();
                    let fx = (p0.x + p1.x) / 2.0;
                    let fy = (p0.y + p1.y) / 2.0;
                    let dist = touch_distance(&touches[0], &touches[1]);
                    let (vl, vt, _vw, _vh) = viewport_rect();
                    let s0 = scale();
                    let pw = page_w;
                    pinch_start
                        .set(
                            Some(PinchState {
                                initial_distance: dist,
                                initial_scale: s0,
                                last_dist: dist,
                            }),
                        );
                    js_api::pinch_zoom::start::<
                        DioxusBridge,
                    >("zoom-root", fx, fy, vl, vt, s0, dist, pw);
                } else if touches.len() == 1 {
                    js_api::pinch_zoom::reset_snap::<DioxusBridge>();
                }
                on_preview_interact(page_indicator_hide_task);
            },
            ontouchmove: move |e: Event<TouchData>| {
                let touches = e.data.touches();
                if touches.len() == 2
                    && let Some(ps) = pinch_start.write().as_mut()
                {
                    let dist = touch_distance(&touches[0], &touches[1]);
                    ps.last_dist = dist;
                    let ratio = dist / ps.initial_distance.max(1.0);
                    let ns = (ps.initial_scale * ratio).clamp(0.3, 10.0);
                    let k = ns / ps.initial_scale;
                    js_api::pinch_zoom::zoom::<DioxusBridge>(ns, k);
                }
            },
            ontouchend: move |_| {
                if let Some(p) = pinch_start.take() {
                    let ns = (p.initial_scale * p.last_dist / p.initial_distance.max(1.0))
                        .clamp(0.3, 10.0);
                    js_api::pinch_zoom::end::<DioxusBridge>(ns);
                    scale.set(ns);
                    if (ns - p.initial_scale).abs() > f64::EPSILON {
                        zoom_mode.set(ZoomMode::Free);
                    }
                }
                on_preview_interact(page_indicator_hide_task);
            },
            ondoubleclick: move |_| {
                let current = scale();
                let same_as_paper =
                    (content_fit_scale() - fit_scale()).abs() < 0.001;
                let new_mode = match zoom_mode() {
                    ZoomMode::ContentFit => ZoomMode::Actual,
                    ZoomMode::Actual => {
                        if same_as_paper { ZoomMode::ContentFit } else { ZoomMode::PaperFit }
                    }
                    ZoomMode::PaperFit => ZoomMode::ContentFit,
                    ZoomMode::Free => ZoomMode::ContentFit,
                };
                let new_scale = match new_mode {
                    ZoomMode::ContentFit => content_fit_scale(),
                    ZoomMode::Actual => 1.0,
                    ZoomMode::PaperFit => fit_scale(),
                    ZoomMode::Free => content_fit_scale(),
                };
                apply_zoom_scroll(mounted_el, current, new_scale, None);
                scale.set(new_scale);
                zoom_mode.set(new_mode);
                if new_mode == ZoomMode::ContentFit {
                    center_request.set(center_request() + 1);
                }
                on_preview_interact(page_indicator_hide_task);
            },
            onscroll: {
                let preview_class = styles::preview.trim().to_string();
                move |_| {
                    on_preview_interact(page_indicator_hide_task);
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
            match svg_resource() {
                None => rsx! {
                    div { class: styles::preview_loading, { i18n_support::translate(&i18n_ctx, "typst-compiling") } }
                },
                Some(Ok((svg, text_layer, crect))) => {
                    let s = scale();
                    let container_w = page_w * s;
                    let svg_fingerprint = svg.len();
                    if svg_fingerprint != needs_fit_svg()
                        && let Some(el) = mounted_el()
                    {
                        needs_fit_svg.set(svg_fingerprint);
                        let el_clone = el.clone();
                        let svg_str = svg.clone();
                        spawn(async move {
                            let (w_pt, h_pt) = extract_svg_size(&svg_str);
                            let w_px = w_pt * 4.0 / 3.0;
                            let h_px = h_pt * 4.0 / 3.0;
                            if let Ok(rect) = el_clone.get_client_rect().await {
                                let avail_w = (rect.width() - 16.0).max(1.0);
                                let paper_fit = (avail_w / w_px)
                                    .min(rect.height() / h_px)
                                    .clamp(0.3, 5.0);
                                fit_scale.set(paper_fit);
                                let content_w_px = crect.map_or(w_px, |c| {
                                    let page_w_pt = w_px * 3.0 / 4.0;
                                    ContentFitViewport::width(c.width, c.left, page_w_pt)
                                        * 4.0
                                        / 3.0
                                });
                                let cfit = (avail_w / content_w_px).clamp(0.3, 5.0);
                                content_fit_scale.set(cfit);
                                page_dims.set((w_px, h_px));
                                content_rect.set(crect);
                                scale.set(cfit);
                                zoom_mode.set(ZoomMode::ContentFit);
                                center_request.set(center_request() + 1);
                            }
                            inject_typst_page_numbers("zoom-root");
                            hide_all_page_numbers();
                            align_typst_text_layer("zoom-root");
                        });
                    }
                    rsx! {
                        div {
                            class: styles::zoom_container,
                            id: "zoom-root",
                            style: "width: {container_w}px;",
                            div { class: styles::typst_output, dangerous_inner_html: svg }
                            div {
                                class: styles::text_layer,
                                style: if show_text_layer() { "width: {page_w}px; transform: scale({s}); transform-origin: 0 0; background: rgba(255,255,0,0.08);" } else { "width: {page_w}px; transform: scale({s}); transform-origin: 0 0;" },
                                dangerous_inner_html: text_layer,
                            }
                        }
                    }
                }
                Some(Err(err)) => rsx! {
                    div { class: styles::preview_error, { i18n_support::translate_err(&i18n_ctx, "err-typst-compile", &err) } }
                },
            }
        }
    }
}

fn apply_zoom_scroll(
    mounted_el: Signal<Option<Rc<MountedData>>>,
    old_scale: f64,
    new_scale: f64,
    focus: Option<(f64, f64)>,
) {
    if let Some(el) = mounted_el.cloned() {
        spawn(async move {
            if let Ok(offset) = el.get_scroll_offset().await {
                let viewport_focus = match focus {
                    Some((fx, fy)) => Some((fx, fy)),
                    None => match el.get_client_rect().await {
                        Ok(rect) => Some((rect.width() / 2.0, rect.height() / 2.0)),
                        Err(e) => {
                            tracing::error!("get_client_rect 失败: {e}");
                            None
                        }
                    },
                };
                if let Some((vx, vy)) = viewport_focus {
                    let cx = (offset.x + vx) / old_scale;
                    let cy = (offset.y + vy) / old_scale;
                    let new_x = cx * new_scale - vx;
                    let new_y = cy * new_scale - vy;
                    if let Err(e) = el
                        .scroll(
                            PixelsVector2D::new(new_x, new_y),
                            dioxus::html::ScrollBehavior::Instant,
                        )
                        .await
                    {
                        tracing::error!("scroll 失败: {e}");
                    }
                }
            } else if let Err(e) = el.get_scroll_offset().await {
                tracing::error!("get_scroll_offset 失败: {e}");
            }
        });
    }
}

fn touch_distance(t0: &TouchPoint, t1: &TouchPoint) -> f64 {
    let p0 = t0.client_coordinates();
    let p1 = t1.client_coordinates();
    let dx = p0.x - p1.x;
    let dy = p0.y - p1.y;
    (dx * dx + dy * dy).sqrt()
}

fn inject_snap(page_w: f64, scale: f64, is_content_mode: bool, crect: Option<ContentRect>) {
    let page_w_px = page_w * scale;
    // 吸附目标跟随内容包围盒（缩放后像素）；无内容矩形时退回整页
    let (content_l_px, content_w_px) = crect
        .map(|c| (c.left * 4.0 / 3.0 * scale, c.width * 4.0 / 3.0 * scale))
        .unwrap_or((0.0, page_w_px));
    js_api::pinch_zoom::inject_snap::<DioxusBridge>(
        "zoom-root",
        content_l_px,
        content_w_px,
        is_content_mode,
    );
}

fn scroll_to_content_center(page_w: f64, scale: f64, crect: Option<ContentRect>) {
    let page_w_px = page_w * scale;
    let center_px = crect.map(|c| (c.left + c.width / 2.0) * 4.0 / 3.0 * scale);
    js_api::pinch_zoom::scroll_to_content_center::<DioxusBridge>(
        "zoom-root",
        page_w_px,
        center_px,
    );
}

async fn compile_typst(
    file_path: String,
    font_paths: Vec<PathBuf>,
    content: String,
) -> Result<(String, String, Option<ContentRect>), String> {
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
    let compiler = get_compiler();
    compiler
        .ask(CompileTypstContent {
            file_path,
            font_paths,
            content,
            cache_dir,
        })
        .await
        .map_err(|e| e.to_string())
}

fn extract_svg_size(svg: &str) -> (f64, f64) {
    if let Some(start) = svg.find("viewBox=\"") {
        let rest = &svg[start + 9..];
        if let Some(end) = rest.find('"') {
            let viewbox = &rest[..end];
            let parts: Vec<&str> = viewbox.split_whitespace().collect();
            if parts.len() >= 4
                && let Ok(w) = parts[2].parse::<f64>()
                && let Ok(h) = parts[3].parse::<f64>()
            {
                return (w, h);
            }
        }
    }
    (595.0, 842.0)
}

fn align_typst_text_layer(container_id: &str) {
    js_api::pinch_zoom::align_text_layer::<DioxusBridge>(container_id);
}

fn toggle_text_layer_debug(visible: bool) {
    // debug：让文字层可见（红字 + 页面蓝框），用于核对选中文字与 SVG 渲染是否对齐。
    // 框必须用 outline：border 会把 position:relative 的 .text-layer 推进内侧 2px
    // （SVG 侧页面无 border 不随之移动），造成文字层相对 SVG 的全局错位。
    // stylance class 带 hash（text_layer-xxx），用属性选择器匹配。
    js_api::pinch_zoom::toggle_text_layer_debug::<DioxusBridge>(
        "zoom-root",
        "__tl-debug-style",
        visible,
        "[class*=\"text_layer\"] span { color: rgba(255,0,0,0.35) !important; } [class*=\"text_layer\"] [class*=\"typst-text-page\"] { outline: 2px solid rgba(0,0,255,0.3) !important; }",
    );
}
