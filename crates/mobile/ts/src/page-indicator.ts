/** 注入 PDF 页码标签（根据 data-page 属性）。 */
export function injectPdfPageNumbers(containerId: string): void {
  const container = document.getElementById(containerId);
  if (!container) return;
  const existing = container.querySelectorAll(".page-num-label");
  if (existing.length > 0) return;
  const pages = container.children;
  for (let i = 0; i < pages.length; i++) {
    const pageDiv = pages[i] as HTMLElement;
    if (!pageDiv) continue;
    const pageNum = pageDiv.dataset.page;
    if (!pageNum) continue;
    const label = document.createElement("div");
    label.className = "page-num-label";
    label.textContent = pageNum;
    pageDiv.style.position = "relative";
    pageDiv.appendChild(label);
  }
}

/** 注入 Typst 页码标签（按 .typst-page 顺序编号）。 */
export function injectTypstPageNumbers(containerId: string): void {
  const container = document.getElementById(containerId);
  if (!container) return;
  const existing = container.querySelectorAll(".page-num-label");
  if (existing.length > 0) return;
  const pages = container.querySelectorAll(".typst-page");
  for (let i = 0; i < pages.length; i++) {
    const label = document.createElement("div");
    label.className = "page-num-label";
    label.textContent = String(i + 1);
    (pages[i] as HTMLElement).style.position = "relative";
    pages[i]!.appendChild(label);
  }
}

/** 显示页码标签。
 *
 * 标签水平位置：纸张宽 ≥ 视口宽时对齐**视口**中心（纸张中心可能在屏外）；
 * 纸张完整可见（宽 < 视口）时对齐**纸张**中心（视口居中会让页码落到纸外）。
 * 触摸/滚动/缩放事件都会重新调用本函数，显示期间持续跟随。
 */
export function showPageNumbers(): void {
  const labels = document.querySelectorAll<HTMLElement>(".page-num-label");
  const vw = window.innerWidth;
  for (const label of labels) {
    label.classList.remove("page-num-hidden");
    const page = label.parentElement;
    if (!page) continue;
    const pr = page.getBoundingClientRect();
    const targetX = pr.width >= vw ? vw / 2 : pr.left + pr.width / 2;
    label.style.left = `${targetX - pr.left}px`;
  }
}

/** 隐藏页码标签。 */
export function hidePageNumbers(): void {
  const labels = document.querySelectorAll(".page-num-label");
  for (let i = 0; i < labels.length; i++) {
    labels[i]!.classList.add("page-num-hidden");
  }
}
