/** @rust-lint expect(dead_code, reason = "由调用方 JSON 反序列化使用") */
export interface EditorScrollInfo {
  max: number;
  clientHeight: number;
}

/** @rust-lint expect(dead_code, reason = "由调用方 JSON 反序列化使用") */
export interface CursorInfo {
  line: number;
  total: number;
  ratio: number;
}

import type { AppWindow } from "./window-types";

const w = window as unknown as AppWindow;

/** @rust-lint expect(dead_code, reason = "由调用方 JSON 反序列化使用") */
export interface PreviewAnchorInfo {
  ratio: number;
  line: number;
  anchorRatio: number;
}

/** @rust-lint expect(dead_code, reason = "由调用方 JSON 反序列化使用") */
export interface VisibleBounds {
  visible_top: number;
  visible_bottom: number;
}

/** 初始化编辑器滚动同步：textarea 滚动时同步 highlight 层 scrollTop/scrollLeft。 */
export function initEditorScrollSync(
  textareaId: string,
  highlightId: string,
): void {
  const ta = document.getElementById(textareaId);
  const hl = document.getElementById(highlightId);
  if (!ta || !hl) return;

  ta.addEventListener(
    "scroll",
    () => {
      hl.scrollTop = ta.scrollTop;
      hl.scrollLeft = ta.scrollLeft;
    },
    { passive: true },
  );
}

/** 初始化光标跟踪：监听 input/click/keyup/select 事件，派发 cursor-change 事件。 */
export function initCursorTracker(textareaId: string): void {
  const ta = document.getElementById(textareaId) as HTMLTextAreaElement | null;
  if (!ta) return;

  function computeAndNotify(): void {
    const v = ta!.value;
    const s = ta!.selectionStart;
    const b = v.substring(0, s);
    const ls = b.split("\n");
    const last = ls[ls.length - 1] ?? "";
    window.dispatchEvent(
      new CustomEvent("cursor-change", {
        detail: {
          line: ls.length - 1,
          column: new TextEncoder().encode(last).length,
          char_column: last.length,
          scrollTop: ta!.scrollTop,
          scrollLeft: ta!.scrollLeft,
        },
      }),
    );
  }

  ta.addEventListener("input", computeAndNotify, { passive: true });
  ta.addEventListener("click", computeAndNotify, { passive: true });
  ta.addEventListener(
    "keyup",
    (e) => {
      if (
        [
          "ArrowUp",
          "ArrowDown",
          "ArrowLeft",
          "ArrowRight",
          "Home",
          "End",
        ].indexOf(e.key) !== -1
      ) {
        computeAndNotify();
      }
    },
    { passive: true },
  );
  ta.addEventListener("select", computeAndNotify, { passive: true });
}

/** 获取编辑器滚动信息（max scroll, clientHeight）。 */
export function getEditorInfo(): EditorScrollInfo {
  const ta = document.querySelector("textarea");
  if (!ta) return { max: 0, clientHeight: 0 };
  const max = ta.scrollHeight - ta.clientHeight;
  return { max: max > 0 ? max : 0, clientHeight: ta.clientHeight };
}

export function getLinePosition(line: number): number {
  const ta = document.querySelector("textarea");
  if (!ta) return 0;
  const v = (ta as HTMLTextAreaElement).value;
  const total = v.split("\n").length;
  const lh =
    parseFloat(getComputedStyle(ta).lineHeight) || ta.clientHeight / total;
  return line * lh;
}

/** 滚动编辑器到指定位置（同时同步 highlight 层，兼容 Android WebView）。 */
export function scrollEditor(targetScroll: number): void {
  const ta = document.querySelector("textarea") as HTMLTextAreaElement | null;
  if (!ta) return;
  const hl = ta.parentElement?.querySelector(
    '[class*="highlight"]',
  ) as HTMLElement | null;
  ta.scrollTop = targetScroll;
  if (hl) hl.scrollTop = targetScroll;
}

export function getMaxScroll(): number {
  const ta = document.querySelector("textarea");
  if (!ta) return 0;
  const max = ta.scrollHeight - ta.clientHeight;
  return max > 0 ? max : 0;
}

