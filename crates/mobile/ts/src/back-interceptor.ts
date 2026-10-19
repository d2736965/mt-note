import type { AppWindow } from "./window-types";

const w = window as unknown as AppWindow;

/** 推入返回栈（页面导航时记录）。 */
export function pushBack(id: number): void {
  if (!w.__bi) {
    w.__bi = [];
  }
  w.__bi.push(id);
}

/** 移除返回栈条目（页面销毁时）。 */
export function removeBack(id: number): void {
  if (w.__bi) {
    const i = w.__bi.indexOf(id);
    if (i !== -1) {
      w.__bi.splice(i, 1);
    }
  }
}

/** 获取栈顶导航 ID（供 Rust 侧判断是否拦截返回键）。 */
export function getBackStackTop(): string {
  return w.__bi && w.__bi.length > 0 ? `i:${w.__bi[w.__bi.length - 1]}` : "1";
}
