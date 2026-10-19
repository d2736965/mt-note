import type { AppWindow } from "./window-types";

const w = window as unknown as AppWindow;

/** 捏合开始：记录初始状态（焦点位置、滚动位置、缩放基准）。 */
export function pinchStart(
  elementId: string,
  fx: number,
  fy: number,
  vl: number,
  vt: number,
  s0: number,
  dist: number,
  pw: number,
): void {
  const preview = document.getElementById(elementId)?.parentElement;
  if (!preview) return;
  w.__afterPinch = true;
  w.__snapActive = false;
  const sx = preview.scrollLeft;
  const sy = preview.scrollTop;
  const el = document.getElementById(elementId);
  if (!el) return;
  w.__pinch = {
    sx,
    sy,
    fx,
    fy,
    vl,
    vt,
    scale0: s0,
    dist0: dist,
    pageW: pw,
    ox: fx - vl + sx,
    oy: fy - vt + sy,
    el,
    ns: s0,
  };
}

/** 捏合过程中：用 CSS transform scale 实时缩放，不动 DOM 尺寸。 */
export function pinchMove(ns: number, k: number): void {
  const p = w.__pinch;
  if (!p) return;
  p.ns = ns;
  p.el.style.transform = `scale(${k})`;
  p.el.style.transformOrigin = `${p.ox}px ${p.oy}px`;
}

/**
 * 捏合结束：清除 transform，将缩放应用到实际 DOM 尺寸（width），
 * 并同步滚动位置使焦点不动。PDF 预览额外更新 canvas/textLayer 尺寸。
 */
export function pinchEnd(ns: number): void {
  w.__afterPinch = true;
  w.__snapActive = false;
  const p = w.__pinch;
  if (!p) return;
  const zc = p.el;
  const preview = zc.parentElement!;
  const slBefore = preview.scrollLeft;
  const stBefore = preview.scrollTop;
  const swBefore = preview.scrollWidth;
  const cwBefore = preview.clientWidth;
  const cx = p.ox / p.scale0;
  const cy = p.oy / p.scale0;
  const vx = p.fx - p.vl;
  const vy = p.fy - p.vt;
  zc.style.transform = "none";
  zc.style.width = `${ns * p.pageW}px`;
  // typst 预览的 text_layer 靠自身 scale(s) 与 svg 布局宽（=page_w*s）保持同因子。
  // 上面 width 已同步生效，而 Rust 重渲染 scale(ns) 是异步的——不立即跟上的话，
  // 窗口期内 svg 因子是 ns、文字层因子还是 s0，文字层整体错位（页码越靠后越明显）。
  // Rust 渲染到达后写入相同值，幂等。PDF 预览无此结构（类名不同），天然跳过。
  const tlEl = zc.children[1] as HTMLElement | undefined;
  if (tlEl && /text_layer/.test(tlEl.className)) {
    tlEl.style.transform = `scale(${ns})`;
  }
  const pdfPreview = w.__pdfPreview;
  let pageW2: number | null = null;
  let pageH2: number | null = null;
  if (pdfPreview?.query?.pageInfos) {
    const raw = pdfPreview.query.pageInfos();
    try {
      const parsed = JSON.parse(raw);
      if (parsed && parsed.w != null) {
        pageW2 = parsed.w;
        pageH2 = parsed.h;
      }
    } catch {
      // ignore parse errors
    }
  }
  const canvases = zc.querySelectorAll("canvas");
  const textLayers = zc.querySelectorAll(".textLayer");
  if (pageW2 != null && pageH2 != null) {
    for (let i = 0; i < canvases.length; i++) {
      (canvases[i] as HTMLElement).style.width = `${pageW2 * ns}px`;
      (canvases[i] as HTMLElement).style.height = `${pageH2 * ns}px`;
    }
    for (let i = 0; i < textLayers.length; i++) {
      (textLayers[i] as HTMLElement).style.width = `${pageW2 * ns}px`;
      (textLayers[i] as HTMLElement).style.height = `${pageH2 * ns}px`;
    }
  }
  const targetSL = cx * ns - vx;
  const targetST = cy * ns - vy;
  console.log(
    "[PINCH-END]",
    "ns:",
    ns.toFixed(4),
    "s0:",
    p.scale0.toFixed(4),
    "pageW:",
    p.pageW.toFixed(1),
    "ox:",
    p.ox.toFixed(1),
    "slBefore:",
    slBefore.toFixed(1),
    "stBefore:",
    stBefore.toFixed(1),
    "targetSL:",
    targetSL.toFixed(1),
    "swBefore:",
    swBefore.toFixed(1),
    "newWidth:",
    (ns * p.pageW).toFixed(1),
    "cw:",
    cwBefore.toFixed(1),
  );
  preview.scrollLeft = targetSL;
  preview.scrollTop = targetST;
  const slAfter = preview.scrollLeft;
  console.log(
    "[PINCH-END] after set: slAfter:",
    slAfter.toFixed(1),
    "diff:",
    (slAfter - slBefore).toFixed(1),
    "swAfter:",
    preview.scrollWidth.toFixed(1),
  );
  w.__pinch = null;
}

