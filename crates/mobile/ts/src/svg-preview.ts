import type { AppWindow } from "./window-types";

const w = window as unknown as AppWindow;

/** 重置 SVG 预览状态（页面切换时调用）。 */
export function resetSvgState(): void {
  w.__svgT = null;
  w.__svgPinch = null;
  w.__svgDrag = null;
}

/** 以视口中心为焦点缩放 SVG：保持焦点不变，计算新偏移。 */
export function applySvgScale(
  cid: string,
  s: number,
  vl: number,
  vt: number,
  vw: number,
  vh: number,
): void {
  const zc = document.getElementById(cid);
  if (!zc) return;
  let t = w.__svgT;
  if (t && Math.abs(t.s - s) < 0.0001) return;
  if (!t) {
    t = { s: 1, tx: 0, ty: 0 };
    w.__svgT = t;
  }
  const cx = (vl + vw / 2 - vl - t.tx) / t.s;
  const cy = (vt + vh / 2 - vt - t.ty) / t.s;
  const ntx = vl + vw / 2 - cx * s;
  const nty = vt + vh / 2 - cy * s;
  zc.style.transform = `translate(${ntx}px,${nty}px) scale(${s})`;
  w.__svgT = { s, tx: ntx, ty: nty };
}

/** 适配缩放：直接设置缩放比例和偏移（fit-to-width 等）。 */
export function applySvgFit(
  cid: string,
  fit: number,
  tx: number,
  ty: number,
): void {
  const zc = document.getElementById(cid);
  if (!zc) return;
  zc.style.transform = `translate(${tx}px,${ty}px) scale(${fit})`;
  w.__svgT = { s: fit, tx, ty };
}

/**
 * 读取 SVG 内容包围盒（图形实际绘制的上下左右边缘）。
 *
 * getBBox 返回 viewBox 用户坐标；svg 以 xMinYMin meet 1:1 映射到元素
 * （容器宽 = viewBox 宽），故元素 px 坐标 = 用户坐标 − viewBox 原点。
 * 空内容（宽高为 0）或测量失败返回 "null"，调用方退回画布适应。
 */
export function getSvgContentBBox(cid: string): string {
  const svg = document.getElementById(cid)?.querySelector("svg");
  if (!svg) return "null";
  try {
    const b = svg.getBBox();
    const vb = svg.viewBox.baseVal;
    return JSON.stringify({
      x: b.x - vb.x,
      y: b.y - vb.y,
      width: b.width,
      height: b.height,
    });
  } catch {
    return "null";
  }
}

/** SVG 捏合开始：锁定落指瞬间双指中点处的内容点。
 *
 * 跟手策略（物理接触模型）：落指瞬间两指「捏住」内容点 cx/cy，之后每帧
 * 把这个点投到**实时双指中点**下——捏住的内容点跟随中点移动与缩放，
 * 过程无漂移，松手后无需任何校正。
 */
export function svgPinchStart(
  cid: string,
  fx: number,
  fy: number,
  vl: number,
  vt: number,
): void {
  const zc = document.getElementById(cid);
  if (!zc) return;
  const t = w.__svgT ?? { s: 1, tx: 0, ty: 0 };
  w.__svgPinch = {
    vl,
    vt,
    cx: (fx - vl - t.tx) / t.s,
    cy: (fy - vt - t.ty) / t.s,
    el: zc,
  };
}

/** SVG 捏合移动：把锁定内容点投到实时双指中点下，按新缩放重排偏移。
 *
 * 最终帧与松手时 Rust 侧 scale.set 的值一致，applySvgScale 的同值
 * 早退保证松手瞬间 transform 不再被改写。
 */
export function svgPinchMove(ns: number, fx: number, fy: number): void {
  const p = w.__svgPinch;
  if (!p) return;
  const ntx = fx - p.vl - p.cx * ns;
  const nty = fy - p.vt - p.cy * ns;
  p.el.style.transform = `translate(${ntx}px,${nty}px) scale(${ns})`;
  w.__svgT = { s: ns, tx: ntx, ty: nty };
}

/** SVG 拖拽开始：记录起始触摸点和当前偏移。 */
export function svgDragStart(_cid: string, x: number, y: number): void {
  const t = w.__svgT ?? { s: 1, tx: 0, ty: 0 };
  w.__svgDrag = {
    sx: x,
    sy: y,
    tx0: t.tx,
    ty0: t.ty,
  };
}

/** SVG 拖拽移动：根据位移量更新偏移。 */
export function svgDragMove(_cid: string, x: number, y: number): void {
  const d = w.__svgDrag;
  if (!d) return;
  const t = w.__svgT ?? { s: 1, tx: 0, ty: 0 };
  const dx = x - d.sx,
    dy = y - d.sy;
  const ntx = d.tx0 + dx;
  const nty = d.ty0 + dy;
  const zc = document.getElementById(_cid);
  if (!zc) return;
  zc.style.transform = `translate(${ntx}px,${nty}px) scale(${t.s})`;
  w.__svgT = { s: t.s, tx: ntx, ty: nty };
}

/** 清理 SVG 预览状态（页面切换时调用）。 */
export function cleanupSvgPreview(): void {
  w.__svgT = null;
  w.__svgPinch = null;
  w.__svgDrag = null;
}
