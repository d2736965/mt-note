/**
 * Markdown 图形灯箱：全屏预览 + 相册级捏合缩放 / 单指平移，单击退出。
 *
 * - 预览对象：<img>（按 src 加载）与内联 <svg>（mermaid 图表、内嵌 svg 等）。
 *   SVG 走 cloneNode 复制进浮层——mermaid 用 foreignObject 承载文字标签，
 *   只有 DOM 内联渲染才生效，转 <img src=blob> 会整块丢失。
 * - 打开：markdown.ts 的委托点击监听调用 openLightbox（纯 JS 侧，无 Rust 参与）
 * - Typst 预览入口：initTypstPreviewLightbox 的文档级单击委托（模块加载时绑定
 *   一次），只命中 image() 嵌入的 <image> 元素（位图/矢量图都以 data URI 内嵌，
 *   直开 img）；单击延迟打开、双击让给预览的缩放切换
 * - 关闭：工具栏 ✕ 按钮，或 Android 返回键（Rust 侧 use_back_interceptor
 *   经 js-bridge 绑定调用 closeLightbox）
 * - 工具栏：单击浮层切换显隐（照片查看器式），手势一开始即让位淡出，
 *   无操作闲置 CHROME_IDLE_MS 后自动隐藏；含「适应宽度」（恢复打开时的
 *   初始缩放，s=1 contain 口径）与「1:1」（原始像素 100%）两个缩放按钮、
 *   「居中并钉住」二态按钮（单击 = 动画归中并锁定：平移禁用、捏合围绕
 *   视口中心；再击解除）与关闭按钮，配色全走主题 CSS 变量，预览中切换
 *   主题自动跟随
 * - 状态通知：开关状态变化时派发 CustomEvent('md-lightbox-change')，
 *   Rust 侧经 events.waitForEvent 感知，驱动返回键拦截的注册/注销
 *
 * 坐标模型：托底框（.md-lightbox-frame）按 s=1 的 contain 显示尺寸定宽高、在
 * 浮层内居中，棋盘格与细边框标示图片矩形边界（透明图片在主题 surface 底上
 * 原本与背景融为一体）；transform = translate(tx,ty) scale(s) 作用于托底框，
 * 原点为视口中心。捏合默认焦点跟随（「当前双指中点下的图像点」钉回中点）；
 * 钉住模式下改为视口中心锚定（tx/ty 随倍率等比缩放，中心点下的图像点不动）。
 */

import { tr } from "./i18n";
import { iconSvg } from "./icons";

/** 缩放上限：超宽图形 contain 适配后倍率很小（如 2678px 宽图起始仅 ~0.14），
 * 需要更高上限才能放大到原始字号阅读（与 svg-preview 的手动缩放上限一致） */
const MAX_SCALE = 10;
/** 主动缩放下限：允许缩小到适配尺寸的一半（0.5x）纵览整体 */
const MIN_SCALE = 0.5;
/** 单击判定：按下时长与累计位移上限 */
const TAP_DURATION_MS = 300;
const TAP_SLOP_PX = 10;
/** 松手回弹动画时长（ms） */
const SETTLE_MS = 220;
/** 开关淡入淡出时长（ms），需与 markdown.module.scss 的 transition 保持一致 */
const FADE_MS = 180;
/** 越界拖拽的橡皮筋阻尼：越界位移按此比例跟随手指（0–1，越小越硬） */
const OVERDRAG_RESISTANCE = 0.3;
/** 轴向「仍视为适配」的溢出容差（占该轴视口的比例）：溢出不超过它的轴按
 * 卡片式自由摆放。手动捏合回到 1x 附近时 s 微超 1、贴边轴溢出仅数 px，
 * 按缩放倍率判定（s ≤ 1）会漏掉这种情况，轴余量趋零拖不动 */
const ADAPT_OVERFLOW_TOLERANCE = 0.08;
/** Typst 预览单击开灯箱的延迟：窗口内的下一次触摸视为双击第二击，取消待开 */
const TYPST_OPEN_DELAY_MS = 280;
/** 惯性滚动：松手时速度采样的 EMA 平滑系数（新采样的权重，越大越跟手） */
const FLING_EMA = 0.25;
/** 惯性滚动：速度指数衰减时间常数（ms），手感对标原生 fling */
const FLING_TAU_MS = 400;
/** 惯性滚动：松手静置超过该时长视为停顿，不起惯性（ms） */
const FLING_REST_MS = 120;
/** 惯性滚动：起振速度下限与停止阈值（px/s） */
const FLING_MIN_VELOCITY = 150;
const FLING_STOP_VELOCITY = 80;
/** 工具栏闲置自动隐藏时长：从最后一次显示/交互起算（ms） */
const CHROME_IDLE_MS = 3000;
/** 打开灯箱后工具栏先立即显示，经此时长自动隐藏，进入正常显隐逻辑 */
const OPEN_CHROME_HIDE_DELAY_MS = 1500;