/** 重置 snap 状态：捏合结束后重新激活 snap 吸附。 */
export function resetSnap(): void {
  console.log("[PINCH] resetSnap called, was afterPinch:", w.__afterPinch);
  w.__afterPinch = false;
  w.__snapActive = !!w.__snapShouldBeActive;
}

/**
 * 注入 snap 吸附：内容适应模式下滚动停止后，一律平滑回到内容居中位。
 * viewport 按内容划定（内容不出 viewport），无需按内容边缘距离判断吸左/吸右。
 * @param mlp 内容左边界在预览容器中的偏移
 * @param cwp 内容宽度
 * @param active 是否激活 snap
 */
export function injectSnap(
  elementId: string,
  mlp: number,
  cwp: number,
  active: boolean,
): void {
  const zc = document.getElementById(elementId);
  if (!zc) return;
  const preview = zc.parentElement;
  if (!preview) return;
  if (!preview.__snapBound) {
    preview.__snapBound = true;
    const snap = () => {
      if (w.__afterPinch) return;
      const vw = preview.clientWidth;
      const maxSl = preview.scrollWidth - vw;
      if (maxSl <= 0) return;
      if (w.__snapCustomTarget != null) {
        const target = Math.max(0, Math.min(maxSl, w.__snapCustomTarget));
        if (Math.abs(preview.scrollLeft - target) < vw * 0.4) {
          preview.scrollTo({ left: target, behavior: "smooth" });
        }
        return;
      }
      if (!w.__snapActive) return;
      const target = Math.max(
        0,
        Math.min(maxSl, w.__snapMLP + w.__snapCWP / 2 - vw / 2),
      );
      preview.scrollTo({ left: target, behavior: "smooth" });
    };
    if ("onscrollend" in window) {
      preview.addEventListener("scrollend", snap);
    } else {
      let t: ReturnType<typeof setTimeout>;
      preview.addEventListener("scroll", () => {
        clearTimeout(t);
        t = setTimeout(snap, 120);
      });
    }
  }
  w.__snapMLP = mlp;
  w.__snapCWP = cwp;
  w.__snapShouldBeActive = active;
  if (!w.__afterPinch) {
    w.__snapActive = active;
  }
}

/** 设置自定义吸附目标：保存当前滚动位置，之后始终吸附到此位置。 */
export function setCustomSnapTarget(elementId: string): void {
  const zc = document.getElementById(elementId);
  if (!zc) return;
  const preview = zc.parentElement;
  if (!preview) return;
  w.__snapCustomTarget = preview.scrollLeft;
  console.log("[PINCH] setCustomSnapTarget:", w.__snapCustomTarget.toFixed(1));
}

