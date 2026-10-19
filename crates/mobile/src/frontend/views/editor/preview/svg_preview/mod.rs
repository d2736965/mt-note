use std::path::Path;
use std::rc::Rc;
use std::sync::LazyLock;

use dioxus::logger::tracing;
use dioxus::prelude::*;
use dioxus_i18n::prelude::i18n;

use crate::frontend::i18n as i18n_support;
use crate::frontend::js_bridge::{DioxusBridge, js_api};
use crate::frontend::views::editor::{ZoomMode, ZoomState};

stylance::import_style!(styles, "svg_preview.module.scss");

const CONTAINER_ID: &str = "svg-zoom-root";

type SvgLoadResult = Result<String, String>;

#[derive(Clone, PartialEq)]
struct PinchState {
    initial_distance: f64,
    initial_scale: f64,
    last_dist: f64,
}

#[derive(Clone, PartialEq, Default)]
struct DragState;

#[component]
pub fn SvgPreview(file_path: String) -> Element {
    let i18n_ctx = i18n();
    let fp = file_path.clone();

    let mut scale = use_context::<ZoomState>().scale;
    let mut fit_scale = use_context::<ZoomState>().fit_scale;
    let mut content_fit_scale = use_context::<ZoomState>().content_fit_scale;
    let mut zoom_mode = use_context::<ZoomState>().zoom_mode;
    let refresh_trigger = use_context::<ZoomState>().refresh_trigger;

    let mut pinch_start = use_signal(|| None::<PinchState>);
    let mut drag_start = use_signal(|| None::<DragState>);
    let mut mounted_el: Signal<Option<Rc<MountedData>>> = use_signal(|| None);
    let mut viewport_rect = use_signal(|| (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64));
    let mut svg_dims = use_signal(|| (300.0_f64, 150.0_f64));
    let mut needs_fit = use_signal(|| 0_usize);
    let mut last_refresh = use_signal(|| 0_u32);

    let mut svg_resource: Resource<SvgLoadResult> = use_resource(move || {
        let fp = fp.clone();
        load_svg(fp)
    });

    use_effect(move || {
        let trigger = refresh_trigger();
        if trigger != last_refresh() {
            last_refresh.set(trigger);
            js_api::svg_preview::reset_state::<DioxusBridge>();
            svg_resource.restart();
        }
    });

    use_effect(move || {
        let s = scale();
        let (vl, vt, vw, vh) = viewport_rect();
        js_api::svg_preview::apply_scale::<DioxusBridge>(CONTAINER_ID, s, vl, vt, vw, vh);
    });

    let (svg_w, _svg_h) = svg_dims();

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
                    drag_start.set(None);
                    let p0 = touches[0].client_coordinates();
                    let p1 = touches[1].client_coordinates();
                    let fx = (p0.x + p1.x) / 2.0;
                    let fy = (p0.y + p1.y) / 2.0;
                    let (vl, vt, _vw, _vh) = viewport_rect();
                    let dist = touch_distance(&touches[0], &touches[1]);
                    pinch_start.set(Some(PinchState {
                        initial_distance: dist,
                        initial_scale: scale(),
                        last_dist: dist,
                    }));
                    js_api::svg_preview::pinch_start::<DioxusBridge>(CONTAINER_ID, fx, fy, vl, vt);
                } else if touches.len() == 1 {
                    // 防御：正常流程 touchend/touchcancel 已清 pinch 状态；若事件
                    // 丢失残留，这里清掉，避免单指手势失效、抬手按陈旧状态错算
                    pinch_start.set(None);
                    let p = touches[0].client_coordinates();
                    js_api::svg_preview::drag_start::<DioxusBridge>(CONTAINER_ID, p.x, p.y);
                    drag_start.set(Some(DragState));
                }
            },
            ontouchmove: move |e: Event<TouchData>| {
                let touches = e.data.touches();
                if touches.len() == 2 {
                    drag_start.set(None);
                    if let Some(ps) = pinch_start.write().as_mut() {
                        let dist = touch_distance(&touches[0], &touches[1]);
                        ps.last_dist = dist;
                        let ratio = dist / ps.initial_distance.max(1.0);
                        let ns = (ps.initial_scale * ratio).clamp(0.3, 10.0);
                        // 焦点=实时双指中点：中点抓着的内容点跟随中点（跟手）
                        let p0 = touches[0].client_coordinates();
                        let p1 = touches[1].client_coordinates();
                        let fx = (p0.x + p1.x) / 2.0;
                        let fy = (p0.y + p1.y) / 2.0;
                        js_api::svg_preview::pinch_move::<DioxusBridge>(ns, fx, fy);
                    }
                } else if touches.len() == 1 && pinch_start().is_none() {
                    let p = touches[0].client_coordinates();
                    if drag_start().is_none() {
                        // 捏合抬一指后的接续拖动没有 touchstart，在此初始化
                        // 拖拽起点——否则 drag_move 会用上一次拖拽残留的旧
                        // 起点/偏移，松手瞬间内容瞬移
                        drag_start.set(Some(DragState));
                        js_api::svg_preview::drag_start::<DioxusBridge>(CONTAINER_ID, p.x, p.y);
                    } else {
                        js_api::svg_preview::drag_move::<DioxusBridge>(CONTAINER_ID, p.x, p.y);
                    }
                }
            },
            ontouchend: move |_| {
                if let Some(p) = pinch_start.take() {
                    // 与 pinch_move 完全相同的浮点顺序，保证 scale.set 的值
                    // 与 JS 已应用的 transform 一致（applySvgScale 同值早退）
                    let ratio = p.last_dist / p.initial_distance.max(1.0);
                    let ns = (p.initial_scale * ratio).clamp(0.3, 10.0);
                    scale.set(ns);
                    if (ns - p.initial_scale).abs() > f64::EPSILON {
                        zoom_mode.set(ZoomMode::Free);
                    }
                } else {
                    drag_start.set(None);
                }
            },
            // 系统打断（来电/通知/边缘手势）：手势作废，transform 定格在
            // 已应用状态（last_dist 半新不旧不可信），只清状态防残留
            ontouchcancel: move |_| {
                pinch_start.take();
                drag_start.set(None);
            },
            match svg_resource() {
                None => rsx! {
                    div { class: styles::preview_loading, { i18n_support::translate(&i18n_ctx, "common-loading") } }
                },
                    Some(Ok(svg)) => {
                    let fingerprint = svg.len();
                    if fingerprint != needs_fit()
                        && let Some(el) = mounted_el()
                    {
                        needs_fit.set(fingerprint);
                        let el_clone = el.clone();
                        let svg_str = svg.clone();
                        spawn(async move {
                            let (w, h) = extract_svg_size(&svg_str);
                            if let Ok(rect) = el_clone.get_client_rect().await {
                                let (vw, vh) = (rect.width(), rect.height());
                                // 画布适应（viewBox 含留白，纸张语义）
                                let fit = (vw / w).min(vh / h).clamp(0.3, 5.0);
                                // 内容包围盒实测；空内容/测量失败退回画布居中
                                let bbox_json = js_api::svg_preview::get_svg_content_b_box::<
                                    DioxusBridge,
                                >(CONTAINER_ID)
                                .await;
                                let bbox: Option<(f64, f64, f64, f64)> =
                                    serde_json::from_str::<serde_json::Value>(&bbox_json)
                                        .ok()
                                        .and_then(|v| {
                                            let bx = v["x"].as_f64()?;
                                            let by = v["y"].as_f64()?;
                                            let bw = v["width"].as_f64()?;
                                            let bh = v["height"].as_f64()?;
                                            (bw > 0.0 && bh > 0.0)
                                                .then_some((bx, by, bw, bh))
                                        });
                                let (content_fit, tx, ty) = match bbox {
                                    Some((bx, by, bw, bh)) => {
                                        SvgContentFit::solve(bx, by, bw, bh, vw, vh)
                                    }
                                    None => {
                                        let tx = (vw - w * fit) / 2.0;
                                        let ty = (vh - h * fit) / 2.0;
                                        (fit, tx, ty)
                                    }
                                };
                                fit_scale.set(fit);
                                content_fit_scale.set(content_fit);
                                svg_dims.set((w, h));
                                scale.set(content_fit);
                                zoom_mode.set(ZoomMode::ContentFit);
                                js_api::svg_preview::apply_fit::<
                                    DioxusBridge,
                                >(CONTAINER_ID, content_fit, tx, ty);
                            }
                        });
                    }
                    rsx! {
                        div {
                            class: styles::zoom_container,
                            id: "{CONTAINER_ID}",
                            style: "width: {svg_w}px;",
                            dangerous_inner_html: svg,
                        }
                    }
                }
                Some(Err(err)) => rsx! {
                    div { class: styles::preview_error, "{err}" }
                },
            }
        }
    }
}