interface Transform {
  s: number;
  tx: number;
  ty: number;
}

interface TouchStartInfo {
  x: number;
  y: number;
  time: number;
  moved: number;
  fingers: number;
}

let overlay: HTMLDivElement | null = null;
/** transform 作用对象：包住预览内容的托底框，按 contain 尺寸居中（见文件头坐标模型） */
let frameEl: HTMLDivElement | null = null;
/** 浮层内的预览元素（img 或克隆的 svg），固有尺寸从这里读，见 containSize */
let viewEl: HTMLElement | SVGElement | null = null;
/** svg 的固有尺寸（viewBox 或宽高属性推导）；图片走 naturalWidth，见 containSize */
let svgIntrinsic: { w: number; h: number } | null = null;
/** 无 viewBox 的 svg 按原始尺寸渲染（非 contain 适配），containSize 需区分口径 */
let svgFixed = false;
let transform: Transform = { s: 1, tx: 0, ty: 0 };
let mode: "idle" | "pan" | "pinch" = "idle";
let panLast = { x: 0, y: 0 };
let startInfo: TouchStartInfo = { x: 0, y: 0, time: 0, moved: 0, fingers: 1 };
let pinchDist0 = 1;
let pinchT0: Transform = { s: 1, tx: 0, ty: 0 };
let settleTimer: ReturnType<typeof setTimeout> | null = null;
/** 单指平移的瞬时速度（px/s），touchmove 间以 EMA 平滑，松手用于惯性滚动 */
let panVelocity = { x: 0, y: 0 };
let lastMoveTime = 0;
/** 惯性滚动动画的 rAF 句柄 */
let inertiaRaf: number | null = null;
/** 底部工具栏（显隐类切换）与闲置隐藏定时器 */
let toolbarEl: HTMLDivElement | null = null;
let chromeTimer: ReturnType<typeof setTimeout> | null = null;
/** 居中并钉住：false = 自由平移 + 围绕双指中点缩放（默认），true = 图像锁定
 * 居中（平移禁用，捏合围绕视口中心）；工具栏按钮二态切换，每次打开重置 */
let pinned = false;

/**
 * 打开灯箱（幂等）。source 为图片地址，或内联 SVG 元素（mermaid 图表等）。
 * invertInDark：暗色主题下是否反色，须与来源预览的正文渲染一致（默认按
 * 「.svg 地址」判断，即 md 正文 img[src$=".svg"] 的规则；Typst 预览传 true）。
 */
export function openLightbox(
  source: string | SVGElement,
  alt = "",
  invertInDark = false,
): void {
  if (overlay) return;

  const el = document.createElement("div");
  el.className = "md-img-lightbox";
  // 主题类挂在 .page 容器上且带 stylance 哈希（如 theme-light-xxxx），浮层挂到
  // body 下拿不到，须复制过来，var(--md-surface) 等变量才能与正文同源解析
  document.querySelector('[class*="theme-"]')?.classList.forEach((cls) => {
    if (cls.includes("theme")) el.classList.add(cls);
  });
  const view = buildView(source, alt, invertInDark);
  const frame = document.createElement("div");
  frame.className = "md-lightbox-frame";
  frame.appendChild(view);
  el.appendChild(frame);
  document.body.appendChild(el);

  overlay = el;
  frameEl = frame;
  viewEl = view;
  pinned = false;
  sizeFrame();
  // 图片加载完成才有固有尺寸；加载前托底框保持收缩，避免空棋盘格铺满屏
  if (view instanceof HTMLImageElement && !view.complete) {
    view.addEventListener("load", sizeFrame, { once: true });
  }
  transform = { s: 1, tx: 0, ty: 0 };
  mode = "idle";
  // 打开即先显示工具栏（短延迟后自动隐藏），此后进入正常的显隐逻辑：
  // 单击浮层切换 + 无操作闲置 CHROME_IDLE_MS 自动隐藏
  toolbarEl = buildToolbar();
  el.appendChild(toolbarEl);
  showChrome(OPEN_CHROME_HIDE_DELAY_MS);
  bindTouch(el);
  window.addEventListener("resize", resetToFit);

  // 下一帧置 opacity 触发淡入（初始 opacity:0 由样式表提供）
  requestAnimationFrame(() => {
    if (overlay === el) el.style.opacity = "1";
  });
  window.dispatchEvent(
    new CustomEvent("md-lightbox-change", { detail: { open: true } }),
  );
}

/**
 * 构造浮层内的预览元素：字符串按图片地址加载；内联 SVG 克隆副本并归一化尺寸。
 * invertInDark：暗色主题下是否反色，须与来源预览的正文渲染一致——
 * md 正文只反 img[src$=".svg"]（默认按此判断）；Typst 预览暗色下整树反色
 * （调用点显式传 true，图片 href 是 data URI 无法按扩展名判断）。
 */