/** 清除自定义吸附目标，恢复默认吸附行为。 */
export function clearCustomSnapTarget(_elementId: string): void {
  console.log("[PINCH] clearCustomSnapTarget");
  w.__snapCustomTarget = null;
}

/**
 * 水平滚动到内容居中位置，夹取在可滚动范围内（贴纸边时自然对齐屏幕边）。
 * @param pageWidth 页面缩放后的像素宽度
 * @param contentCenter 内容包围盒中心的缩放后像素坐标；null 时按整页居中
 */
export function scrollToContentCenter(
  elementId: string,
  pageWidth: number,
  contentCenter: number | null,
): void {
  console.log(
    "[PINCH] scrollToContentCenter called, pageWidth:",
    pageWidth.toFixed(1),
    "contentCenter:",
    contentCenter?.toFixed(1) ?? "null",
    "afterPinch:",
    w.__afterPinch,
  );
  const zc = document.getElementById(elementId);
  if (!zc) return;
  const preview = zc.parentElement;
  if (!preview) return;
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      const maxSl = preview.scrollWidth - preview.clientWidth;
      if (maxSl <= 0) return;
      const base = contentCenter ?? pageWidth / 2;
      const target = Math.max(
        0,
        Math.min(maxSl, base - preview.clientWidth / 2),
      );
      preview.scrollLeft = target;
    });
  });
}

/** 开关 textLayer 调试样式（为 textLayer 注入彩色背景，用于对齐调试）。 */
export function toggleTextLayerDebug(
  _elementId: string,
  styleId: string,
  visible: boolean,
  cssContent: string,
): void {
  const existing = document.getElementById(styleId);
  if (visible) {
    if (existing) return;
    const style = document.createElement("style");
    style.id = styleId;
    style.textContent = cssContent;
    document.head.appendChild(style);
  } else {
    if (existing) existing.remove();
  }
}

/** 对齐 SVG 与 textLayer：根据 SVG 子元素位置同步 textLayer 偏移，
 * 再按 pdfjs TextLayer 的方式精修每个 span 的摆位。 */
export function alignTextLayer(containerId: string): void {
  requestAnimationFrame(() => {
    const zc = document.getElementById(containerId);
    if (!zc) return;
    const svg = zc.children[0] as HTMLElement | undefined;
    const tl = zc.children[1] as HTMLElement | undefined;
    if (!svg || !tl) return;
    const transform = tl.style.transform;
    const match = transform && transform.match(/scale\(([^)]+)\)/);
    let s = match?.[1] ? parseFloat(match[1]) : 1;
    if (!s || s === 0) s = 1;
    for (let i = 0; i < svg.children.length && i < tl.children.length; i++) {
      const svgPage = svg.children[i] as HTMLElement;
      const textPage = tl.children[i] as HTMLElement;
      textPage.style.position = "absolute";
      textPage.style.top = `${svgPage.offsetTop / s}px`;
      textPage.style.left = "0";
    }
    // text_layer 容器内部 px 空间与 SVG viewBox（页面 pt）的换算系数
    const innerSvg = svg.querySelector("svg");
    const viewBoxW = innerSvg?.viewBox.baseVal.width ?? 0;
    const tlWidth = Number.parseFloat(tl.style.width);
    const k = viewBoxW > 0 && tlWidth > 0 ? tlWidth / viewBoxW : 4 / 3;
    const spans = tl.querySelectorAll<HTMLElement>("span");
    for (const span of spans) {
      tlLayoutSpan(span, k);
    }
    // span 的 font-family 指向 Rust 注入的 typst 运行时字体子集；这些
    // FontFace 惰性加载，加载完成前 metrics 量到的是 fallback——触发
    // 加载并在就绪后重精修一遍（Map 去重防循环）。
    const families = new Set<string>();
    for (const span of spans) {
      const family = span.style.fontFamily;
      if (family) families.add(family);
    }
    for (const family of families) {
      if (tlFontLoads.has(family)) continue;
      const load = document.fonts.load(`16px ${family}`);
      tlFontLoads.set(family, load);
      void load.then(() => {
        // 加载前 ratio 量到的是 fallback，已入缓存——清掉重量
        tlAscentRatio.delete(family);
        alignTextLayer(containerId);
      });
    }
  });
}

