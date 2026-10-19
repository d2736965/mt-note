// Markdown 渲染所需的第三方库改为 npm 包直接 import（类型安全，随 bundle 注入）。
// highlight.js 按需注册常用语言，控制体积；mermaid 模块加载时初始化。
import { tr } from "./i18n";

import hljs from "highlight.js/lib/core";
import rust from "highlight.js/lib/languages/rust";
import javascript from "highlight.js/lib/languages/javascript";
import typescript from "highlight.js/lib/languages/typescript";
import python from "highlight.js/lib/languages/python";
import java from "highlight.js/lib/languages/java";
import cpp from "highlight.js/lib/languages/cpp";
import go from "highlight.js/lib/languages/go";
import sql from "highlight.js/lib/languages/sql";
import bash from "highlight.js/lib/languages/bash";
import json from "highlight.js/lib/languages/json";
import yaml from "highlight.js/lib/languages/yaml";
import ini from "highlight.js/lib/languages/ini";
import markdownLang from "highlight.js/lib/languages/markdown";
import xml from "highlight.js/lib/languages/xml";
import scss from "highlight.js/lib/languages/scss";
import kotlin from "highlight.js/lib/languages/kotlin";
import swift from "highlight.js/lib/languages/swift";
import dart from "highlight.js/lib/languages/dart";
import renderMathInElement from "katex/contrib/auto-render";
import mermaid, { type MermaidConfig } from "mermaid";

import { openLightbox } from "./image-lightbox";
import { iconSvg } from "./icons";
import type { AppWindow } from "./window-types";

// 按需注册 highlight.js 语言（xml 含 html 别名，bash 含 shell 别名）
hljs.registerLanguage("rust", rust);
hljs.registerLanguage("javascript", javascript);
hljs.registerLanguage("typescript", typescript);
hljs.registerLanguage("python", python);
hljs.registerLanguage("java", java);
hljs.registerLanguage("cpp", cpp);
hljs.registerLanguage("go", go);
hljs.registerLanguage("sql", sql);
hljs.registerLanguage("bash", bash);
hljs.registerLanguage("json", json);
hljs.registerLanguage("yaml", yaml);
hljs.registerLanguage("ini", ini);
// highlight.js 11 无独立 toml，复用 ini 规则（toml 与 ini 语法兼容）
hljs.registerLanguage("toml", ini);
hljs.registerLanguage("markdown", markdownLang);
hljs.registerLanguage("xml", xml);
hljs.registerLanguage("scss", scss);
hljs.registerLanguage("kotlin", kotlin);
hljs.registerLanguage("swift", swift);
hljs.registerLanguage("dart", dart);

// 图表按自然尺寸渲染：mermaid 默认（useMaxWidth: true）把 svg 压进容器宽度，
// 手机上宽图会整体缩小到文字不可读；禁用后超宽图由 .mermaid 容器横向滚动查看。
// syncMermaidTheme 会整体重设 config，两个 initialize 都必须带上这份配置
const mermaidNaturalSize: Partial<MermaidConfig> = {
  flowchart: { useMaxWidth: false },
  sequence: { useMaxWidth: false },
  gantt: { useMaxWidth: false },
  class: { useMaxWidth: false },
  state: { useMaxWidth: false },
  er: { useMaxWidth: false },
  journey: { useMaxWidth: false },
  pie: { useMaxWidth: false },
  requirement: { useMaxWidth: false },
  mindmap: { useMaxWidth: false },
  timeline: { useMaxWidth: false },
  gitGraph: { useMaxWidth: false },
};

// 初始化 mermaid（模块加载时执行一次）
mermaid.initialize({
  startOnLoad: false,
  theme: "default",
  securityLevel: "loose",
  ...mermaidNaturalSize,
});

/** 根据当前页面主题重新初始化 mermaid 主题。 */
function syncMermaidTheme(): void {
  const isDark = document.querySelector('[class*="theme-dark"]') !== null;
  mermaid.initialize({
    startOnLoad: false,
    theme: isDark ? "dark" : "default",
    securityLevel: "loose",
    ...mermaidNaturalSize,
    themeCSS: isDark
      ? "text.actor { fill: #ccc !important; }" +
        "text.actor > tspan { fill: #ccc !important; }" +
        '[class*="branch-label"] { fill: #ccc !important; }' +
        ".commit-label { fill: #ccc !important; }" +
        ".gitTitleText { fill: #ccc !important; }"
      : "",
  });
}