/** 获取光标信息：当前行号、总行数、在视口中的比例。 */
export function getCursorInfo(): CursorInfo {
  const ta = document.querySelector("textarea") as HTMLTextAreaElement | null;
  if (!ta) return { line: 0, total: 1, ratio: 0.5 };
  const v = ta.value;
  const s = ta.selectionStart;
  const b = v.substring(0, s);
  const line = b.split("\n").length - 1;
  const total = v.split("\n").length;
  const lh =
    parseFloat(getComputedStyle(ta).lineHeight) || ta.clientHeight / total;
  const lineTop = line * lh;
  const lineBot = lineTop + lh;
  const visible =
    lineTop >= ta.scrollTop && lineBot <= ta.scrollTop + ta.clientHeight;
  if (visible) {
    const ratio =
      ta.clientHeight > 0 ? (lineTop - ta.scrollTop) / ta.clientHeight : 0.5;
    return { line, total, ratio: Math.max(0, Math.min(1, ratio)) };
  } else {
    const centerLine = Math.min(
      Math.floor((ta.scrollTop + ta.clientHeight / 2) / lh),
      total - 1,
    );
    return { line: Math.max(0, centerLine), total, ratio: 0.5 };
  }
}

/** 获取预览区锚点信息：滚动比例、当前行号、锚点在视口中的比例。 */
export function getPreviewAnchorInfo(selector: string): PreviewAnchorInfo {
  const c = document.querySelector(selector) as HTMLElement | null;
  if (!c) return { ratio: 0, line: 0, anchorRatio: 0.5 };
  const max = c.scrollHeight - c.clientHeight;
  const ratio = max > 0 ? c.scrollTop / max : 0;
  const markers = c.querySelectorAll("[data-source-line]");
  if (markers.length === 0) return { ratio, line: 0, anchorRatio: 0.5 };
  const cRect = c.getBoundingClientRect();
  const viewMid = cRect.top + cRect.height / 2;
  let best: Element | null = null;
  let bestDist = Infinity;
  for (let i = 0; i < markers.length; i++) {
    const m = markers.item(i);
    if (!m) continue;
    const r = m.getBoundingClientRect();
    const mid = r.top + r.height / 2;
    const dist = Math.abs(mid - viewMid);
    if (dist < bestDist) {
      bestDist = dist;
      best = m;
    }
  }
  if (!best) return { ratio, line: 0, anchorRatio: 0.5 };
  const line = parseInt(best.getAttribute("data-source-line") || "0") || 0;
  const bRect = best.getBoundingClientRect();
  const anchorRatio =
    cRect.height > 0 ? (bRect.top - cRect.top) / cRect.height : 0.5;
  return { ratio, line, anchorRatio: Math.max(0, Math.min(1, anchorRatio)) };
}

export function getPreviewScrollRatio(previewClass: string): number {
  const c = document.querySelector(`.${previewClass}`) as HTMLElement | null;
  if (!c) return 0;
  const max = c.scrollHeight - c.clientHeight;
  return max > 0 ? c.scrollTop / max : 0;
}

/** 滚动预览区到指定锚点（找到最近的 data-source-line 标记后滚动）。 */
export function scrollPreviewToAnchor(
  previewClass: string,
  anchorLine: number,
  anchorRatio: number,
): void {
  setTimeout(() => {
    const container = document.querySelector(
      `.${previewClass}`,
    ) as HTMLElement | null;
    if (!container) return;
    const markers = container.querySelectorAll("[data-source-line]");
    if (markers.length === 0) return;
    let best: Element | null = null;
    let bestDiff = Infinity;
    for (let i = 0; i < markers.length; i++) {
      const m = markers.item(i);
      if (!m) continue;
      const ln = parseInt(m.getAttribute("data-source-line") || "0");
      const diff = Math.abs(ln - anchorLine);
      if (diff < bestDiff) {
        bestDiff = diff;
        best = m;
      }
    }
    if (!best) return;
    const cRect = container.getBoundingClientRect();
    const bRect = best.getBoundingClientRect();
    const elTop = bRect.top - cRect.top + container.scrollTop;
    const targetScroll = elTop - anchorRatio * container.clientHeight;
    container.scrollTop = Math.max(
      0,
      Math.min(container.scrollHeight - container.clientHeight, targetScroll),
    );
  }, 100);
}

/** 获取可视区域边界（相对编辑器的 top/bottom）。 */
export function getVisibleBounds(): VisibleBounds {
  const vv = window.visualViewport;
  const el = document.querySelector(
    '[class*="editor_content"]',
  ) as HTMLElement | null;
  if (!el || !vv) return { visible_top: 0, visible_bottom: 0 };
  const r = el.getBoundingClientRect();
  const vv_bottom = vv.offsetTop + vv.height;
  const vt = Math.max(0, vv.offsetTop - r.top);
  const vb = Math.min(r.height, vv_bottom - r.top);
  return { visible_top: vt, visible_bottom: vb };
}