// 已触发过加载的 @font-face family → 加载 promise（去重，防止
// alignTextLayer → load.then → alignTextLayer 死循环）。
const tlFontLoads = new Map<string, Promise<FontFace[]>>();

// 文字层 span 的 font-family 由 Rust 侧内联为 typst 运行时命中的字体
// （子集 @font-face）；此处 sans-serif 仅作 family 缺失时的度量后备。
const TL_MEASURE_FAMILY = "sans-serif";
const tlAscentRatio = new Map<string, number>();

/** 实测 span（line-height:1）行盒内基线到盒顶的比例。canvas 的
 * fontBoundingBox 与 Chromium 行盒 strut 取的 metrics 来源不同，公式
 * 估算有每字体 0.005~0.012em 的系统差；这里用 inline-block 基线探针
 * 直接量：高 0 的 inline-block 底边就是对齐到的行盒基线。量不到时
 * 回退 0.8em。 */
function tlGetBaselineInBoxRatio(family: string): number {
  const cached = tlAscentRatio.get(family);
  if (cached !== undefined) return cached;
  let ratio = 0.8;
  const host = document.createElement("div");
  host.style.cssText =
    "position:absolute;left:0;top:0;visibility:hidden;line-height:1;" +
    `font-size:100px;font-family:${family};white-space:pre;`;
  const probe = document.createElement("span");
  probe.style.cssText = "display:inline-block;width:0;height:0;";
  host.appendChild(probe);
  host.appendChild(document.createTextNode("汉"));
  document.body.appendChild(host);
  const hostTop = host.getBoundingClientRect().top;
  const baseline = probe.getBoundingClientRect().bottom;
  if (hostTop !== 0 || baseline !== hostTop) {
    const measured = (baseline - hostTop) / 100;
    if (Number.isFinite(measured) && measured > 0.5 && measured < 1.2) {
      ratio = measured;
    }
  }
  host.remove();
  tlAscentRatio.set(family, ratio);
  return ratio;
}

/** 精修一个文字层 span：行盒基线精确落位。Rust 侧已把 TextItem 拆成
 * cluster 级 span（逐字/连字整体），起点由 data-x 钉死在 typst 精确位置，
 * 字符宽度差不跨字符累积；span 的 font-family 是 typst 实际命中的字体，
 * 基线 ratio 按各自字体度量（缓存于 tlAscentRatio）。data-x/y/fs 是页面
 * pt 原值，k 为 text_layer 内部 px 空间相对 pt 的系数；Rust 侧内联的
 * left/top/font-size 只是精修前的近似初值，这里全部覆盖。 */
function tlLayoutSpan(span: HTMLElement, k: number): void {
  const x = Number.parseFloat(span.dataset.x ?? "");
  const baselineY = Number.parseFloat(span.dataset.y ?? "");
  const fontSize = Number.parseFloat(span.dataset.fs ?? "");
  if ([x, baselineY, fontSize].some(Number.isNaN)) return;
  const fsPx = fontSize * k;
  const family = span.style.fontFamily || TL_MEASURE_FAMILY;
  span.style.left = `${x * k}px`;
  span.style.top = `${baselineY * k - fsPx * tlGetBaselineInBoxRatio(family)}px`;
  span.style.fontSize = `${fsPx}px`;
}

/** 清理捏合缩放状态（页面切换时调用）。 */
export function cleanupPinchZoom(): void {
  w.__pinch = null;
  w.__afterPinch = false;
  w.__snapActive = false;
  w.__snapShouldBeActive = false;
  w.__snapMLP = 0;
  w.__snapCWP = 0;
  w.__snapCustomTarget = null;
}