/** 渲染 Markdown 内容：代码高亮、KaTeX 数学公式、Mermaid 图表、代码块复制。 */
export function renderHighlightAndKatex(mdId: string): void {
  const copySvg = iconSvg("ContentCopy");
  const checkSvg = iconSvg("CheckMark");

  function extractCSV(table: HTMLTableElement): string {
    const rows: string[] = [];
    table.querySelectorAll("tr").forEach((tr) => {
      const cells: string[] = [];
      tr.querySelectorAll("th, td").forEach((cell) => {
        const text = cell.textContent!.replace(/"/g, '""');
        cells.push(`"${text}"`);
      });
      rows.push(cells.join(","));
    });
    return rows.join("\n");
  }

  function processElement(): void {
    const el = document.getElementById(mdId);
    if (!el) {
      requestAnimationFrame(processElement);
      return;
    }

    if (!el.dataset.anchorSetup) {
      function scrollToTarget(target: HTMLElement): void {
        let container: HTMLElement | null = target.parentElement;
        while (container) {
          const style = getComputedStyle(container);
          if (
            (style.overflowY === "auto" || style.overflowY === "scroll") &&
            container.scrollHeight > container.clientHeight
          ) {
            break;
          }
          container = container.parentElement;
        }
        if (container) {
          const targetRect = target.getBoundingClientRect();
          const containerRect = container.getBoundingClientRect();
          container.scrollTo({
            top: container.scrollTop + targetRect.top - containerRect.top,
            behavior: "instant" as ScrollBehavior,
          });
        } else {
          target.scrollIntoView({
            behavior: "instant" as ScrollBehavior,
            block: "start",
          });
        }
      }

      el.addEventListener("click", (e: Event) => {
        const anchorLink = (e.target as HTMLElement).closest('a[href^="#"]');
        if (anchorLink) {
          e.preventDefault();
          const targetId = decodeURIComponent(
            (anchorLink as HTMLAnchorElement).getAttribute("href")!.slice(1),
          );
          const target = el.querySelector(
            `#${CSS.escape(targetId)}`,
          ) as HTMLElement | null;
          if (target) {
            scrollToTarget(target);
          }
          return;
        }

        const localLink = (e.target as HTMLElement).closest(
          'a:not([href^="#"])',
        );
        if (localLink) {
          const href = (localLink as HTMLAnchorElement).getAttribute("href");
          if (
            href &&
            !href.startsWith("http://") &&
            !href.startsWith("https://")
          ) {
            e.preventDefault();
            e.stopPropagation();
            let decoded = href;
            try {
              decoded = decodeURIComponent(href);
            } catch {
              /* ignore */
            }
            let base = el.dataset.mdBasePath || "";
            try {
              base = decodeURIComponent(base);
            } catch {
              /* ignore */
            }
            // 保存滚动位置（含文件路径，返回后仅同路径恢复）
            const scrollContainer = el.parentElement;
            if (scrollContainer) {
              const max =
                scrollContainer.scrollHeight - scrollContainer.clientHeight;
              (window as unknown as AppWindow).__savedMdScroll =
                max > 0 ? scrollContainer.scrollTop / max : 0;
              (window as unknown as AppWindow).__savedMdScrollPath =
                el.dataset.mdBasePath || "";
            }
            window.dispatchEvent(
              new CustomEvent("link-click", {
                detail: { href: decoded, base },
              }),
            );
          }
        }

        // 图形灯箱：图片与内联 SVG（mermaid 图表、内嵌 svg 等）全屏预览；
        // 链接/按钮内的图形保持原有语义（复制按钮的图标 svg 不被误触发）
        const graphic = (e.target as Element).closest("img, svg") as
          HTMLImageElement | SVGSVGElement | null;
        if (graphic && !graphic.closest("a, button")) {
          e.preventDefault();
          if (graphic instanceof HTMLImageElement) {
            openLightbox(graphic.src, graphic.alt);
          } else {
            openLightbox(graphic, graphic.getAttribute("aria-label") ?? "");
          }
          return;
        }

        const heading = (e.target as HTMLElement).closest(
          "h1, h2, h3, h4, h5, h6",
        ) as HTMLElement | null;
        if (heading && heading.id) {
          scrollToTarget(heading);
        }
      });
      el.dataset.anchorSetup = "1";
    }

    function doRender(): void {
      // 裸 HTML <img> 的相对 src 不经 Rust 侧重写（pulldown-cmark 只重写
      // markdown 图片语法事件，HTML 标签原样透传），在此按 data-md-base-path
      // 解析为 /local-file/ 绝对路径，口径与 Rust resolve_image_url 一致；
      // 已重写的（以 / 开头）与网络/data 地址跳过，重复执行幂等
      el!.querySelectorAll("img[src]").forEach((img: Element) => {
        const imgEl = img as HTMLImageElement;
        const raw = imgEl.getAttribute("src") ?? "";
        const base = el!.dataset.mdBasePath || "";
        if (
          !raw ||
          !base ||
          raw.startsWith("/") ||
          raw.startsWith("http://") ||
          raw.startsWith("https://") ||
          raw.startsWith("data:")
        )
          return;
        const dir = base.slice(0, base.lastIndexOf("/")); // 去掉文件名留目录
        const joined = raw.startsWith("/") ? raw : `${dir}/${raw}`;
        imgEl.setAttribute("src", `/local-file/${joined.replace(/^\/+/, "")}`);
      });

      // 代码块卡片化：wrapper = 顶栏（语言名 + 复制按钮）+ pre，结构与表格
      // 卡片（md-table-bar + md-table-scroll）一致。按钮放顶栏而非悬浮在
      // 代码上，不遮挡任何代码文字；语言名取自 hljs 的 language-xxx 类
      el!.querySelectorAll("pre code").forEach((block: Element) => {
        hljs.highlightElement(block as HTMLElement);
        const pre = block.parentElement as HTMLElement;
        let wrapper = pre.parentElement;
        if (!wrapper?.classList.contains("md-code-wrapper")) {
          wrapper = document.createElement("div");
          wrapper.className = "md-code-wrapper";
          pre.parentNode!.insertBefore(wrapper, pre);
          wrapper.appendChild(pre);
        }
        if (!wrapper.querySelector(".md-code-bar")) {
          const langMatch = /language-([\w#+.-]+)/.exec(block.className);
          const bar = document.createElement("div");
          bar.className = "md-code-bar";
          bar.innerHTML =
            '<span class="md-code-lang"></span>' +
            '<div class="md-code-actions">' +
            `<button class="md-copy-btn" title="${tr("md-copy")}">` +
            copySvg +
            "</button>" +
            "</div>";
          bar.querySelector(".md-code-lang")!.textContent =
            langMatch?.[1] ?? tr("md-code");
          wrapper.insertBefore(bar, pre);
          const btn = bar.querySelector(".md-copy-btn") as HTMLButtonElement;
          btn.addEventListener("click", (ev: Event) => {
            ev.stopPropagation();
            const text = block.textContent!;
            const doCopy = () => {
              btn.innerHTML = checkSvg;
              btn.title = tr("md-copied");
              setTimeout(() => {
                btn.innerHTML = copySvg;
                btn.title = tr("md-copy");
              }, 1500);
            };
            if (navigator.clipboard && navigator.clipboard.writeText) {
              navigator.clipboard.writeText(text).then(doCopy);
            } else {
              const ta = document.createElement("textarea");
              ta.value = text;
              ta.style.cssText = "position:fixed;left:-9999px";
              document.body.appendChild(ta);
              ta.select();
              document.execCommand("copy");
              document.body.removeChild(ta);
              doCopy();
            }
          });
        }
      });

      el!.querySelectorAll("table").forEach((table: HTMLTableElement) => {
        if (table.parentElement!.classList.contains("md-table-scroll")) return;

        const csv = extractCSV(table);

        const wrapper = document.createElement("div");
        wrapper.className = "md-table-wrapper";

        const bar = document.createElement("div");
        bar.className = "md-table-bar";
        bar.innerHTML =
          `<span class="md-table-bar-title">${tr("md-table")}</span>` +
          '<div class="md-table-bar-actions">' +
          `<button class="md-table-copy" title="${tr("md-copy")}">` +
          copySvg +
          "</button>" +
          "</div>";

        const scroll = document.createElement("div");
        scroll.className = "md-table-scroll";

        table.parentNode!.insertBefore(wrapper, table);
        wrapper.appendChild(bar);
        wrapper.appendChild(scroll);
        scroll.appendChild(table);

        const columnWordNumLimitDefault = 9;
        const cellBorderLeft = 1;
        table.style.borderCollapse = "separate";
        table.style.borderSpacing = "0";
        table.querySelectorAll("th, td").forEach((c: Element) => {
          (c as HTMLElement).style.borderTop =
            "1px solid var(--md-outline-variant)";
          (c as HTMLElement).style.borderLeft =
            `${cellBorderLeft}px solid var(--md-outline-variant)`;
          (c as HTMLElement).style.borderBottom = "none";
          (c as HTMLElement).style.borderRight = "none";
        });
        const containerW = scroll.clientWidth;
        const allCells = table.querySelectorAll("th, td");
        allCells.forEach((c: Element) => {
          (c as HTMLElement).style.whiteSpace = "nowrap";
        });
        const naturalW = table.getBoundingClientRect().width;
        allCells.forEach((c: Element) => {
          (c as HTMLElement).style.whiteSpace = "";
        });
        if (naturalW <= containerW) {
          table.style.width = "100%";
        } else {
          table.style.tableLayout = "fixed";
          const firstCell = table.querySelector("th, td");
          const cellPaddingH = firstCell
            ? parseFloat(getComputedStyle(firstCell).paddingLeft) +
              parseFloat(getComputedStyle(firstCell).paddingRight)
            : 0;
          const cellExtraH = cellPaddingH + cellBorderLeft;
          const measureSpan = document.createElement("span");
          measureSpan.style.position = "absolute";
          measureSpan.style.visibility = "hidden";
          measureSpan.style.whiteSpace = "nowrap";
          measureSpan.style.font = getComputedStyle(table).font;
          document.body.appendChild(measureSpan);
          function measureTextWidth(text: string): number {
            measureSpan.textContent = text;
            return measureSpan.getBoundingClientRect().width;
          }
          const firstRow = table.querySelector("tr");
          const colCount = firstRow ? firstRow.children.length : 0;
          const colWordNums: number[] = [];
          const colTextWidths: number[] = [];
          for (let ci = 0; ci < colCount; ci++) {
            let maxWordNum = 0;
            let maxTextWidth = 0;
            table.querySelectorAll("tr").forEach((row) => {
              const cell = row.children[ci] as HTMLElement | undefined;
              if (cell) {
                const text = cell.textContent!.trim();
                if (text.length > maxWordNum) {
                  maxWordNum = text.length;
                  maxTextWidth = measureTextWidth(text);
                } else if (text.length === maxWordNum) {
                  const w = measureTextWidth(text);
                  if (w > maxTextWidth) maxTextWidth = w;
                }
              }
            });
            colWordNums.push(maxWordNum);
            colTextWidths.push(maxTextWidth);
          }
          const limitTextWidth =
            Math.ceil(
              measureTextWidth("国".repeat(columnWordNumLimitDefault)),
            ) + 1;
          document.body.removeChild(measureSpan);
          const colMinPxWidths: number[] = [];
          for (let ci = 0; ci < colCount; ci++) {
            if (colWordNums[ci]! <= columnWordNumLimitDefault) {
              colMinPxWidths[ci] = colTextWidths[ci]! + cellExtraH;
            } else {
              colMinPxWidths[ci] = limitTextWidth + cellExtraH;
            }
          }
          const totalMinPx = colMinPxWidths.reduce((a, b) => a + b, 0);
          table.style.width = `${totalMinPx}px`;
          for (let ci = 0; ci < colCount; ci++) {
            const colWidthPercent = (colMinPxWidths[ci]! / totalMinPx) * 100;
            table.querySelectorAll("tr").forEach((row) => {
              const cell = row.children[ci] as HTMLElement | undefined;
              if (cell) {
                cell.style.width = `${colWidthPercent}%`;
                if (colWordNums[ci]! > columnWordNumLimitDefault) {
                  cell.style.whiteSpace = "normal";
                  cell.style.overflowWrap = "break-word";
                  cell.style.wordBreak = "break-word";
                } else {
                  cell.style.whiteSpace = "nowrap";
                }
              }
            });
          }
        }
        const rows = table.querySelectorAll("tr");
        rows.forEach((row, ri) => {
          if (ri === rows.length - 1) {
            row.querySelectorAll("th, td").forEach((c: Element) => {
              (c as HTMLElement).style.borderBottom =
                "1px solid var(--md-outline-variant)";
            });
          }
          const cells = row.querySelectorAll("th, td");
          if (cells.length > 0) {
            (cells[cells.length - 1] as HTMLElement).style.borderRight =
              "1px solid var(--md-outline-variant)";
          }
        });

        const csvData = csv;
        bar.querySelector(".md-table-copy")!.addEventListener("click", () => {
          const btn = bar.querySelector(".md-table-copy") as HTMLButtonElement;
          if (navigator.clipboard && navigator.clipboard.writeText) {
            navigator.clipboard.writeText(csvData).then(() => {
              btn.innerHTML = checkSvg;
              setTimeout(() => {
                btn.innerHTML = copySvg;
              }, 1500);
            });
          }
        });
      });

      // 数学公式：auto-render 直接调用
      renderMathInElement(el!, {
        delimiters: [
          { left: "$$", right: "$$", display: true },
          { left: "$", right: "$", display: false },
        ],
        throwOnError: false,
      });
      el!.querySelectorAll(".katex-display").forEach((displayEl: Element) => {
        if (displayEl.parentElement!.classList.contains("math-scroll-wrapper"))
          return;
        const wrapper = document.createElement("div");
        wrapper.className = "math-scroll-wrapper";
        displayEl.parentNode!.insertBefore(wrapper, displayEl);
        wrapper.appendChild(displayEl);
      });
      el!.querySelectorAll(".katex").forEach((katexEl: Element) => {
        if (katexEl.classList.contains("katex-display")) return;
        const block = katexEl.closest(
          "p, li, dd, blockquote, td, th",
        ) as HTMLElement | null;
        if (block && block.scrollWidth > block.clientWidth) {
          block.style.overflowX = "auto";
          block.style.overscrollBehaviorX = "none";
        }
      });

      // 内联 SVG 滚动容器：无 viewBox 的 SVG 无法等比缩放（CSS 压缩宽度只会把
      // 内容裁掉），实际溢出的保持原始宽度包进横向滚动容器（同 .md-table-scroll）。
      // 按钮图标、KaTeX、mermaid、链接内图形及未溢出的行内小图标一律不动；
      // 仅包裹实际溢出者，避免改变段落内行内图形的排布
      el!.querySelectorAll("svg").forEach((svg: Element) => {
        const svgEl = svg as SVGSVGElement;
        if (svgEl.ownerSVGElement) return; // 嵌套 svg 随外层一起处理
        if (svgEl.closest(".md-graphic-scroll, .mermaid, .katex, a, button"))
          return;
        const parent = svgEl.parentElement;
        if (!parent) return;
        if (svgEl.getBoundingClientRect().width <= parent.clientWidth) return;
        const wrapper = document.createElement("div");
        wrapper.className = "md-graphic-scroll";
        parent.insertBefore(wrapper, svgEl);
        wrapper.appendChild(svgEl);
      });

      // mermaid 图表
      const mermaidNodes = Array.from(
        el!.querySelectorAll<HTMLElement>(".mermaid"),
      );
      if (mermaidNodes.length > 0) {
        syncMermaidTheme();
        mermaid.run({
          nodes: mermaidNodes,
          suppressErrors: true,
        });
      }
    }

    doRender();
  }

  processElement();
}
