/**
 * TS 侧取词：@fluent/bundle + 与 Rust 共用的 Fluent 翻译文件。
 *
 * 真源是 `crates/mobile/locales/*.ftl`（与 Rust 侧 dioxus-i18n 嵌入的是同一份
 * 文件，经 vite `import.meta.glob` + `?raw` 在构建期全部嵌入——放入新语言文件
 * 即自动生效）；语言由 Rust 侧经 `set_locale` 下发（已解析为已注册标签），
 * 切换即重建 FluentBundle（消息量小，毫秒级）。
 * 取词失败回退键名本身（与 Rust 侧 translate 回退策略一致）。
 */
import { FluentBundle, FluentResource } from "@fluent/bundle";

/** 回退基准语言，与 Rust 侧约定一致（mobile/build.rs 门禁保证 en.ftl 存在）。 */
const FALLBACK_LOCALE = "en";

const sources = import.meta.glob("../../locales/*.ftl", {
  eager: true,
  query: "?raw",
  import: "default",
});

function sourceOf(locale: string): string {
  const path = `../../locales/${locale}.ftl`;
  return (
    sources[path] ?? sources[`../../locales/${FALLBACK_LOCALE}.ftl`] ?? path
  );
}

function buildBundle(locale: string): FluentBundle {
  // useIsolating: false——Fluent 默认在插值两侧加 Unicode 隔离符（不可见
  // 控制字符），错误串插路径时显示异常；界面插值场景无需隔离。
  const bundle = new FluentBundle(locale, { useIsolating: false });
  bundle.addResource(new FluentResource(sourceOf(locale)));
  return bundle;
}

let bundle = buildBundle(FALLBACK_LOCALE);

/** 切换界面语言（Rust 侧经 js-bridge-bindgen 调用）。未知语言回退基准语言。 */
export function setLocale(value: string): void {
  bundle = buildBundle(value);
}

/** 取词。缺键或格式化出错回退键名。 */
export function tr(
  key: string,
  args?: Record<string, string | number>,
): string {
  const message = bundle.getMessage(key);
  if (!message?.value) return key;
  const errors: Error[] = [];
  const text = bundle.formatPattern(message.value, args, errors);
  return errors.length > 0 ? key : text;
}

/** i18n 工厂接口：js-bridge-bindgen 据此生成 Rust 绑定。 */
export interface I18n {
  setLocale: (value: string) => void;
  /** @rust-lint expect(dead_code, reason = "由 Rust 侧调用") */
  destroy: () => void;
}

export function createI18n(): I18n {
  return {
    setLocale,
    destroy: () => {
      bundle = buildBundle(FALLBACK_LOCALE);
    },
  };
}
