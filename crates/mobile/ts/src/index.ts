import {
  initEditorScrollSync,
  initCursorTracker,
  getEditorInfo,
  getLinePosition,
  scrollEditor,
  getMaxScroll,
  getCursorInfo,
  getPreviewAnchorInfo,
  getPreviewScrollRatio,
  scrollPreviewToAnchor,
  scrollPreviewToRatio,
  getVisibleBounds,
  cleanupEditor,
  clipboardWrite,
  deleteLine,
  setTextareaValue,
  formatDateTime,
  insertTextAtCursor,
  getRecentFormats,
  saveRecentFormat,
  getDeviceDateFormat,
  getDeviceTimeFormat,
  previewDateTime,
  getSavedMdScroll,
  getSavedMdScrollPath,
  clearSavedMdScroll,
} from "./editor";
import {
  pinchStart,
  pinchMove,
  pinchEnd,
  resetSnap,
  injectSnap,
  scrollToContentCenter,
  toggleTextLayerDebug,
  alignTextLayer,
  setCustomSnapTarget,
  clearCustomSnapTarget,
  cleanupPinchZoom,
} from "./pinch-zoom";
import {
  resetSvgState,
  applySvgScale,
  getSvgContentBBox,
  applySvgFit,
  svgPinchStart,
  svgPinchMove,
  svgDragStart,
  svgDragMove,
  cleanupSvgPreview,
} from "./svg-preview";
import {
  injectPdfPageNumbers,
  injectTypstPageNumbers,
  showPageNumbers,
  hidePageNumbers,
} from "./page-indicator";
import { pushBack, removeBack, getBackStackTop } from "./back-interceptor";
import {
  initHomeScroll,
  getHomeScrollTop,
  setHomeScrollTop,
  resetHomeScroll,
  saveHomeScroll,
} from "./scroll";
import { createI18n } from "./i18n";
import type { I18n } from "./i18n";
import { renderHighlightAndKatex } from "./markdown";
import { closeLightbox, initTypstPreviewLightbox } from "./image-lightbox";
import {
  configure,
  loadPdf,
  cleanupPdfPreview,
  setPdfScale,
  getPdfPageInfos,
  getPdfContentRect,
} from "./pdf-preview";
import type { AppWindow } from "./window-types";

/** Rust ↔ JS 事件通道：CustomEvent 队列，支持异步等待。 */
export interface Events {
  waitForEvent: (eventName: string) => Promise<string>;
  /** @rust-lint expect(dead_code, reason = "由 Rust 侧调用") */
  cleanupAll: () => void;
  resetAll: () => void;
  /** @rust-lint expect(dead_code, reason = "由 Rust 侧调用") */
  destroy: () => void;
}

/** 文本编辑器：滚动同步、光标跟踪、剪贴板。 */
export interface Editor {
  initScrollSync: (textareaId: string, highlightId: string) => void;
  initCursorTracker: (textareaId: string) => void;
  getInfo: () => string;
  getLinePosition: (line: number) => number;
  scrollTo: (targetScroll: number) => void;
  getMaxScroll: () => number;
  getCursorInfo: () => string;
  getPreviewAnchorInfo: (previewClass: string) => string;
  getPreviewScrollRatio: (previewClass: string) => number;
  scrollPreviewToAnchor: (
    previewClass: string,
    anchorLine: number,
    anchorRatio: number,
  ) => void;
  scrollPreviewToRatio: (previewClass: string, ratio: number) => void;
  getVisibleBounds: () => string;
  /** @rust-lint expect(dead_code, reason = "由 Rust 侧调用") */
  cleanup: () => void;
  clipboardWrite: (text: string) => void;
  deleteLine: () => string;
  setTextareaValue: (value: string) => boolean;
  formatDateTime: (format: string, timestamp?: number) => string;
  insertTextAtCursor: (text: string) => void;
  getRecentFormats: () => string;
  saveRecentFormat: (format: string) => void;
  getDeviceDateFormat: () => string;
  getDeviceTimeFormat: () => string;
  previewDateTime: (
    format: string,
    dateStr: string,
    timeStr: string,
    useCurrent: boolean,
  ) => string;
  /** @rust-lint expect(dead_code, reason = "由 Rust 侧调用") */
  destroy: () => void;
  getSavedMdScroll: () => number;
  getSavedMdScrollPath: () => string;
  clearSavedMdScroll: () => void;
}

