// PDF 预览渲染逻辑（按需渲染/虚拟化）。
//
// 由原 `pdf_preview/js/inject_pdfjs_libraries.js` 的 `window.__pdfPreview` 闭包迁移而来，
// 改为 module scope 状态 + npm 包直接 import（类型安全，主库随 bundle 注入）。
// worker 与 cmaps 路径因带 asset! hash 后缀，由 Rust 侧 `configure` 传入。
//
// 数据流：
// 1. Rust `configure(workerUrl, cmapsUrl)` 设置 workerSrc 与 cMapUrl
// 2. Rust `render.load(b64, cid, scale)` → getDocument → 取首页尺寸 + 内容矩形后
//    **立即返回**（不等渲染），Rust 秒级算出 fit scale；其余占位页后台补齐
// 3. 滚动/缩放事件驱动 `virtualRenderPass`：只渲染视口 ± 一屏缓冲的页，
//    离开缓冲区的页回收 canvas/textLayer 并 `page.cleanup()`，内存有界
// 4. 缩放变化 `render.setScale` → 回收全部 canvas → 重排占位尺寸 → 重渲可视页
//
// 为什么必须虚拟化：全量渲染会为每页建常驻 canvas（DPR 3 下约 20MB/页），
// 数百页即耗尽 WebView 内存；且渲染完才返回会让 Rust 的尺寸轮询饿死，
// 容器塌缩成 1px 宽（大文档必现空白）。

import * as pdfjsLib from "pdfjs-dist";
import { injectPdfPageNumbers, hidePageNumbers } from "./page-indicator";

// pdfjs 6.x 依赖的 ES2025 原型方法 polyfill（Android WebView 可能缺失）
function ensurePolyfills(): void {
  const u8 = Uint8Array.prototype as unknown as { toHex?: () => string };
  if (!u8.toHex) {
    u8.toHex = function toHex(this: Uint8Array): string {
      const hex: string[] = [];
      for (let i = 0; i < this.length; i++) {
        hex.push((this[i]! >>> 0).toString(16).padStart(2, "0"));
      }
      return hex.join("");
    };
  }
  const mapProto = Map.prototype as unknown as {
    getOrInsertComputed?: <K, V>(key: K, fn: () => V) => V;
    getOrInsert?: <K, V>(key: K, val: V) => V;
  };
  if (!mapProto.getOrInsertComputed) {
    mapProto.getOrInsertComputed = function getOrInsertComputed<K, V>(
      this: Map<K, V>,
      key: K,
      fn: () => V,
    ): V {
      if (this.has(key)) return this.get(key) as V;
      const val = fn();
      this.set(key, val);
      return val;
    };
  }
  if (!mapProto.getOrInsert) {
    mapProto.getOrInsert = function getOrInsert<K, V>(
      this: Map<K, V>,
      key: K,
      val: V,
    ): V {
      if (this.has(key)) return this.get(key) as V;
      this.set(key, val);
      return val;
    };
  }
}

// textLayer 样式（pdfjs 手动创建 textLayer div 时需要，原 js 内联）
const TEXTLAYER_CSS =
  ".textLayer{position:absolute;left:0;top:0;right:0;bottom:0;overflow:clip;line-height:1;-webkit-text-size-adjust:none;-moz-text-size-adjust:none;text-size-adjust:none;forced-color-adjust:none;transform-origin:0 0;z-index:2}.textLayer :is(span,br){color:transparent;position:absolute;white-space:pre;cursor:text;transform-origin:0% 0%}.textLayer span{font-size:calc(var(--font-height)*var(--total-scale-factor,1))}.textLayer span[dir=ltr]{transform:scaleX(var(--scale-x,1))}.textLayer span[dir=rtl]{transform:scaleX(var(--scale-x,1)) translateX(-100%)}.textLayer span.markedContent{top:0;height:0}.textLayer .highlight{margin:-1px;padding:1px;background-color:rgba(180,0,170,0.2);border-radius:4px}.textLayer .highlight.appended{position:initial}.textLayer .highlight.begin{border-radius:4px 0 0 4px}.textLayer .highlight.end{border-radius:0 4px 4px 0}.textLayer .highlight.middle{border-radius:0}.textLayer .highlight.selected{background-color:rgba(0,100,0,0.2)}.textLayer ::selection{background:rgba(0,0,255,0.25);mix-blend-mode:multiply}.textLayer br::selection{background:transparent}.textLayer .endOfContent{display:block;position:absolute;inset:100% 0 0;z-index:-1;cursor:default;user-select:none}.textLayer .endOfContent.active{top:0}";