/** 清理编辑器状态（页面切换时调用）。 */
export function cleanupEditor(): void {
  w.__editorScrollSync = null;
  w.__cursorTracker = null;
  w.__vrq = null;
  w.__vrr = null;
  w.__crq = null;
  w.__crr = null;
}

/** 按比例滚动预览区（用于恢复滚动位置）。 */
export function scrollPreviewToRatio(
  previewClass: string,
  ratio: number,
): void {
  setTimeout(() => {
    const c = document.querySelector(`.${previewClass}`) as HTMLElement | null;
    if (!c) return;
    const max = c.scrollHeight - c.clientHeight;
    if (max > 0) c.scrollTop = max * ratio;
  }, 500);
}

export function clipboardWrite(text: string): void {
  if (navigator.clipboard && navigator.clipboard.writeText) {
    navigator.clipboard.writeText(text);
  }
}

/** 删除光标所在行，返回新的 textarea 内容。 */
export function deleteLine(): string {
  const ta = document.querySelector("textarea") as HTMLTextAreaElement | null;
  if (!ta) return "";
  ta.focus();
  const v = ta.value;
  const s = ta.selectionStart;
  const e = ta.selectionEnd;

  let lineStart = s;
  while (lineStart > 0 && v[lineStart - 1] !== "\n") lineStart--;
  let lineEnd = e;
  while (lineEnd < v.length && v[lineEnd] !== "\n") lineEnd++;

  let cursorPos: number;
  if (lineEnd < v.length) {
    lineEnd++;
    cursorPos = lineStart;
  } else if (lineStart > 0) {
    lineStart--;
    cursorPos = lineStart;
  } else {
    cursorPos = 0;
  }

  const newValue = v.substring(0, lineStart) + v.substring(lineEnd);
  ta.value = newValue;
  ta.selectionStart = cursorPos;
  ta.selectionEnd = cursorPos;
  return newValue;
}

/** 程序化设置 textarea 的值（补全、重命名等场景）。 */
export function setTextareaValue(value: string): boolean {
  const ta = document.querySelector("textarea") as HTMLTextAreaElement | null;
  if (!ta) return false;
  ta.value = value;
  return true;
}

/** 格式化日期时间（SimpleDateFormat 风格）。可传入 timestamp 毫秒值，默认当前时间。 */
export function formatDateTime(format: string, timestamp?: number): string {
  const date = typeof timestamp === "number" ? new Date(timestamp) : new Date();
  const months = [
    "Jan",
    "Feb",
    "Mar",
    "Apr",
    "May",
    "Jun",
    "Jul",
    "Aug",
    "Sep",
    "Oct",
    "Nov",
    "Dec",
  ];
  const monthsFull = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
  ];
  const weekdays = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
  const weekdaysFull = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
  ];

  const pad = (n: number, len = 2): string => String(n).padStart(len, "0");

  const tokens: Record<string, string> = {
    yyyy: String(date.getFullYear()),
    yy: String(date.getFullYear()).slice(-2),
    MMMM: monthsFull[date.getMonth()]!,
    MMM: months[date.getMonth()]!,
    MM: pad(date.getMonth() + 1),
    M: String(date.getMonth() + 1),
    dd: pad(date.getDate()),
    d: String(date.getDate()),
    EEEE: weekdaysFull[date.getDay()]!,
    EEE: weekdays[date.getDay()]!,
    HH: pad(date.getHours()),
    H: String(date.getHours()),
    hh: pad(date.getHours() % 12 || 12),
    h: String(date.getHours() % 12 || 12),
    mm: pad(date.getMinutes()),
    m: String(date.getMinutes()),
    ss: pad(date.getSeconds()),
    s: String(date.getSeconds()),
    SSS: pad(date.getMilliseconds(), 3),
    a: date.getHours() < 12 ? "AM" : "PM",
  };

  const sortedTokens = Object.keys(tokens).sort((a, b) => b.length - a.length);

  let result = "";
  let i = 0;
  while (i < format.length) {
    let matched = false;
    for (const token of sortedTokens) {
      if (format.startsWith(token, i)) {
        result += tokens[token];
        i += token.length;
        matched = true;
        break;
      }
    }
    if (!matched) {
      result += format[i];
      i++;
    }
  }
  return result;
}

