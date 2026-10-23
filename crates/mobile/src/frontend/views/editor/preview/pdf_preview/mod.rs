//! PDF 预览组件。
//!
//! 使用 pdf.js 在 WebView 中渲染 PDF。
//! 支持双指捏合缩放和双击切换缩放，与 Typst 预览保持一致的交互。
//! 多页 PDF 竖向排列，所有页同步缩放。
//!
//! 数据流：
//! 1. `use_resource` 异步读取 PDF 字节（spawn_blocking）
//! 2. 字节就绪后 base64 编码并调用 `load_pdf` JS 函数
//! 3. JS 端 pdf.js 解析 PDF，首页尺寸/内容矩形就绪后**立即返回**；
//!    页面按需渲染（滚动/缩放驱动，只渲染视口附近页，离屏回收 canvas），
//!    其余占位页由 JS 后台补齐——大文档不再全量渲染撑爆内存
//! 4. 轮询 `window.__pdfPreview.getPageInfos()` 取得页面尺寸后计算 fit scale
//! 5. 缩放变化时调用 `set_pdf_scale` 触发按需重渲染
//!
//! 缩放交互完全复用 Typst 预览逻辑：
//! - 双指捏合：transform 阶段 + 松手后 width = ns * pageW
//! - 双击三态：ContentFit → Actual → PaperFit
//! - 菜单缩放：通过共享 ZoomState 触发

use std::path::Path;
use std::rc::Rc;

use base64::{Engine as _, engine::general_purpose};
use dioxus::html::geometry::PixelsVector2D;
use dioxus::logger::tracing;
use dioxus::prelude::*;
use dioxus_i18n::prelude::i18n;
use dioxus_core::Task;
use serde_json;

use crate::frontend::i18n as i18n_support;
use crate::frontend::js_bridge::{DioxusBridge, js_api};
use crate::frontend::views::editor::preview::ContentFitViewport;
use crate::frontend::views::editor::preview::page_indicator::{
    hide_all_page_numbers, inject_pdf_page_numbers, on_preview_interact,
};
use crate::frontend::views::editor::{ZoomMode, ZoomState};

stylance::import_style!(styles, "pdf_preview.module.scss");

/// PDF 预览专用的容器 id，避免与 Typst 预览的 zoom-root 冲突。
const CONTAINER_ID: &str = "pdf-zoom-root";

/// `use_resource` 返回的 PDF 字节读取结果。
#[derive(Clone)]
struct PdfBytesResult {
    bytes: Vec<u8>,
}

#[derive(Clone, PartialEq)]
struct PinchState {
    initial_distance: f64,
    initial_scale: f64,
    last_dist: f64,
}