function ensureTextLayerCss(): void {
  if (document.getElementById("pdfjs-textlayer-css")) return;
  const style = document.createElement("style");
  style.id = "pdfjs-textlayer-css";
  style.textContent = TEXTLAYER_CSS;
  document.head.appendChild(style);
}

interface ContentRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

interface PageInfo {
  width: number;
  height: number;
}

type PdfRenderTask = ReturnType<pdfjsLib.PDFPageProxy["render"]>;

/** 一页的活跃渲染产物：canvas/textLayer/渲染任务/页代理。 */
interface RenderedPage {
  canvas: HTMLCanvasElement;
  task: PdfRenderTask;
  textLayerDiv: HTMLElement;
  page: pdfjsLib.PDFPageProxy;
}

// module scope 状态（替代原 window.__pdfPreview 闭包）
let pdfDoc: pdfjsLib.PDFDocumentProxy | null = null;
let container: HTMLElement | null = null;
let currentScale = 1.0;
let pageInfos: PageInfo[] = [];
/** pageTops[i] = 第 i 页顶部在 container 内的 y（含页间距），随页数增长/缩放重建 */
let pageTops: number[] = [];
let loadId = 0;
/** 渲染纪元：换文档/换缩放时 +1，使在途渲染任务自行作废 */
let renderEpoch = 0;
let renderRaf = 0;
let cachedContentRect: ContentRect | null = null;
let cmapsUrl = "";
/** 滚动容器（container 的父级 .preview div），滚动事件驱动按需渲染 */
let scroller: HTMLElement | null = null;
/** 已渲染页：下标 → 产物 */
const renderedPages = new Map<number, RenderedPage>();
/** 渲染进行中的页下标 */
const inFlightPages = new Set<number>();

/** 页间距（px）：与占位页 div 的 marginBottom 保持一致 */
const PAGE_GAP = 8;

// Worker 就绪 Promise：configure 异步创建 blob worker，loadPdf 等待它完成。
// Android WebView 中 pdfjs 内部 `new Worker(url, {type:"module"})` 对 asset URL
// 静默失败，必须像旧 inject_pdfjs_libraries.js 那样手动 fetch worker 源码、
// patch import.meta.url、改写 ESM export 为 globalThis 赋值、包成 Blob 再创建
// classic Worker，赋给 GlobalWorkerOptions.workerPort。
let workerReady: Promise<void> = Promise.resolve();

function decodeBase64(b64: string): Uint8Array {
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) {
    bytes[i] = bin.charCodeAt(i);
  }
  return bytes;
}

// Android WebView 的 canvas 面积上限约 16.7M 像素（4096²），超出后绘制静默
// 失败整块空白（DPR 3 下大页极易撞上）；钳制内部分辨率，牺牲清晰度换可见性。
const MAX_CANVAS_PIXELS = 16_000_000;

function isCancelError(e: unknown): boolean {
  const name = (e as { name?: string } | null)?.name;
  return name === "RenderingCancelledException" || name === "AbortException";
}

/** 回收一页：取消在途任务、释放 canvas 位图与页面解码资源。页码标签保留。 */
function recyclePage(i: number): void {
  const entry = renderedPages.get(i);
  if (!entry) return;
  renderedPages.delete(i);
  entry.task.cancel();
  entry.canvas.width = 0;
  entry.canvas.height = 0;
  entry.canvas.remove();
  entry.textLayerDiv.remove();
  void entry.page.cleanup();
}

/** 计算当前需保持渲染的页范围：视口 ± 一屏缓冲。 */
function computeVisibleRange(): [number, number] {
  const s = scroller;
  if (!s || !container || pageTops.length === 0) return [0, -1];
  const viewH = s.clientHeight || 1;
  const top = s.scrollTop - container.offsetTop;
  const yMin = top - viewH;
  const yMax = top + viewH * 2;
  // 页底单调递增：二分找第一个「页底 > yMin」的页
  let lo = 0;
  let hi = pageTops.length - 1;
  while (lo < hi) {
    const m = (lo + hi) >> 1;
    const bottom =
      pageTops[m]! + (pageInfos[m]?.height ?? 0) * currentScale + PAGE_GAP;
    if (bottom <= yMin) lo = m + 1;
    else hi = m;
  }
  const first = lo;
  // 页顶单调递增：二分找最后一个「页顶 < yMax」的页
  let a = first;
  let b = pageTops.length - 1;
  let last = first;
  while (a <= b) {
    const m = (a + b) >> 1;
    if (pageTops[m]! < yMax) {
      last = m;
      a = m + 1;
    } else {
      b = m - 1;
    }
  }
  return [first, last];
}

