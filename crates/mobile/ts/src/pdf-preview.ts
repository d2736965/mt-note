// PDF 预览渲染逻辑。
//
// 由原 `pdf_preview/js/inject_pdfjs_libraries.js` 的 `window.__pdfPreview` 闭包迁移而来，
// 改为 module scope 状态 + npm 包直接 import（类型安全，主库随 bundle 注入）。
// worker 与 cmaps 路径因带 asset! hash 后缀，由 Rust 侧 `configure` 传入。
//
// 数据流：
// 1. Rust `configure(workerUrl, cmapsUrl)` 设置 workerSrc 与 cMapUrl
// 2. Rust `render.load(b64, cid, scale)` → getDocument + 逐页 canvas + textLayer
// 3. Rust 轮询 `query.pageInfos()` 取页面尺寸后计算 fit scale
// 4. 缩放变化 `render.setScale` 触发重渲染

import * as pdfjsLib from "pdfjs-dist";

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

// module scope 状态（替代原 window.__pdfPreview 闭包）
let pdfDoc: pdfjsLib.PDFDocumentProxy | null = null;
let container: HTMLElement | null = null;
let currentScale = 1.0;
let pageInfos: PageInfo[] = [];
let loadId = 0;
let renderId = 0;
let cachedContentRect: ContentRect | null = null;
let cmapsUrl = "";

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

async function renderAllPages(): Promise<void> {
  if (!pdfDoc || !container) return;
  const myRenderId = ++renderId;
  const dpr = window.devicePixelRatio || 1;
  const renderScale = currentScale * dpr;
  for (let i = 0; i < pageInfos.length; i++) {
    if (myRenderId !== renderId) return;
    const pageDiv = container.children[i] as HTMLElement | undefined;
    if (!pageDiv) continue;
    const canvas = pageDiv.querySelector("canvas");
    if (!canvas) continue;
    const page = await pdfDoc.getPage(i + 1);
    if (myRenderId !== renderId) return;
    let viewport = page.getViewport({ scale: renderScale });
    const pixels = viewport.width * viewport.height;
    if (pixels > MAX_CANVAS_PIXELS) {
      viewport = page.getViewport({
        scale: renderScale * Math.sqrt(MAX_CANVAS_PIXELS / pixels),
      });
    }
    canvas.width = Math.floor(viewport.width);
    canvas.height = Math.floor(viewport.height);
    canvas.style.width = `${pageInfos[i]!.width * currentScale}px`;
    canvas.style.height = `${pageInfos[i]!.height * currentScale}px`;
    const ctx = canvas.getContext("2d");
    if (!ctx) continue;
    await page.render({ canvas, canvasContext: ctx, viewport }).promise;
    if (myRenderId !== renderId) return;
    const textLayerDiv = pageDiv.querySelector(".textLayer");
    if (textLayerDiv instanceof HTMLElement) {
      textLayerDiv.innerHTML = "";
      textLayerDiv.style.setProperty(
        "--total-scale-factor",
        String(currentScale),
      );
      const textContent = await page.getTextContent();
      if (myRenderId !== renderId) return;
      const textLayer = new pdfjsLib.TextLayer({
        textContentSource: textContent,
        container: textLayerDiv,
        viewport: page.getViewport({ scale: currentScale }),
      });
      await textLayer.render();
    }
  }
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

/** 加载 PDF（base64），渲染所有页到容器，返回 JSON 结果。 */
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
  container.innerHTML = "";
  pageInfos = [];
  for (let i = 1; i <= pdfDoc.numPages; i++) {
    const pageDiv = document.createElement("div");
    pageDiv.dataset.page = String(i);
    pageDiv.style.display = "block";
    pageDiv.style.position = "relative";
    pageDiv.style.marginBottom = "8px";
    const canvas = document.createElement("canvas");
    canvas.style.display = "block";
    pageDiv.appendChild(canvas);
    const textLayerDiv = document.createElement("div");
    textLayerDiv.className = "textLayer";
    pageDiv.appendChild(textLayerDiv);
    container.appendChild(pageDiv);
  }
  for (let i = 1; i <= pdfDoc.numPages; i++) {
    const page = await pdfDoc.getPage(i);
    const v = page.getViewport({ scale: 1.0 });
    pageInfos.push({ width: v.width, height: v.height });
  }
  if (myId !== loadId) return JSON.stringify(null);
  cachedContentRect = await computeContentRect();
  await renderAllPages();
  const result = {
    numPages: pdfDoc.numPages,
    pageInfos,
    contentRect: cachedContentRect,
  };
  window.dispatchEvent(new CustomEvent("pdf-loaded", { detail: result }));
  return JSON.stringify(result);
}

/** 设置缩放并重渲染。 */
export function setPdfScale(scale: number): void {
  if (!pdfDoc) return;
  currentScale = scale;
  void renderAllPages();
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
  if (pdfDoc) {
    void (pdfDoc as unknown as { destroy: () => Promise<void> }).destroy();
  }
  pdfDoc = null;
  if (container) {
    container.innerHTML = "";
  }
  container = null;
  pageInfos = [];
  cachedContentRect = null;
  loadId++;
}
