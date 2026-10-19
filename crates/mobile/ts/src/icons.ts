/**
 * TS 侧图标入口：path 数据由 mobile/build.rs 从 ui 包 icons 模块提取生成
 * （icons.gen.ts，单一真源在 Rust 的 icons! 宏），本模块只负责包上 <svg> 壳。
 * 与 Rust 侧 IconSvg 组件同构：viewBox 0 0 24 24、currentColor 填充，尺寸由
 * 使用方 CSS 控制。禁止再手写内联 <svg> 图标——新图标先加进 icons! 宏。
 */
import { ICON_PATHS, type IconName } from "./icons.gen";

/** 生成可直接赋给 innerHTML 的图标 <svg> 字符串。 */
export function iconSvg(name: IconName): string {
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="${ICON_PATHS[name]}"/></svg>`;
}