/** 渲染单页：canvas + 文字层。纪元变化/文档切换时静默放弃。 */
async function renderPage(i: number, epoch: number): Promise<void> {
  if (!pdfDoc || !container) return;
  const info = pageInfos[i];
  const pageDiv = container.children[i] as HTMLElement | undefined;
  if (!info || !pageDiv) return;
  inFlightPages.add(i);
  try {
    const page = await pdfDoc.getPage(i + 1);
    if (epoch !== renderEpoch || !container) return;
    const dpr = window.devicePixelRatio || 1;
    const renderScale = currentScale * dpr;
    let viewport = page.getViewport({ scale: renderScale });
    const pixels = viewport.width * viewport.height;
    if (pixels > MAX_CANVAS_PIXELS) {
      viewport = page.getViewport({
        scale: renderScale * Math.sqrt(MAX_CANVAS_PIXELS / pixels),
      });
    }
    const canvas = document.createElement("canvas");
    canvas.width = Math.floor(viewport.width);
    canvas.height = Math.floor(viewport.height);
    canvas.style.display = "block";
    canvas.style.width = `${info.width * currentScale}px`;
    canvas.style.height = `${info.height * currentScale}px`;
    const textLayerDiv = document.createElement("div");
    textLayerDiv.className = "textLayer";
    textLayerDiv.style.setProperty(
      "--total-scale-factor",
      String(currentScale),
    );
    pageDiv.insertBefore(canvas, pageDiv.firstChild);
    pageDiv.appendChild(textLayerDiv);
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const task = page.render({ canvas, canvasContext: ctx, viewport });
    const entry: RenderedPage = { canvas, task, textLayerDiv, page };
    renderedPages.set(i, entry);
    try {
      await task.promise;
      if (epoch !== renderEpoch) return;
      const textContent = await page.getTextContent();
      if (epoch !== renderEpoch) return;
      const textLayer = new pdfjsLib.TextLayer({
        textContentSource: textContent,
        container: textLayerDiv,
        viewport: page.getViewport({ scale: currentScale }),
      });
      await textLayer.render();
    } catch (e) {
      if (renderedPages.get(i) === entry) {
        // 真渲染失败（非滚走取消）：回收后下次进入视口自动重试
        recyclePage(i);
        if (!isCancelError(e)) {
          console.warn(`pdf: 第 ${i + 1} 页渲染失败`, e);
        }
      }
    }
  } finally {
    inFlightPages.delete(i);
  }
}

/** 按需渲染一遍：回收缓冲区外的页，从视口中心向外补齐缺页。 */
async function virtualRenderPass(): Promise<void> {
  if (!pdfDoc || !container) return;
  const epoch = renderEpoch;
  const [lo, hi] = computeVisibleRange();
  // Map 迭代期间删除元素是安全的（recyclePage 内部会 delete）
  for (const i of renderedPages.keys()) {
    if (i < lo || i > hi) recyclePage(i);
  }
  const s = scroller;
  const center = s ? s.scrollTop - container.offsetTop + s.clientHeight / 2 : 0;
  const missing: number[] = [];
  for (let i = lo; i <= hi; i++) {
    if (!renderedPages.has(i) && !inFlightPages.has(i)) missing.push(i);
  }
  missing.sort(
    (a, b) =>
      Math.abs((pageTops[a] ?? 0) - center) -
      Math.abs((pageTops[b] ?? 0) - center),
  );
  for (const i of missing) {
    if (epoch !== renderEpoch || !pdfDoc || !container) return;
    if (renderedPages.has(i) || inFlightPages.has(i)) continue;
    await renderPage(i, epoch);
  }
}

function scheduleVirtualRender(): void {
  if (renderRaf !== 0) return;
  renderRaf = requestAnimationFrame(() => {
    renderRaf = 0;
    void virtualRenderPass();
  });
}

function onScrollerScroll(): void {
  scheduleVirtualRender();
}

function attachScrollListener(): void {
  detachScrollListener();
  const s = container?.parentElement;
  if (!s) return;
  scroller = s;
  s.addEventListener("scroll", onScrollerScroll, { passive: true });
}

function detachScrollListener(): void {
  if (!scroller) return;
  scroller.removeEventListener("scroll", onScrollerScroll);
  scroller = null;
}

function makePageDiv(index1based: number, info: PageInfo): HTMLElement {
  const pageDiv = document.createElement("div");
  pageDiv.dataset.page = String(index1based);
  pageDiv.style.display = "block";
  pageDiv.style.position = "relative";
  pageDiv.style.marginBottom = "8px";
  pageDiv.style.width = `${info.width * currentScale}px`;
  pageDiv.style.height = `${info.height * currentScale}px`;
  return pageDiv;
}

