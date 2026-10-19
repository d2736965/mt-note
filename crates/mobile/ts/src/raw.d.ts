/** vite `?raw` 导入：文件内容作为字符串（i18n 嵌入 .ftl 用）。 */
declare module "*?raw" {
  const content: string;
  export default content;
}

/**
 * vite 的静态 glob 导入（构建期展开为各匹配模块的导入，零运行时开销）。
 * tsconfig 的 types 为空（无 vite/client），此处按本项目用到的子集声明；
 * 默认返回类型对应 ?raw 场景（键为相对路径，值为文件内容）。
 */
interface ImportMetaGlobOptions {
  /** 同步收集全部匹配（返回 Record 而非惰性 import 函数）。 */
  eager?: boolean;
  /** 追加查询（如 "?raw" 取文件原文）。 */
  query?: string;
  /** 指定取匹配模块的哪个导出（"default" 即默认导出）。 */
  import?: string;
}

interface ImportMeta {
  glob<T = Record<string, string>>(
    pattern: string,
    options?: ImportMetaGlobOptions,
  ): T;
}