function buildView(
  source: string | SVGElement,
  alt: string,
  invertInDark: boolean,
): HTMLImageElement | SVGSVGElement {
  svgFixed = false;
  const isDark = !!document.querySelector('[class*="theme-dark"]');
  if (typeof source === "string") {
    const image = document.createElement("img");
    image.src = source;
    image.alt = alt;
    // 暗色主题下与正文渲染保持一致（正文对 img[src$=".svg"] 反色）
    if ((invertInDark || image.src.endsWith(".svg")) && isDark) {
      image.style.filter = "invert(1)";
    }
    return image;
  }

  const clone = source.cloneNode(true) as SVGSVGElement;
  // mermaid 的内置 <style> 以自身 id 作作用域（#mermaid-xxx …），移除 id 会让
  // 节点/文字失去填充回退成黑色、在黑底灯箱里不可见，故克隆体保留 id
  // （重复 id 仅为语义瑕疵，CSS 的 id 选择器会同时命中原件与克隆体）
  // mermaid 会写 max-width 限制尺寸，铺满浮层前必须去掉；渲染器还会给正文 svg
  // 写 width:100%/height:auto 内联样式，cloneNode 一并带过来——内联优先级高于类
  // 样式，不剥掉元素就缩成正文里的适配高度、贴在浮层顶部，捏合/平移的「内容
  // 居中于视口」坐标模型整体失准（顶部裁切、底部留白）
  clone.style.removeProperty("max-width");
  clone.style.removeProperty("width");
  clone.style.removeProperty("height");
  clone.classList.add("md-lightbox-svg");
  const vb = clone.viewBox.baseVal;
  if (vb.width > 0 && vb.height > 0) {
    // 有 viewBox：铺满浮层，preserveAspectRatio 默认 meet 即 contain 语义
    clone.setAttribute("width", "100%");
    clone.setAttribute("height", "100%");
    svgIntrinsic = { w: vb.width, h: vb.height };
  } else {
    // 无 viewBox：注入 viewBox 让内容可整体 contain 适配。按「原始尺寸限幅」
    // 渲染时，超出视口的内容在 svg 视口层面被裁掉，放大也永远看不到（替换
    // 元素不随元素盒缩放）；画布口径取宽高属性（与正文渲染一致），缺失时退
    // 回 getBBox 内容边界，两者皆不可得才按原始尺寸限幅
    const wAttr = clone.getAttribute("width") ?? "";
    const hAttr = clone.getAttribute("height") ?? "";
    const w = wAttr.includes("%") ? NaN : Number.parseFloat(wAttr);
    const h = hAttr.includes("%") ? NaN : Number.parseFloat(hAttr);
    let box: { x: number; y: number; w: number; h: number } | null = null;
    if (w > 0 && h > 0) {
      box = { x: 0, y: 0, w, h };
    } else {
      try {
        const b = (source as SVGGraphicsElement).getBBox();
        if (b.width > 0 && b.height > 0) {
          box = { x: b.x, y: b.y, w: b.width, h: b.height };
        }
      } catch {
        /* 未渲染的 svg getBBox 可能抛错，退回限幅路径 */
      }
    }
    if (box) {
      clone.setAttribute("viewBox", `${box.x} ${box.y} ${box.w} ${box.h}`);
      clone.setAttribute("width", "100%");
      clone.setAttribute("height", "100%");
      svgIntrinsic = { w: box.w, h: box.h };
    } else {
      clone.classList.add("md-lightbox-svg--fixed");
      svgFixed = true;
      svgIntrinsic = null;
    }
  }
  return clone;
}

/** 关闭灯箱（幂等）：淡出后移除 DOM。 */
export function closeLightbox(): void {
  if (!overlay) return;
  const el = overlay;
  overlay = null;
  frameEl = null;
  viewEl = null;
  svgIntrinsic = null;
  svgFixed = false;
  mode = "idle";
  pinned = false;
  toolbarEl = null;
  if (chromeTimer) {
    clearTimeout(chromeTimer);
    chromeTimer = null;
  }
  stopSettle();
  stopInertia();
  window.removeEventListener("resize", resetToFit);
  el.style.opacity = "0";
  setTimeout(() => el.remove(), FADE_MS);
  window.dispatchEvent(
    new CustomEvent("md-lightbox-change", { detail: { open: false } }),
  );
}

function bindTouch(el: HTMLDivElement): void {
  // passive:false 才能通过 preventDefault 阻止 WebView 默认手势与底层滚动
  el.addEventListener("touchstart", onTouchStart, { passive: false });
  el.addEventListener("touchmove", onTouchMove, { passive: false });
  el.addEventListener("touchend", onTouchEnd, { passive: false });
  el.addEventListener("touchcancel", onTouchEnd, { passive: false });
}