/** 追加一页的 pageTop（与 pageInfos 同下标增长）。 */
function appendPageTop(): void {
  if (pageTops.length === 0) {
    pageTops.push(0);
    return;
  }
  const last = pageTops.length - 1;
  pageTops.push(
    pageTops[last]! + pageInfos[last]!.height * currentScale + PAGE_GAP,
  );
}

/** 缩放后全量重算 pageTop 前缀和。 */
function rebuildPageTops(): void {
  const tops: number[] = Array.from({ length: pageInfos.length }, () => 0);
  let y = 0;
  for (let i = 0; i < pageInfos.length; i++) {
    tops[i] = y;
    y += pageInfos[i]!.height * currentScale + PAGE_GAP;
  }
  pageTops = tops;
}

/** 后台补齐剩余占位页（真实纸张尺寸撑滚动条），补齐后补挂页码标签。 */
async function buildPlaceholders(
  doc: pdfjsLib.PDFDocumentProxy,
  myId: number,
): Promise<void> {
  for (let i = pageInfos.length; i < doc.numPages; i++) {
    const page = await doc.getPage(i + 1);
    if (myId !== loadId || !container) return;
    const v = page.getViewport({ scale: 1.0 });
    const info = { width: v.width, height: v.height };
    pageInfos.push(info);
    appendPageTop();
    container.appendChild(makePageDiv(i + 1, info));
  }
  if (myId !== loadId || !container) return;
  injectPdfPageNumbers(container.id);
  hidePageNumbers();
  scheduleVirtualRender();
}

/** 配置 worker 与 cmaps 路径（Rust 侧 asset! hash URL 传入）。
 *
 * Android WebView 中 pdfjs 内部 `new Worker(url, {type:"module"})` 对 asset URL
 * 静默失败（跨域 + import.meta.url 解析为无效路径）。必须手动创建 classic Worker：
 * fetch worker 源码 → patch import.meta.url → 改写 ESM export → 包成 Blob → new Worker。
 * 异步执行，结果存入 workerReady，loadPdf 会 await 它。
 */
export function configure(workerUrl: string, cmaps: string): void {
  ensurePolyfills();
  ensureTextLayerCss();
  cmapsUrl = cmaps;
  workerReady = (async () => {
    const fullUrl = location.origin + workerUrl;
    const workerResponse = await fetch(workerUrl);
    let workerSrc = await workerResponse.text();
    // patch import.meta.url → 绝对 URL（worker 内部用它在 blob 中定位自身）
    workerSrc = workerSrc.replace(/import\.meta\.url/g, `'${fullUrl}'`);
    // 改写 ESM export 为 globalThis 赋值，使 classic Worker 能执行
    workerSrc = workerSrc.replace(/export\{([^}]+)\};?$/, "globalThis.$1=$1;");
    // polyfill（与主线程一致，worker 也需要 ES2025 原型方法）
    const polyfillCode =
      'if(!Uint8Array.prototype.toHex){Uint8Array.prototype.toHex=function(){var h=[];for(var i=0;i<this.length;i++)h.push((this[i]>>>0).toString(16).padStart(2,"0"));return h.join("")}};if(!Map.prototype.getOrInsertComputed){Map.prototype.getOrInsertComputed=function(k,fn){if(this.has(k))return this.get(k);var v=fn();this.set(k,v);return v}};if(!Map.prototype.getOrInsert){Map.prototype.getOrInsert=function(k,v){if(this.has(k))return this.get(k);this.set(k,v);return v}};';
    const combined = polyfillCode + workerSrc;
    const workerBlob = new Blob([combined], { type: "text/javascript" });
    const blobUrl = URL.createObjectURL(workerBlob);
    const pdfWorker = new Worker(blobUrl);
    pdfjsLib.GlobalWorkerOptions.workerPort = pdfWorker;
  })();
}

/** 加载 PDF（base64）：解析 + 首页尺寸/内容矩形就绪后立即返回，
 * 其余占位页与页面渲染全部按需/后台进行。
 */