fn touch_distance(t0: &TouchPoint, t1: &TouchPoint) -> f64 {
    let p0 = t0.client_coordinates();
    let p1 = t1.client_coordinates();
    let dx = p0.x - p1.x;
    let dy = p0.y - p1.y;
    (dx * dx + dy * dy).sqrt()
}

/// SVG「内容适应」打开策略：内容包围盒两轴 contain 于视口，放大封顶。
struct SvgContentFit;

impl SvgContentFit {
    /// 放大上限（内容固有尺寸的倍数）：fit 倍数超过时截断，
    /// 防止小图形被放大到铺满视口的夸张倍数；缩小方向不受限。
    const MAX_UPSCALE: f64 = 2.0;

    /// 由内容包围盒 `(bx, by, bw, bh)`（元素 px 坐标）与视口
    /// `(vw, vh)` 求 `(scale, tx, ty)`：scale 取「两轴贴视口边缘的
    /// contain 倍数」与上限的较小者；平移使内容中心落在视口中心。
    fn solve(bx: f64, by: f64, bw: f64, bh: f64, vw: f64, vh: f64) -> (f64, f64, f64) {
        let s = (vw / bw)
            .min(vh / bh)
            .min(Self::MAX_UPSCALE)
            .clamp(0.3, 5.0);
        let tx = vw / 2.0 - s * (bx + bw / 2.0);
        let ty = vh / 2.0 - s * (by + bh / 2.0);
        (s, tx, ty)
    }
}