function onTouchStart(e: TouchEvent): void {
  e.preventDefault();
  stopSettle();
  stopInertia();
  const t0 = e.touches[0];
  if (!t0) return;
  if (e.touches.length === 1) {
    mode = "pan";
    panLast = { x: t0.clientX, y: t0.clientY };
    panVelocity = { x: 0, y: 0 };
    lastMoveTime = e.timeStamp;
    startInfo = {
      x: t0.clientX,
      y: t0.clientY,
      time: Date.now(),
      moved: 0,
      fingers: 1,
    };
  } else if (e.touches.length >= 2) {
    const t1 = e.touches[1]!;
    mode = "pinch";
    // 双指起手即操作图形：工具栏让位，避免遮挡与误触
    hideChrome();
    pinchDist0 = touchDistance(t0, t1);
    pinchT0 = { ...transform };
    startInfo.fingers = 2;
  }
}

function onTouchMove(e: TouchEvent): void {
  e.preventDefault();
  if (mode === "pinch" && e.touches.length >= 2) {
    const t0 = e.touches[0]!;
    const t1 = e.touches[1]!;
    const dist = touchDistance(t0, t1);
    const ns = clamp(pinchT0.s * (dist / pinchDist0), MIN_SCALE, MAX_SCALE);
    if (pinned) {
      // 钉住模式：视口中心下的图像点钉在中心——u = -t0/s0 固定，
      // 解得 t = t0·(ns/s0)，偏移随倍率等比缩放
      const k = ns / pinchT0.s;
      transform = { s: ns, tx: pinchT0.tx * k, ty: pinchT0.ty * k };
    } else {
      // 焦点跟随：中点下的图像点 u 保持在中点 m 之下
      // P = C + t + s·u，u 由当前帧反解，t 由 m、u、新 s 重解
      const m = midpoint(t0, t1);
      const cx = window.innerWidth / 2;
      const cy = window.innerHeight / 2;
      const ux = (m.x - cx - transform.tx) / transform.s;
      const uy = (m.y - cy - transform.ty) / transform.s;
      transform = {
        s: ns,
        tx: m.x - cx - ns * ux,
        ty: m.y - cy - ns * uy,
      };
    }
    // 捏合期间实时夹取：焦点跟随到边界即止，避免松手后大幅回弹吃掉平移余量
    clampOffset();
    apply();
  } else if (mode === "pan" && e.touches.length === 1) {
    const t = e.touches[0]!;
    const dx = t.clientX - panLast.x;
    const dy = t.clientY - panLast.y;
    panLast = { x: t.clientX, y: t.clientY };
    startInfo.moved += Math.abs(dx) + Math.abs(dy);
    // 拖出单击判定阈值即确认是拖拽：工具栏让位（单击才切换显隐，二者不冲突）
    if (startInfo.moved > TAP_SLOP_PX) {
      // 钉住态图像不动，拖拽没有可见反馈，唤出工具栏提示锁定原因
      // （按钮主色高亮即「为什么拖不动、在哪解锁」）；未钉照常让位
      if (pinned) showChrome();
      else hideChrome();
    }
    // 钉住态图像锁定居中：不跟手、不采样速度（松手不起惯性），手势簿记照常
    if (pinned) return;
    // 速度 EMA 采样（px/s）：供松手惯性滚动使用
    const dt = e.timeStamp - lastMoveTime;
    lastMoveTime = e.timeStamp;
    if (dt > 0) {
      panVelocity.x += ((dx / dt) * 1000 - panVelocity.x) * FLING_EMA;
      panVelocity.y += ((dy / dt) * 1000 - panVelocity.y) * FLING_EMA;
    }
    // 任意倍率都可拖拽（关闭已改走工具栏/返回键，无需为单击关闭让出门控）；
    // 缩小态拖拽可把图片挪回视口内任意位置，贴边后橡皮筋阻尼、松手 settle 弹回
    const raw = {
      ...transform,
      tx: transform.tx + dx,
      ty: transform.ty + dy,
    };
    transform = rubberBand(raw);
    apply();
  }
}

function onTouchEnd(e: TouchEvent): void {
  e.preventDefault();
  if (mode === "pinch") {
    const t0 = e.touches[0];
    if (t0) {
      // 双指抬起一指：剩余一指继续平移；捏合过则不再判定为单击
      mode = "pan";
      panLast = { x: t0.clientX, y: t0.clientY };
      startInfo.moved = TAP_SLOP_PX + 1;
    } else {
      mode = "idle";
      settle();
    }
  } else if (mode === "pan" && e.touches.length === 0) {
    mode = "idle";
    const isTap =
      startInfo.fingers === 1 &&
      Date.now() - startInfo.time < TAP_DURATION_MS &&
      startInfo.moved < TAP_SLOP_PX;
    if (isTap) {
      // 单击切换工具栏显隐（照片查看器式）；关闭走工具栏 ✕ 或返回键
      toggleChrome();
    } else if (canFling()) {
      startInertia();
    } else {
      settle();
    }
  }
}