/** 在光标位置插入文本（不通过 input 事件，直接操作 textarea）。 */
export function insertTextAtCursor(text: string): void {
  const ta = document.querySelector("textarea") as HTMLTextAreaElement | null;
  if (!ta) return;
  ta.focus();
  const start = ta.selectionStart;
  const end = ta.selectionEnd;
  ta.value = ta.value.substring(0, start) + text + ta.value.substring(end);
  ta.selectionStart = ta.selectionEnd = start + text.length;
  ta.dispatchEvent(new Event("input", { bubbles: true }));
}

const RECENT_FORMATS_KEY = "datetime_dialog_recent_formats";
const MAX_RECENT_FORMATS = 5;

/** 获取最近使用的日期时间格式列表（JSON 字符串）。 */
export function getRecentFormats(): string {
  try {
    const raw = localStorage.getItem(RECENT_FORMATS_KEY);
    if (!raw) return "[]";
    const parsed: unknown = JSON.parse(raw);
    if (Array.isArray(parsed)) {
      const filtered = parsed.filter((v): v is string => typeof v === "string");
      return JSON.stringify(filtered);
    }
    return "[]";
  } catch {
    return "[]";
  }
}

/** 保存格式到最近使用列表。 */
export function saveRecentFormat(format: string): void {
  if (!format.trim()) return;
  const recentStr = getRecentFormats();
  let recent: string[];
  try {
    recent = JSON.parse(recentStr) as string[];
    if (!Array.isArray(recent)) recent = [];
  } catch {
    recent = [];
  }
  const filtered = recent.filter((f) => f !== format);
  filtered.unshift(format);
  const trimmed = filtered.slice(0, MAX_RECENT_FORMATS);
  localStorage.setItem(RECENT_FORMATS_KEY, JSON.stringify(trimmed));
}

/** 获取系统本地化的日期格式（基于 navigator.language）。 */
export function getDeviceDateFormat(): string {
  const lang = navigator.language || "en-US";
  try {
    const fmt = new Intl.DateTimeFormat(lang, {
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
    });
    const parts = fmt.formatToParts(new Date(2020, 0, 1));
    let result = "";
    for (const p of parts) {
      switch (p.type) {
        case "year":
          result += "yyyy";
          break;
        case "month":
          result += "MM";
          break;
        case "day":
          result += "dd";
          break;
        default:
          result += p.value;
      }
    }
    return result;
  } catch {
    return "yyyy-MM-dd";
  }
}

/** 获取系统本地化的时间格式（基于 navigator.language）。 */
export function getDeviceTimeFormat(): string {
  const lang = navigator.language || "en-US";
  try {
    const fmt = new Intl.DateTimeFormat(lang, {
      hour: "2-digit",
      minute: "2-digit",
    });
    const parts = fmt.formatToParts(new Date(2020, 0, 1, 13, 0));
    let result = "";
    for (const p of parts) {
      switch (p.type) {
        case "hour":
          result += "HH";
          break;
        case "minute":
          result += "mm";
          break;
        default:
          result += p.value;
      }
    }
    return result;
  } catch {
    return "HH:mm";
  }
}

/** 根据日期/时间组件生成预览文本。 */
export function previewDateTime(
  format: string,
  dateStr: string,
  timeStr: string,
  useCurrent: boolean,
): string {
  if (useCurrent) {
    return formatDateTime(format);
  }
  const [y, m, d] = dateStr.split("-").map(Number);
  const [h, min] = timeStr.split(":").map(Number);
  if (y === undefined || m === undefined || d === undefined)
    return formatDateTime(format);
  const hour = h ?? 0;
  const minute = min ?? 0;
  const timestamp = new Date(y, m - 1, d, hour, minute).getTime();
  return formatDateTime(format, timestamp);
}

/** 获取已保存的 Markdown 预览滚动比例（-1 表示无保存值）。 */
export function getSavedMdScroll(): number {
  return (window as unknown as AppWindow).__savedMdScroll ?? -1;
}

/** 获取已保存的 Markdown 预览滚动对应的文件路径（空串表示无保存值）。 */
export function getSavedMdScrollPath(): string {
  return (window as unknown as AppWindow).__savedMdScrollPath ?? "";
}

/** 清除已保存的 Markdown 预览滚动位置。 */
export function clearSavedMdScroll(): void {
  (window as unknown as AppWindow).__savedMdScroll = -1;
  (window as unknown as AppWindow).__savedMdScrollPath = "";
}