export async function loadPdf(
  b64: string,
  cid: string,
  scale: number,
): Promise<string> {
  const myId = ++loadId;
  // 等待 worker blob bootstrap 完成（configure 异步创建 worker）
  await workerReady;
  const el = document.getElementById(cid);
  if (!el) return JSON.stringify(null);
  container = el;
  currentScale = scale;
  renderEpoch++;
  renderedPages.clear();
  inFlightPages.clear();
  container.innerHTML = "";
  pageInfos = [];
  pageTops = [];
  cachedContentRect = null;
  const bytes = decodeBase64(b64);
  const loadingTask = pdfjsLib.getDocument({
    data: bytes,
    cMapUrl: cmapsUrl,
    cMapPacked: true,
  });
  const doc = await loadingTask.promise;
  if (myId !== loadId) {
    void (doc as unknown as { destroy: () => Promise<void> }).destroy();
    return JSON.stringify(null);
  }
  pdfDoc = doc;
  // 首页尺寸 + 内容矩形就绪即返回，Rust 秒级拿到 fit scale；
  // 大文档的剩余占位页后台补齐，不阻塞加载门控
  const first = await doc.getPage(1);
  const v1 = first.getViewport({ scale: 1.0 });
  const info1 = { width: v1.width, height: v1.height };
  pageInfos.push(info1);
  appendPageTop();
  container.appendChild(makePageDiv(1, info1));
  cachedContentRect = await computeContentRect();
  if (myId !== loadId) return JSON.stringify(null);
  attachScrollListener();
  scheduleVirtualRender();
  void buildPlaceholders(doc, myId);
  const result = {
    numPages: doc.numPages,
    pageInfos,
    contentRect: cachedContentRect,
  };
  window.dispatchEvent(new CustomEvent("pdf-loaded", { detail: result }));
  return JSON.stringify(result);
}

/** 设置缩放并按需重渲染（回收全部旧 canvas，重排占位尺寸）。 */
export function setPdfScale(scale: number): void {
  if (!pdfDoc) return;
  currentScale = scale;
  renderEpoch++;
  for (const i of renderedPages.keys()) recyclePage(i);
  for (let i = 0; i < pageInfos.length; i++) {
    const div = container?.children[i] as HTMLElement | undefined;
    const info = pageInfos[i];
    if (!div || !info) continue;
    div.style.width = `${info.width * currentScale}px`;
    div.style.height = `${info.height * currentScale}px`;
  }
  rebuildPageTops();
  scheduleVirtualRender();
}

/** 查询首页尺寸 + 页数 + 内容区域（供 Rust 计算 fit scale）。
 *
 * 扫描件等无文本层的 PDF 没有 contentRect，cr 传 null——Rust 侧按整页
 * 兜底（fit 用整页宽、吸附/居中回退整页），不能因此拒绝返回页面尺寸，
 * 否则 Rust 轮询超时、page_dims 停留在 1.0、容器塌缩成 1px 宽。
 */
export function getPdfPageInfos(): string {
  if (pageInfos.length > 0) {
    return JSON.stringify({
      w: pageInfos[0]!.width,
      h: pageInfos[0]!.height,
      n: pageInfos.length,
      cr: cachedContentRect,
    });
  }
  return JSON.stringify(null);
}

/** 查询内容区域。 */
export function getPdfContentRect(): string {
  return JSON.stringify(cachedContentRect);
}

/** 清理 PDF 资源。 */
export function cleanupPdfPreview(): void {
  renderEpoch++;
  loadId++;
  detachScrollListener();
  if (renderRaf !== 0) {
    cancelAnimationFrame(renderRaf);
    renderRaf = 0;
  }
  if (pdfDoc) {
    void (pdfDoc as unknown as { destroy: () => Promise<void> }).destroy();
  }
  pdfDoc = null;
  renderedPages.clear();
  inFlightPages.clear();
  if (container) {
    container.innerHTML = "";
  }
  container = null;
  pageInfos = [];
  pageTops = [];
  cachedContentRect = null;
}

async function computeContentRect(): Promise<ContentRect | null> {
  if (!pdfDoc) return null;
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  for (let i = 1; i <= Math.min(pdfDoc.numPages, 3); i++) {
    try {
      const page = await pdfDoc.getPage(i);
      const tc = await page.getTextContent();
      const vp = page.getViewport({ scale: 1.0 });
      for (const item of tc.items) {
        const t = item as unknown as {
          transform: number[];
          width: number;
          height: number;
        };
        if (!t.transform || t.transform.length < 6) continue;
        const tx = t.transform[4]!;
        const ty = vp.height - t.transform[5]!;
        const tw = t.width;
        const th = t.height;
        if (tw < 1 || th < 1) continue;
        if (tx < minX) minX = tx;
        if (ty < minY) minY = ty;
        if (tx + tw > maxX) maxX = tx + tw;
        if (ty + th > maxY) maxY = ty + th;
      }
    } catch {
      /* ignore single page error */
    }
  }
  if (minX >= Infinity) return null;
  return { left: minX, top: minY, width: maxX - minX, height: maxY - minY };
}