/** 松手时是否具备惯性滚动条件：速度足够、未在橡皮筋越界态、无停顿。 */
function canFling(): boolean {
  const bounds = maxOffset(transform);
  const inBounds =
    Math.abs(transform.tx) <= bounds.x && Math.abs(transform.ty) <= bounds.y;
  if (!inBounds) return false; // 越界态交给 settle 弹回
  if (performance.now() - lastMoveTime > FLING_REST_MS) return false;
  return Math.hypot(panVelocity.x, panVelocity.y) >= FLING_MIN_VELOCITY;
}

/**
 * 惯性滚动：以松手速度继续滑动，速度按指数衰减（时间常数 FLING_TAU_MS），
 * 撞边即该轴贴边停速；速度耗尽或双轴都贴边后 settle 归位。
 */
function startInertia(): void {
  let last = performance.now();
  const step = (now: number): void => {
    inertiaRaf = null;
    const dtMs = Math.max(now - last, 1);
    last = now;
    const dt = dtMs / 1000;
    transform = {
      s: transform.s,
      tx: transform.tx + panVelocity.x * dt,
      ty: transform.ty + panVelocity.y * dt,
    };
    const decay = Math.exp(-dtMs / FLING_TAU_MS);
    panVelocity.x *= decay;
    panVelocity.y *= decay;
    // 撞边界：该轴夹取贴边并清零该轴速度（边界只取决于 s，惯性期间不变）
    const bounds = maxOffset(transform);
    if (transform.tx < -bounds.x || transform.tx > bounds.x) {
      transform.tx = clamp(transform.tx, -bounds.x, bounds.x);
      panVelocity.x = 0;
    }
    if (transform.ty < -bounds.y || transform.ty > bounds.y) {
      transform.ty = clamp(transform.ty, -bounds.y, bounds.y);
      panVelocity.y = 0;
    }
    apply();
    const axisAliveX = panVelocity.x !== 0 && bounds.x > 0;
    const axisAliveY = panVelocity.y !== 0 && bounds.y > 0;
    if (
      Math.hypot(panVelocity.x, panVelocity.y) >= FLING_STOP_VELOCITY &&
      (axisAliveX || axisAliveY)
    ) {
      inertiaRaf = requestAnimationFrame(step);
    } else {
      settle();
    }
  };
  inertiaRaf = requestAnimationFrame(step);
}

/** 取消惯性滚动动画（新触摸/关闭/重置时调用）。 */
function stopInertia(): void {
  if (inertiaRaf !== null) {
    cancelAnimationFrame(inertiaRaf);
    inertiaRaf = null;
  }
}

/** 松手归位：越界平移回弹到边界内（带短过渡动画）；缩放已在捏合中实时夹取。 */
function settle(): void {
  const bounds = maxOffset(transform);
  animateTo({
    s: transform.s,
    tx: clamp(transform.tx, -bounds.x, bounds.x),
    ty: clamp(transform.ty, -bounds.y, bounds.y),
  });
}

/** 偏移归零、保留当前缩放（「居中并钉住」按钮的归中动画），与 settle 同款。 */
function recenter(): void {
  stopInertia();
  stopSettle();
  animateTo({ s: transform.s, tx: 0, ty: 0 });
}

/** 带过渡动画地把 transform 移动到目标态（settle 与 recenter 共用）。 */
function animateTo(target: Transform): void {
  const el = frameEl;
  if (!el) return;
  const unchanged =
    target.s === transform.s &&
    target.tx === transform.tx &&
    target.ty === transform.ty;
  if (unchanged) return;
  el.style.transition = `transform ${SETTLE_MS}ms ease-out`;
  transform = target;
  apply();
  settleTimer = setTimeout(() => {
    settleTimer = null;
    el.style.transition = "";
  }, SETTLE_MS);
}

function stopSettle(): void {
  if (settleTimer) {
    clearTimeout(settleTimer);
    settleTimer = null;
    // 打断动画：直接跳到目标位，避免 transition 残留导致触摸跟手失效
    if (frameEl) frameEl.style.transition = "";
  }
}

/** 窗口尺寸变化（如旋转）：托底框重定尺寸并重置回适配状态。 */
function resetToFit(): void {
  stopSettle();
  stopInertia();
  transform = { s: 1, tx: 0, ty: 0 };
  sizeFrame();
  apply();
}

function apply(): void {
  if (!frameEl) return;
  frameEl.style.transform = `translate(${transform.tx}px, ${transform.ty}px) scale(${transform.s})`;
}

/** 平移夹取：溢出轴贴边、未溢出轴整体留在视口内。 */
function clampOffset(): void {
  const bounds = maxOffset(transform);
  transform = {
    ...transform,
    tx: clamp(transform.tx, -bounds.x, bounds.x),
    ty: clamp(transform.ty, -bounds.y, bounds.y),
  };
}

