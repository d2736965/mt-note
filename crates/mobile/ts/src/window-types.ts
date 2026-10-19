/** @rust-lint expect(dead_code, reason = "仅用于类型转换，不直接构造") */
export interface AppWindow extends Window {
  __pinch: {
    sx: number;
    sy: number;
    fx: number;
    fy: number;
    vl: number;
    vt: number;
    scale0: number;
    dist0: number;
    pageW: number;
    ox: number;
    oy: number;
    el: HTMLElement;
    ns: number;
  } | null;
  __afterPinch: boolean;
  __snapActive: boolean;
  __snapShouldBeActive: boolean;
  __snapMLP: number;
  __snapCWP: number;
  __snapCustomTarget: number | null;
  __pdfPreview: {
    configure: (workerUrl: string, cmapsUrl: string) => void;
    render: {
      load: (b64: string, cid: string, scale: number) => Promise<string>;
      setScale: (scale: number) => void;
    };
    query: {
      pageInfos: () => string;
      contentRect: () => string;
    };
    cleanup: () => void;
  } | null;
  __svgT: {
    s: number;
    tx: number;
    ty: number;
  } | null;
  __svgPinch: {
    vl: number;
    vt: number;
    /** 落指瞬间锁定的内容点（元素 px 坐标），每帧投到实时双指中点下 */
    cx: number;
    cy: number;
    el: HTMLElement;
  } | null;
  __svgDrag: {
    sx: number;
    sy: number;
    tx0: number;
    ty0: number;
  } | null;
  __bi: number[];
  __editorScrollSync: null;
  __cursorTracker: null;
  __vrq: null;
  __vrr: null;
  __crq: null;
  __crr: null;
  __savedMdScroll: number;
  __savedMdScrollPath: string;
}

declare global {
  interface HTMLElement {
    __snapBound?: boolean;
  }
}