/** 双指捏合缩放：焦点计算、滚动位置同步、snap 吸附。 */
export interface PinchZoom {
  /** @rust-lint expect(clippy::too_many_arguments, reason = "捏合缩放需要所有初始状态参数") */
  start: (
    elementId: string,
    fx: number,
    fy: number,
    vl: number,
    vt: number,
    s0: number,
    dist: number,
    pw: number,
  ) => void;
  zoom: (ns: number, k: number) => void;
  end: (ns: number) => void;
  resetSnap: () => void;
  injectSnap: (
    elementId: string,
    mlp: number,
    cwp: number,
    active: boolean,
  ) => void;
  scrollToContentCenter: (
    elementId: string,
    pageWidth: number,
    contentCenter: number | null,
  ) => void;
  toggleTextLayerDebug: (
    elementId: string,
    styleId: string,
    visible: boolean,
    cssContent: string,
  ) => void;
  alignTextLayer: (containerId: string) => void;
  setCustomSnapTarget: (elementId: string) => void;
  clearCustomSnapTarget: (elementId: string) => void;
  /** @rust-lint expect(dead_code, reason = "由 Rust 侧调用") */
  cleanup: () => void;
  /** @rust-lint expect(dead_code, reason = "由 Rust 侧调用") */
  destroy: () => void;
}

/** SVG 预览：缩放、适配、拖拽、捏合。 */
export interface SvgPreview {
  resetState: () => void;
  applyScale: (
    cid: string,
    s: number,
    vl: number,
    vt: number,
    vw: number,
    vh: number,
  ) => void;
  applyFit: (cid: string, fit: number, tx: number, ty: number) => void;
  /** 读取 SVG 内容包围盒（JSON 字符串，空内容为 "null"）。 */
  getSvgContentBBox: (cid: string) => string;
  pinchStart: (
    cid: string,
    fx: number,
    fy: number,
    vl: number,
    vt: number,
  ) => void;
  /** 实时双指中点为焦点：落指锁定的内容点跟随中点。 */
  pinchMove: (ns: number, fx: number, fy: number) => void;
  dragStart: (cid: string, x: number, y: number) => void;
  dragMove: (cid: string, x: number, y: number) => void;
  /** @rust-lint expect(dead_code, reason = "由 Rust 侧调用") */
  cleanup: () => void;
  /** @rust-lint expect(dead_code, reason = "由 Rust 侧调用") */
  destroy: () => void;
}

/** 页码指示器：注入 PDF/Typst 页号标签，显示/隐藏。 */
export interface PageIndicator {
  injectPdf: (containerId: string) => void;
  injectTypst: (containerId: string) => void;
  show: () => void;
  hide: () => void;
  /** @rust-lint expect(dead_code, reason = "由 Rust 侧调用") */
  destroy: () => void;
}

/** 返回键拦截：导航栈管理，Android 物理返回键处理。 */
export interface BackInterceptor {
  push: (id: number) => void;
  remove: (id: number) => void;
  getStackTop: () => string;
  /** @rust-lint expect(dead_code, reason = "由 Rust 侧调用") */
  destroy: () => void;
}

/** 首页滚动位置记录与恢复。 */
export interface Scroll {
  initHomeScroll: () => void;
  getHomeScrollTop: () => number;
  setHomeScrollTop: (scrollTop: number) => void;
  resetHomeScroll: () => void;
  saveHomeScroll: () => void;
  /** @rust-lint expect(dead_code, reason = "由 Rust 侧调用") */
  destroy: () => void;
}

/** Markdown 渲染：代码高亮、KaTeX 数学公式、Mermaid 图表、图形灯箱。 */
export interface Markdown {
  render: (mdId: string) => void;
  /** 关闭图形灯箱（Android 返回键拦截回调）。 */
  closeLightbox: () => void;
  /** @rust-lint expect(dead_code, reason = "由 Rust 侧调用") */
  destroy: () => void;
}

/** PDF 预览：pdfjs 渲染，worker/cmaps 配置，缩放控制。 */
export interface PdfPreview {
  configure: (workerUrl: string, cmapsUrl: string) => void;
  render: {
    load: (b64: string, cid: string, scale: number) => Promise<string>;
    setScale: (scale: number) => void;
  };
  query: {
    pageInfos: () => string;
    /** @rust-lint expect(dead_code) */
    contentRect: () => string;
  };
  cleanup: () => void;
  /** @rust-lint expect(dead_code, reason = "由 Rust 侧调用") */
  destroy: () => void;
}

const eventQueues: Record<
  string,
  { queue: string[]; resolvers: Array<(v: string) => void> }
> = {};

/** CustomEvent 队列：缓冲事件，支持异步等待（Rust 侧通过 waitForEvent 轮询）。 */
function getEventQueue(eventName: string) {
  if (!eventQueues[eventName]) {
    eventQueues[eventName] = { queue: [], resolvers: [] };
    window.addEventListener(eventName, (e: Event) => {
      const json = JSON.stringify((e as CustomEvent).detail ?? {});
      const eq = eventQueues[eventName];
      if (!eq) return;
      if (eq.resolvers.length > 0) {
        eq.resolvers.shift()!(json);
      } else {
        eq.queue.push(json);
      }
    });
  }
  return eventQueues[eventName];
}