/** 越界拖拽的橡皮筋：边界内正常跟随，越界位移按阻尼比例衰减。 */
function rubberBand(t: Transform): Transform {
  const b = maxOffset(t);
  const axis = (v: number, max: number): number => {
    if (v > max) return max + (v - max) * OVERDRAG_RESISTANCE;
    if (v < -max) return -max + (v + max) * OVERDRAG_RESISTANCE;
    return v;
  };
  return { s: t.s, tx: axis(t.tx, b.x), ty: axis(t.ty, b.y) };
}

/**
 * 当前缩放下允许的最大平移量，按轴独立分两种语义（判定依据是显示尺寸与
 * 视口的关系，而非缩放倍率——捏合停在 1x 附近时 s 未必恰为 1）：
 * - 溢出轴（显示尺寸超出视口且溢出超过容差）：相册式贴边，取溢出量的一半
 *   （图像边缘不出视口），供平移查看放大后的细节。
 * - 适配轴（放得下，或溢出不超过容差）：卡片式自由摆放，只约束图像中心
 *   不出视口（至少半幅可见），拖到哪停哪不回弹；跑偏后用「居中并钉住」
 *   一键归位。恰为 1x 时贴边轴余量为零、按相册式永远拖不动；手动缩放回
 *   1x 附近时溢出仅数 px、体感同样锁死，两者都由此规则放开。
 */
function maxOffset(t: Transform): { x: number; y: number } {
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const { w, h } = containSize();
  const axisMax = (size: number, vp: number): number => {
    if (size * t.s - vp <= vp * ADAPT_OVERFLOW_TOLERANCE) return vp / 2;
    return (size * t.s - vp) / 2;
  };
  return { x: axisMax(w, vw), y: axisMax(h, vh) };
}

/** 预览对象的固有尺寸（图片 naturalWidth / svg viewBox 或宽高属性）；未知为 null。 */
function intrinsicSize(): { w: number; h: number } | null {
  if (viewEl instanceof HTMLImageElement) {
    return viewEl.naturalWidth > 0 && viewEl.naturalHeight > 0
      ? { w: viewEl.naturalWidth, h: viewEl.naturalHeight }
      : null;
  }
  return svgIntrinsic;
}

/** 托底框按 s=1 的 contain 显示尺寸定宽高（CSS inset:0+margin:auto 居中）。 */
function sizeFrame(): void {
  if (!frameEl) return;
  const intrinsic = intrinsicSize();
  if (!intrinsic) {
    // 尺寸未知（图片未加载/无固有尺寸）：保持收缩，避免空棋盘格铺满屏
    frameEl.style.width = "";
    frameEl.style.height = "";
    return;
  }
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const k = svgFixed
    ? Math.min(1, vw / intrinsic.w, vh / intrinsic.h)
    : Math.min(vw / intrinsic.w, vh / intrinsic.h);
  frameEl.style.width = `${intrinsic.w * k}px`;
  frameEl.style.height = `${intrinsic.h * k}px`;
}

/** s=1 时预览内容经 contain 语义呈现的显示尺寸（图片 naturalWidth，svg 固有尺寸）。 */
function containSize(): { w: number; h: number } {
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const intrinsic = intrinsicSize();
  if (!intrinsic) return { w: vw, h: vh };
  // 无 viewBox 的 svg 以原始尺寸显示（CSS max-width/max-height 等比限幅），
  // 不走 contain 适配口径，否则平移边界与实际绘制尺寸不符
  const k = svgFixed
    ? Math.min(1, vw / intrinsic.w, vh / intrinsic.h)
    : Math.min(vw / intrinsic.w, vh / intrinsic.h);
  return { w: intrinsic.w * k, h: intrinsic.h * k };
}

function touchDistance(a: Touch, b: Touch): number {
  const dx = a.clientX - b.clientX;
  const dy = a.clientY - b.clientY;
  return Math.sqrt(dx * dx + dy * dy);
}

function midpoint(a: Touch, b: Touch): { x: number; y: number } {
  return { x: (a.clientX + b.clientX) / 2, y: (a.clientY + b.clientY) / 2 };
}

function clamp(v: number, min: number, max: number): number {
  return Math.min(Math.max(v, min), max);
}

// ===== 底部工具栏 =====

/** 隐藏态类名（样式见 markdown.module.scss 的 .md-lightbox-toolbar） */
const TOOLBAR_HIDDEN_CLS = "md-lightbox-toolbar--hidden";

/**
 * 构建底部工具栏：适应宽度、1:1 原始尺寸、居中并钉住（二态切换）、关闭。
 * 图标走 icons.ts（真源在 ui 包 icons 模块）：FitWidth = 双向箭头顶着左右
 * 边界线（「沿宽度撑到视口边」），CenterFocus = 四角括号 + 中央圆点。
 * 触摸事件全部 stopPropagation——浮层手势绑在根元素上，不拦截的话点按钮会
 * 被当成一次「pan」手势，touchend 的单击判定还会把工具栏再切走；不
 * preventDefault，按钮的 click 才能正常触发。
 */