async fn load_svg(file_path: String) -> SvgLoadResult {
    let fp = file_path.clone();
    let bytes = tokio::task::spawn_blocking(move || {
        let path = Path::new(&fp);
        std::fs::read(path)
    })
    .await
    .map_err(|e| format!("线程错误: {e}"))?
    .map_err(|e| format!("读取失败: {e}"))?;

    let raw = String::from_utf8(bytes).map_err(|e| format!("编码错误: {e}"))?;

    sanitize_svg(&raw)
}

static RE_SCRIPT: LazyLock<Result<regex::Regex, regex::Error>> =
    LazyLock::new(|| regex::Regex::new(r"(?is)<script[^>]*>.*?</script\s*>"));

static RE_EVENT_ATTR: LazyLock<Result<regex::Regex, regex::Error>> =
    LazyLock::new(|| regex::Regex::new(r#"\s+on[a-zA-Z]+\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]*)"#));

fn sanitize_svg(raw: &str) -> SvgLoadResult {
    let script_re = RE_SCRIPT
        .as_ref()
        .map_err(|e| format!("script regex 编译失败: {e}"))?;
    let event_re = RE_EVENT_ATTR
        .as_ref()
        .map_err(|e| format!("event attr regex 编译失败: {e}"))?;

    let step1 = script_re.replace_all(raw, "");
    let step2 = event_re.replace_all(&step1, "");
    let cleaned = step2.to_string();

    let (w, h) = extract_svg_size(&cleaned);
    if w > 0.0 && h > 0.0 {
        normalize_svg_size(&cleaned, w, h)
    } else {
        Ok(cleaned)
    }
}

static RE_VIEWBOX: LazyLock<Result<regex::Regex, regex::Error>> =
    LazyLock::new(|| regex::Regex::new(r#"viewBox\s*=\s*"([^"]*)""#));

static RE_SVG_WIDTH_ATTR: LazyLock<Result<regex::Regex, regex::Error>> =
    LazyLock::new(|| regex::Regex::new(r#"<svg[^>]*?\swidth\s*=\s*"([^"]*)""#));

static RE_SVG_HEIGHT_ATTR: LazyLock<Result<regex::Regex, regex::Error>> =
    LazyLock::new(|| regex::Regex::new(r#"<svg[^>]*?\sheight\s*=\s*"([^"]*)""#));

fn extract_svg_size(svg: &str) -> (f64, f64) {
    let vb_re = match RE_VIEWBOX.as_ref() {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("viewBox regex: {e}");
            return (300.0, 150.0);
        }
    };
    let w_re = match RE_SVG_WIDTH_ATTR.as_ref() {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("width regex: {e}");
            return (300.0, 150.0);
        }
    };
    let h_re = match RE_SVG_HEIGHT_ATTR.as_ref() {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("height regex: {e}");
            return (300.0, 150.0);
        }
    };

    let (vb_w, vb_h) = if let Some(caps) = vb_re.captures(svg) {
        if let Some(m) = caps.get(1) {
            let parts: Vec<&str> = m.as_str().split_whitespace().collect();
            if parts.len() >= 4 {
                (
                    parts[2].parse::<f64>().unwrap_or(0.0),
                    parts[3].parse::<f64>().unwrap_or(0.0),
                )
            } else {
                (0.0, 0.0)
            }
        } else {
            (0.0, 0.0)
        }
    } else {
        (0.0, 0.0)
    };

    if vb_w > 0.0 && vb_h > 0.0 {
        return (vb_w, vb_h);
    }

    let attr_w = w_re
        .captures(svg)
        .and_then(|c| c.get(1))
        .and_then(|m| parse_svg_length(m.as_str()));
    let attr_h = h_re
        .captures(svg)
        .and_then(|c| c.get(1))
        .and_then(|m| parse_svg_length(m.as_str()));

    let w = attr_w.unwrap_or(0.0);
    let h = attr_h.unwrap_or(0.0);

    if w > 0.0 && h > 0.0 {
        (w, h)
    } else {
        (300.0, 150.0)
    }
}

fn parse_svg_length(s: &str) -> Option<f64> {
    let s = s.trim();
    let num_str: String = s
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == 'e' || *c == 'E')
        .collect();
    let val: f64 = num_str.parse().ok()?;
    if val <= 0.0 {
        return None;
    }
    let unit: String = s.chars().skip(num_str.len()).collect();
    match unit.as_str() {
        "" | "px" => Some(val),
        "pt" => Some(val * 1.3333),
        "mm" => Some(val * 3.7795),
        "cm" => Some(val * 37.795),
        "in" => Some(val * 96.0),
        _ => None,
    }
}

static RE_SVG_WIDTH: LazyLock<Result<regex::Regex, regex::Error>> =
    LazyLock::new(|| regex::Regex::new(r#"(<svg[^>]*?)\s+width\s*=\s*"[^"]*""#));

static RE_SVG_HEIGHT: LazyLock<Result<regex::Regex, regex::Error>> =
    LazyLock::new(|| regex::Regex::new(r#"(<svg[^>]*?)\s+height\s*=\s*"[^"]*""#));

static RE_SVG_PRESERVE: LazyLock<Result<regex::Regex, regex::Error>> =
    LazyLock::new(|| regex::Regex::new(r#"(<svg[^>]*?)\s+preserveAspectRatio\s*=\s*"[^"]*""#));

static RE_SVG_OPEN: LazyLock<Result<regex::Regex, regex::Error>> =
    LazyLock::new(|| regex::Regex::new(r"(<svg)"));

fn normalize_svg_size(svg: &str, w: f64, h: f64) -> SvgLoadResult {
    let width_re = RE_SVG_WIDTH
        .as_ref()
        .map_err(|e| format!("svg width regex 编译失败: {e}"))?;
    let height_re = RE_SVG_HEIGHT
        .as_ref()
        .map_err(|e| format!("svg height regex 编译失败: {e}"))?;
    let preserve_re = RE_SVG_PRESERVE
        .as_ref()
        .map_err(|e| format!("svg preserve regex 编译失败: {e}"))?;
    let vb_re = RE_VIEWBOX
        .as_ref()
        .map_err(|e| format!("viewBox regex 编译失败: {e}"))?;
    let open_re = RE_SVG_OPEN
        .as_ref()
        .map_err(|e| format!("svg open regex 编译失败: {e}"))?;

    let step1 = width_re.replace_all(svg, "$1");
    let step2 = height_re.replace_all(&step1, "$1");
    let step3 = preserve_re.replace_all(&step2, "$1");

    let has_viewbox = vb_re.is_match(svg);

    let result = if !has_viewbox {
        open_re
            .replace(
                &step3,
                format!(
                    r#"<svg viewBox="0 0 {w:.2} {h:.2}" width="100%" preserveAspectRatio="xMinYMin meet""#
                ),
            )
            .to_string()
    } else {
        open_re
            .replace(
                &step3,
                r#"<svg width="100%" preserveAspectRatio="xMinYMin meet""#,
            )
            .to_string()
    };

    Ok(result)
}
