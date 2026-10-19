// 滚动位置存储在 sessionStorage 中，独立于模块作用域和 JS 对象生命周期。
// sessionStorage 是浏览器原生存储，按 origin 隔离，在 WebView 中同样可用。
const SCROLL_KEY = "mt_note_home_scroll";

function readScroll(): number {
  const v = parseFloat(sessionStorage.getItem(SCROLL_KEY) || "0");
  console.log("readScroll", v);
  return v;
}

function writeScroll(value: number): void {
  console.log("writeScroll", value);
  sessionStorage.setItem(SCROLL_KEY, String(value));
}

/** 初始化首页滚动监听：记录滚动位置，用于页面返回时恢复。 */
export function initHomeScroll(): void {
  const el = document.getElementById("entry_list") as HTMLElement & {
    __scrollInited?: boolean;
  };
  if (el && !el.__scrollInited) {
    el.__scrollInited = true;
    el.addEventListener("scroll", () => {
      // 同步检查：isConnected 拦截元素已移除的情况，
      // scrollTop > 0 拦截子元素清空时浏览器归零 scrollTop 的情况。
      if (
        el.isConnected &&
        el.scrollTop > 0 &&
        el.scrollHeight > el.clientHeight
      ) {
        writeScroll(el.scrollTop);
      }
    });
  }
}

/** 保存当前首页滚动位置。由 Rust 侧在导航离开前调用。 */
export function saveHomeScroll(): void {
  const el = document.getElementById("entry_list") as HTMLElement | null;
  if (el && el.scrollTop > 0) {
    writeScroll(el.scrollTop);
  }
}

/** 获取首页滚动位置。 */
export function getHomeScrollTop(): number {
  const v = readScroll();
  console.log("getHomeScrollTop", v);
  return v;
}

/** 恢复首页滚动位置。 */
export function setHomeScrollTop(scrollTop: number): void {
  console.log("setHomeScrollTop", scrollTop);
  requestAnimationFrame(() => {
    const el = document.getElementById("entry_list");
    if (el) {
      el.scrollTop = scrollTop;
    }
  });
}

/** 重置首页滚动位置：清除记录并滚动到顶部。用于目录切换时。 */
export function resetHomeScroll(): void {
  console.log("resetHomeScroll");
  writeScroll(0);
  requestAnimationFrame(() => {
    const el = document.getElementById("entry_list");
    if (el) {
      el.scrollTop = 0;
    }
  });
}