function buildToolbar(): HTMLDivElement {
  const bar = document.createElement("div");
  bar.className = "md-lightbox-toolbar";
  bar.setAttribute("role", "toolbar");
  bar.setAttribute("aria-label", tr("lightbox-toolbar"));
  bar.innerHTML =
    `<button class="md-lightbox-btn" data-action="fit-width" title="${tr("lightbox-fit-width")}" aria-label="${tr("lightbox-fit-width")}">${iconSvg("FitWidth")}</button>` +
    `<button class="md-lightbox-btn md-lightbox-btn--text" data-action="zoom-100" title="${tr("lightbox-zoom-100")}" aria-label="${tr("lightbox-zoom-100")}">1:1</button>` +
    `<button class="md-lightbox-btn" data-action="center-pin" title="${tr("lightbox-center-pin")}" aria-pressed="false">${iconSvg("CenterFocus")}</button>` +
    '<span class="md-lightbox-toolbar-divider"></span>' +
    `<button class="md-lightbox-btn" data-action="close" title="${tr("lightbox-close")}" aria-label="${tr("lightbox-close")}">${iconSvg("Close")}</button>`;
  bar.classList.add(TOOLBAR_HIDDEN_CLS);
  for (const type of ["touchstart", "touchmove", "touchend", "touchcancel"]) {
    bar.addEventListener(type, (e) => e.stopPropagation());
  }
  bar.addEventListener("click", (e: Event) => {
    const btn = (e.target as HTMLElement).closest(".md-lightbox-btn");
    if (!btn) return;
    const action = (btn as HTMLElement).dataset.action;
    if (action === "fit-width") {
      zoomToFitWidth();
    } else if (action === "zoom-100") {
      zoomToOriginalSize();
    } else if (action === "center-pin") {
      pinned = !pinned;
      if (pinned) {
        // 居中并钉住：先动画归中，此后平移锁定、捏合围绕视口中心
        recenter();
      }
      // 解除钉住时无需移动：钉住期间图像始终居中
      btn.classList.toggle("is-active", pinned);
      btn.setAttribute("aria-pressed", String(pinned));
      btn.setAttribute(
        "title",
        pinned ? tr("lightbox-unpin") : tr("lightbox-center-pin"),
      );
    } else if (action === "close") {
      closeLightbox();
      return;
    }
    restartChromeIdle(); // 关闭外的按钮操作后工具栏保留，闲置计时重新起算
  });
  return bar;
}

/**
 * 「适应宽度」：恢复打开灯箱时的初始缩放。s=1 就是打开时的 contain 适配
 * 规则本身（贴视口、无额外留白），不重新计算适配；围绕视口中心缩放，
 * 平移位置保留，已在初始态时点击无变化。
 */
function zoomToFitWidth(): void {
  zoomToScale(1);
}

/**
 * 「1:1」：内容以原始像素 100% 显示。s=1 的基准是 contain 适配显示而非
 * 原始尺寸（见文件头坐标模型），故目标倍率 = 固有宽 / contain 显示宽。
 */
function zoomToOriginalSize(): void {
  const intrinsic = intrinsicSize();
  if (!intrinsic) return;
  const { w } = containSize();
  if (!(w > 0)) return;
  zoomToScale(clamp(intrinsic.w / w, MIN_SCALE, MAX_SCALE));
}

/**
 * 缩放到目标倍率（工具栏按钮触发）：围绕视口中心——中心下的图像点保持
 * 不动（与钉住捏合同一锚点，先定位再点按钮中心内容不跳），平移夹取到
 * 合法范围后带过渡动画到位。
 */
function zoomToScale(targetS: number): void {
  stopInertia();
  stopSettle();
  const k = targetS / transform.s;
  const bounds = maxOffset({ s: targetS, tx: 0, ty: 0 });
  animateTo({
    s: targetS,
    tx: clamp(transform.tx * k, -bounds.x, bounds.x),
    ty: clamp(transform.ty * k, -bounds.y, bounds.y),
  });
}

function chromeVisible(): boolean {
  return !!toolbarEl && !toolbarEl.classList.contains(TOOLBAR_HIDDEN_CLS);
}

/** 唤出工具栏并重置闲置隐藏计时（打开时的首次显示用短延迟，见 openLightbox）。 */
function showChrome(idleDelay: number = CHROME_IDLE_MS): void {
  if (!toolbarEl || chromeVisible()) return;
  toolbarEl.classList.remove(TOOLBAR_HIDDEN_CLS);
  restartChromeIdle(idleDelay);
}

/** 收起工具栏（幂等）。 */
function hideChrome(): void {
  if (!toolbarEl || !chromeVisible()) return;
  toolbarEl.classList.add(TOOLBAR_HIDDEN_CLS);
  if (chromeTimer) {
    clearTimeout(chromeTimer);
    chromeTimer = null;
  }
}

function toggleChrome(): void {
  if (chromeVisible()) {
    hideChrome();
  } else {
    showChrome();
  }
}