export function createEvents(): Events {
  return {
    waitForEvent: (eventName: string): Promise<string> =>
      new Promise((resolve) => {
        const eq = getEventQueue(eventName);
        if (eq.queue.length > 0) {
          resolve(eq.queue.shift()!);
        } else {
          eq.resolvers.push(resolve);
        }
      }),

    cleanupAll: () => {
      for (const key of Object.keys(eventQueues)) {
        delete eventQueues[key];
      }
    },

    destroy: () => {
      for (const key of Object.keys(eventQueues)) {
        const eq = eventQueues[key]!;
        eq.queue.length = 0;
        eq.resolvers.length = 0;
        delete eventQueues[key];
      }
    },

    resetAll: () => {
      cleanupEditor();
      cleanupPinchZoom();
      cleanupSvgPreview();
      const pdf = (window as unknown as AppWindow).__pdfPreview;
      if (pdf && pdf.cleanup) pdf.cleanup();
    },
  };
}

export function createEditor(): Editor {
  return {
    initScrollSync: initEditorScrollSync,
    initCursorTracker,
    getInfo: () => JSON.stringify(getEditorInfo()),
    getLinePosition,
    scrollTo: scrollEditor,
    getMaxScroll,
    getCursorInfo: () => JSON.stringify(getCursorInfo()),
    getPreviewAnchorInfo: (previewClass: string) =>
      JSON.stringify(getPreviewAnchorInfo(previewClass)),
    getPreviewScrollRatio,
    scrollPreviewToAnchor,
    scrollPreviewToRatio,
    getVisibleBounds: () => JSON.stringify(getVisibleBounds()),
    cleanup: cleanupEditor,
    clipboardWrite,
    deleteLine,
    setTextareaValue,
    formatDateTime,
    insertTextAtCursor,
    getRecentFormats,
    saveRecentFormat,
    getDeviceDateFormat,
    getDeviceTimeFormat,
    previewDateTime,
    getSavedMdScroll,
    getSavedMdScrollPath,
    clearSavedMdScroll,
    destroy: () => {
      cleanupEditor();
    },
  };
}

export function createPinchZoom(): PinchZoom {
  return {
    start: pinchStart,
    zoom: pinchMove,
    end: pinchEnd,
    resetSnap,
    injectSnap,
    scrollToContentCenter,
    toggleTextLayerDebug,
    alignTextLayer,
    setCustomSnapTarget,
    clearCustomSnapTarget,
    cleanup: cleanupPinchZoom,
    destroy: () => {
      cleanupPinchZoom();
    },
  };
}

export function createSvgPreview(): SvgPreview {
  return {
    resetState: resetSvgState,
    applyScale: applySvgScale,
    applyFit: applySvgFit,
    getSvgContentBBox,
    pinchStart: svgPinchStart,
    pinchMove: svgPinchMove,
    dragStart: svgDragStart,
    dragMove: svgDragMove,
    cleanup: cleanupSvgPreview,
    destroy: () => {
      cleanupSvgPreview();
    },
  };
}

export function createPageIndicator(): PageIndicator {
  return {
    injectPdf: injectPdfPageNumbers,
    injectTypst: injectTypstPageNumbers,
    show: showPageNumbers,
    hide: hidePageNumbers,
    destroy: () => {},
  };
}

export function createBackInterceptor(): BackInterceptor {
  return {
    push: pushBack,
    remove: removeBack,
    getStackTop: getBackStackTop,
    destroy: () => {},
  };
}

export function createScroll(): Scroll {
  return {
    initHomeScroll,
    getHomeScrollTop,
    setHomeScrollTop,
    resetHomeScroll,
    saveHomeScroll,
    destroy: () => {},
  };
}

export function createMarkdown(): Markdown {
  return {
    render: renderHighlightAndKatex,
    closeLightbox,
    destroy: () => {},
  };
}

export function createPdfPreview(): PdfPreview {
  return {
    configure,
    render: {
      load: loadPdf,
      setScale: setPdfScale,
    },
    query: {
      pageInfos: getPdfPageInfos,
      contentRect: getPdfContentRect,
    },
    cleanup: cleanupPdfPreview,
    destroy: () => {
      cleanupPdfPreview();
    },
  };
}

/** 全局工厂对象：js-bridge-bindgen 据此生成 Rust 绑定。仅声明工厂方法，内部变量见 window-types.ts。 */
declare global {
  interface Window {
    __mt_note_mobile: {
      createEvents: () => Events;
      createEditor: () => Editor;
      createPinchZoom: () => PinchZoom;
      createSvgPreview: () => SvgPreview;
      createPageIndicator: () => PageIndicator;
      createBackInterceptor: () => BackInterceptor;
      createScroll: () => Scroll;
      createMarkdown: () => Markdown;
      createPdfPreview: () => PdfPreview;
      createI18n: () => I18n;
    };
  }
}

window.__mt_note_mobile = {
  createEvents,
  createEditor,
  createPinchZoom,
  createSvgPreview,
  createPageIndicator,
  createBackInterceptor,
  createScroll,
  createMarkdown,
  createPdfPreview,
  createI18n,
};

// Typst 预览的灯箱入口：文档级单击委托，模块加载时绑定一次（不进 bindgen，
// 无需 Rust 侧调用）
initTypstPreviewLightbox();