/// PDF 预览组件。
///
/// 加载并渲染 PDF 文件，支持多页、缩放、双击切换。
#[component]
pub fn PdfPreview(file_path: String) -> Element {
    let i18n_ctx = i18n();
    let fp = file_path.clone();

    let mut scale = use_context::<ZoomState>().scale;
    let fit_scale = use_context::<ZoomState>().fit_scale;
    let content_fit_scale = use_context::<ZoomState>().content_fit_scale;
    let mut zoom_mode = use_context::<ZoomState>().zoom_mode;
    let mut center_request = use_context::<ZoomState>().center_request;
    let refresh_trigger = use_context::<ZoomState>().refresh_trigger;
    let show_text_layer = use_context::<ZoomState>().show_text_layer;

    let mut pinch_start = use_signal(|| None::<PinchState>);
    let mut mounted_el: Signal<Option<Rc<MountedData>>> = use_signal(|| None);
    let mut viewport_rect = use_signal(|| (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64));
    let page_dims = use_signal(|| (1.0_f64, 1.0_f64));
    let mut pdf_loaded = use_signal(|| false);
    let mut last_refresh = use_signal(|| 0_u32);
    let mut last_scale = use_signal(|| 0.0_f64);
    let content_rect = use_signal(|| None::<(f64, f64, f64, f64)>);
    let page_indicator_hide_task: Signal<Option<Task>> = use_signal(|| None);

    // 配置 pdf.js worker 与 cmaps 路径（主库已随 TS bundle 注入）
    use_effect(|| {
        let worker_url = asset!("/ts/dist/pdfjs/pdf.worker.min.mjs").to_string();
        let cmaps_url = format!(
            "{}/",
            asset!("/ts/dist/pdfjs/cmaps", AssetOptions::folder())
        );
        js_api::pdf_preview::configure::<DioxusBridge>(&worker_url, &cmaps_url);
    });

    use_effect(move || {
        let visible = show_text_layer();
        toggle_pdf_text_layer_debug(visible);
    });

    // 组件卸载时清理 PDF 资源
    use_drop(js_api::pdf_preview::cleanup::<DioxusBridge>);

    // 第一步：异步读取 PDF 字节（不需要 DOM）
    let mut pdf_resource: Resource<Result<PdfBytesResult, String>> = use_resource(move || {
        let fp = fp.clone();
        async move {
            let bytes = tokio::task::spawn_blocking(move || {
                let path = Path::new(&fp);
                tracing::info!("PDF 读取开始: {path:?}");
                let result = std::fs::read(path);
                match &result {
                    Ok(b) => tracing::info!("PDF 读取成功: {} bytes", b.len()),
                    Err(e) => tracing::error!("PDF 读取失败: {e}"),
                }
                result
            })
            .await
            .map_err(|e| format!("线程错误: {e}"))?
            .map_err(|e| format!("读取失败: {e}"))?;

            Ok(PdfBytesResult { bytes })
        }
    });

    // 第二步：字节就绪 → 调用 JS load → 轮询 pageInfos → 更新状态
    // 容器始终渲染在 DOM 中，所以 load_pdf 可以立即找到容器
    use_effect(move || {
        if let Some(Ok(r)) = pdf_resource() {
            let bytes = r.bytes.clone();
            let mut page_dims = page_dims;
            let mut pdf_loaded = pdf_loaded;
            let fit_scale = fit_scale;
            let content_fit_scale = content_fit_scale;
            let scale = scale;
            let zoom_mode = zoom_mode;
            let center_request = center_request;
            let mounted_el = mounted_el;
            let mut content_rect = content_rect;
            spawn(async move {
                tracing::info!("PDF bytes 就绪, {} bytes, 开始加载到 JS", bytes.len());
                let b64 = general_purpose::STANDARD.encode(&bytes);
                tracing::info!("PDF base64 编码完成, {} chars", b64.len());
                js_api::pdf_preview::render_load::<DioxusBridge>(&b64, CONTAINER_ID, 1.0).await;

                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
                loop {
                    if std::time::Instant::now() >= deadline {
                        tracing::error!("PDF 加载超时：pageInfos 轮询 15 秒未返回");
                        break;
                    }
                    let json_str = js_api::pdf_preview::query_page_infos::<DioxusBridge>().await;
                    let val = serde_json::from_str(&json_str).unwrap_or(serde_json::Value::Null);
                    {
                        if val.is_null() {
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                            continue;
                        }
                        if let (Some(w), Some(h)) = (
                            val.get("w").and_then(|v| v.as_f64()),
                            val.get("h").and_then(|v| v.as_f64()),
                        ) {
                            page_dims.set((w, h));
                            let crect = val.get("cr").and_then(|v| {
                                if v.is_null() {
                                    return None;
                                }
                                let l = v.get("left")?.as_f64()?;
                                let t = v.get("top")?.as_f64()?;
                                let cw = v.get("width")?.as_f64()?;
                                let ch = v.get("height")?.as_f64()?;
                                Some((l, t, cw, ch))
                            });
                            content_rect.set(crect);
                            if let Some(el) = mounted_el() {
                                let el_clone = el.clone();
                                let mut fit_scale = fit_scale;
                                let mut content_fit_scale = content_fit_scale;
                                let mut scale = scale;
                                let mut zoom_mode = zoom_mode;
                                let mut center_request = center_request;
                                let mut pdf_loaded = pdf_loaded;
                                spawn(async move {
                                    if let Ok(rect) = el_clone.get_client_rect().await {
                                        let avail_w = (rect.width() - 16.0).max(1.0);
                                        let paper_fit =
                                            (avail_w / w).min(rect.height() / h).clamp(0.3, 5.0);
                                        // 与 Typst 同构：有内容矩形即按 viewport 公式
                                        // （下限兜底防过度放大），无则整页宽
                                        let content_w = crect
                                            .map_or(w, |(l, _, cw, _)| {
                                                ContentFitViewport::width(cw, l, w)
                                            });
                                        let content_fit = (avail_w / content_w).clamp(0.3, 5.0);
                                        fit_scale.set(paper_fit);
                                        content_fit_scale.set(content_fit);
                                        if (scale() - 1.0).abs() < 0.001 {
                                            scale.set(content_fit);
                                            zoom_mode.set(ZoomMode::ContentFit);
                                            center_request.set(center_request() + 1);
                                        }
                                    }
                                    pdf_loaded.set(true);
                                    inject_pdf_page_numbers(CONTAINER_ID);
                                    hide_all_page_numbers();
                                });
                            } else {
                                pdf_loaded.set(true);
                                inject_pdf_page_numbers(CONTAINER_ID);
                                hide_all_page_numbers();
                            }
                            break;
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            });
        }
    });

    // scale 变化时通知 JS 重渲染 canvas
    use_effect(move || {
        let s = scale();
        let ls = last_scale();
        let loaded = pdf_loaded();
        tracing::info!("PDF scale effect: s={s}, last_scale={ls}, pdf_loaded={loaded}");
        if loaded && (s - ls).abs() > 0.001 {
            last_scale.set(s);
            tracing::info!("PDF scale effect → set_pdf_scale({s})");
            js_api::pdf_preview::render_set_scale::<DioxusBridge>(s);
        }
    });

    // 横滚吸附：内容适应模式下，松手后自动吸附到内容左/右边缘对齐屏幕边
    use_effect(move || {
        let s = scale();
        let mode = zoom_mode();
        inject_snap(page_dims().0, s, mode == ZoomMode::ContentFit, *content_rect.peek());
    });

    use_effect(move || {
        center_request();
        let s = *scale.peek();
        scroll_to_page_center(page_dims().0, s, *content_rect.peek());
    });

    // refresh_trigger 变化时重新读取 PDF
    use_effect(move || {
        let t = refresh_trigger();
        if t != last_refresh() {
            last_refresh.set(t);
            pdf_resource.restart();
            pdf_loaded.set(false);
        }
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
                    >(CONTAINER_ID, fx, fy, vl, vt, s0, dist, pw);
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
                    last_scale.set(0.0);
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
                last_scale.set(0.0);
                scale.set(new_scale);
                zoom_mode.set(new_mode);
                if new_mode == ZoomMode::ContentFit {
                    center_request.set(center_request() + 1);
                }
                on_preview_interact(page_indicator_hide_task);
            },
            onscroll: move |_| {
                on_preview_interact(page_indicator_hide_task);
            },
            {
                let s = scale();
                let container_w = page_w * s;
                tracing::info!(
                    "PDF render: scale={s}, page_w={page_w}, container_w={container_w}"
                );
                rsx! {
                    div {
                        class: styles::zoom_container,
                        id: "{CONTAINER_ID}",
                        style: "width: {container_w}px;",

                        match pdf_resource() {
                            None => rsx! {
                                div { class: styles::preview_loading, { i18n_support::translate(&i18n_ctx, "common-loading") } }
                            },
                            Some(Err(err)) => rsx! {
                                div { class: styles::preview_error, "{err}" }
                            },
                            Some(Ok(_)) => rsx! {},
                        }
                    }
                }
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

fn inject_snap(page_w_pt: f64, scale: f64, is_content_mode: bool, crect: Option<(f64, f64, f64, f64)>) {
    let page_w_px = page_w_pt * scale;
    let active = is_content_mode;
    // 吸附目标跟随内容包围盒（与 Typst 一致）；无内容矩形时退回整页
    let (content_l_px, content_w_px) = crect
        .map(|(l, _, cw, _)| (l * scale, cw * scale))
        .unwrap_or((0.0, page_w_px));
    js_api::pinch_zoom::inject_snap::<DioxusBridge>(
        CONTAINER_ID, content_l_px, content_w_px, active,
    );
}

fn scroll_to_page_center(page_w_pt: f64, scale: f64, crect: Option<(f64, f64, f64, f64)>) {
    let page_w_px = page_w_pt * scale;
    // 水平滚到内容中心（与 Typst 一致）；无内容矩形时按整页居中
    let center_px = crect.map(|(l, _, cw, _)| (l + cw / 2.0) * scale);
    js_api::pinch_zoom::scroll_to_content_center::<DioxusBridge>(
        CONTAINER_ID,
        page_w_px,
        center_px,
    );
}

fn toggle_pdf_text_layer_debug(visible: bool) {
    // debug：让透明文字层可见（红字 + 黄底），用于核对选中文字与 canvas 渲染是否对齐
    js_api::pinch_zoom::toggle_text_layer_debug::<DioxusBridge>(
        CONTAINER_ID,
        "__pdf-tl-debug-style",
        visible,
        ".textLayer span { color: rgba(255,0,0,0.35) !important; } .textLayer { background: rgba(255,255,0,0.08) !important; }",
    );
}