/** 闲置后自动隐藏：只在空闲时触发，操作中已先被手势让位。 */
function restartChromeIdle(idleDelay: number = CHROME_IDLE_MS): void {
  if (chromeTimer) clearTimeout(chromeTimer);
  chromeTimer = setTimeout(() => {
    chromeTimer = null;
    hideChrome();
  }, idleDelay);
}

// ===== Typst 预览的灯箱入口 =====

let previewEntryBound = false;
let tapCandidate: { x: number; y: number; time: number } | null = null;
let previewOpenTimer: ReturnType<typeof setTimeout> | null = null;
/** 待开定时器被下一次触摸取消时置位：该手势是双击的第二击，其 tap 不再调度 */
let suppressTap = false;

/**
 * Typst 预览灯箱入口：单击页面图形全屏预览（模块加载时调用一次）。
 *
 * - 灯箱只属于真正的图片元素：命中点经 elementsFromPoint 穿透文字层找
 *   <image>（image() 嵌入的位图/矢量图）；文字/表格/形状/空白处不触发
 * - 预览的双击已被「切换缩放」占用：单击延迟 TYPST_OPEN_DELAY_MS 打开，窗口内
 *   任何新触摸（双击的第二击/捏合起手）都取消待开定时器
 * - 与 markdown 入口同口径：按钮/链接/输入类元素上的点击不触发（图标等
 *   非图片元素不进灯箱）
 * - 全程被动监听、不 preventDefault：原生滚动、文字选择与 dblclick 合成
 *   不受干扰；灯箱开着时直接跳过（浮层自己的手势接管）
 */
export function initTypstPreviewLightbox(): void {
  if (previewEntryBound) return;
  previewEntryBound = true;
  document.addEventListener(
    "touchstart",
    (e: TouchEvent) => {
      if (overlay) return;
      if (previewOpenTimer) {
        clearTimeout(previewOpenTimer);
        previewOpenTimer = null;
        suppressTap = true;
      } else {
        suppressTap = false;
      }
      if (e.touches.length !== 1) {
        tapCandidate = null;
        return;
      }
      const target = e.target instanceof Element ? e.target : null;
      if (
        target?.closest("a, button, input, textarea, [contenteditable='true']")
      ) {
        tapCandidate = null;
        return;
      }
      const t = e.touches[0]!;
      tapCandidate = { x: t.clientX, y: t.clientY, time: Date.now() };
    },
    { passive: true },
  );
  document.addEventListener(
    "touchmove",
    (e: TouchEvent) => {
      const cand = tapCandidate;
      if (overlay || !cand) return;
      const t = e.touches[0];
      if (!t) {
        tapCandidate = null;
        return;
      }
      // 挪出单击判定阈值即视为滚动/拖拽，不再是 tap
      if (Math.hypot(t.clientX - cand.x, t.clientY - cand.y) >= TAP_SLOP_PX) {
        tapCandidate = null;
      }
    },
    { passive: true },
  );
  document.addEventListener(
    "touchend",
    (e: TouchEvent) => {
      const cand = tapCandidate;
      if (overlay || !cand || e.touches.length !== 0) return;
      const t = e.changedTouches[0]!;
      const isTap =
        Date.now() - cand.time < TAP_DURATION_MS &&
        Math.hypot(t.clientX - cand.x, t.clientY - cand.y) < TAP_SLOP_PX;
      tapCandidate = null;
      const suppressed = suppressTap;
      suppressTap = false;
      if (!isTap || suppressed) return;
      previewOpenTimer = setTimeout(() => {
        previewOpenTimer = null;
        openTypstImageLightbox(t.clientX, t.clientY);
      }, TYPST_OPEN_DELAY_MS);
    },
    { passive: true },
  );
  document.addEventListener(
    "touchcancel",
    () => {
      tapCandidate = null;
      if (previewOpenTimer) {
        clearTimeout(previewOpenTimer);
        previewOpenTimer = null;
      }
    },
    { passive: true },
  );
}

/**
 * 开点击命中的 Typst 预览图片的灯箱；点在文字/表格/形状/空白处为空操作。
 *
 * 文字层常驻且可选中，点击目标多为透明 span——用 elementsFromPoint 穿透它取
 * 完整命中链找 <image>。单页文档的 svg 直接挂在 typst_output 下（无
 * .typst-page 包裹，多页才有），故以 typst_output 容器为作用域。
 */
function openTypstImageLightbox(x: number, y: number): void {
  for (const el of document.elementsFromPoint(x, y)) {
    if (!(el instanceof SVGImageElement)) continue;
    if (!el.closest('[class*="typst_output"]')) continue;
    const href = el.getAttribute("href") ?? el.getAttribute("xlink:href");
    // Typst 预览暗色下 .typst_output svg 整树反色（位图矢量都反），灯箱须跟随
    if (href) openLightbox(href, "", true);
    return;
  }
}
