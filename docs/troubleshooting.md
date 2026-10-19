# 常见问题排查（构建 / 安装 / 调试）

遇到构建、安装、真机调试异常时，先对照本文档，再动手改代码。

## 构建问题

### rustc ICE（compiler unexpectedly panicked）

`dx bundle`（Android target）偶发内部编译器崩溃，query stack 多为 `shallow_lint_levels_on` / `skippable_lints`，且指向与本次改动无关的模块。这是**增量编译缓存损坏，不是代码问题**——同一代码用 `python lints/scripts/check.py` 可正常编译。清缓存重建即可（全量重编约 1 分钟）：

```bash
rm -rf target/aarch64-linux-android/android-dev/incremental
dx bundle -p mt-note-mobile --android --target aarch64-linux-android --package-types apk
```

注意：dx wrapper 会把 rustc 报错降级成 WARN，容易被误导去"修"根本没问题的代码。

### dx bundle 失败后可能装上旧 APK

`dx bundle ... && adb install ...` 串联执行时，构建失败可能被吞掉而安装了旧 APK。安装前核对构建输出确为 success。

## 安装问题

### MIUI/小米：INSTALL_FAILED_USER_RESTRICTED

设备对 adb 安装会弹确认框，首次可能报此错误——在手机上点"允许"后**重试即可成功**。

### debug / release 签名互斥

两者签名不同，换装必须先卸载（会清空应用数据）：

```bash
adb uninstall com.d2736965.app.mt_note_mobile
```

release 构建签名用 `scripts/build_release_apk.py`。仓库**不含** release 签名密钥，贡献者需自建 keystore，构建时用 `-k` / `-a` 传入 keystore 路径与 alias。

## 调试问题

### WebView CDP 只在 debug 构建可用

release 装包没有 CDP，排查前端渲染问题必须安装 debug APK（构建命令见 `AGENTS.md`「2. 编译」，不带 `-r` 即 debug）。

### cdp-proxy 截图 / evaluate 偶发超时

退回系统截屏更可靠：

```bash
adb exec-out screencap -p > screen.png
```

### 应用内导航调试

应用内页面切换可用 cdp-remote 的 take_snapshot + click 逐级点目录，无需手动操作手机。

## 样式问题

### stylance：报 `constant ... is never used`，或 TS 切换的类样式静默失效

模块 SCSS 中只被 TS 用字符串 add/remove class 切换（Rust 不引用）的类，选择器必须写 `:global(.xxx)`（嵌套时 `&:global(.xxx)`），如 `.md-lightbox-toolbar--hidden`。

原因：stylance 对普通类生成带哈希的 CSS 类名 + 同名 Rust 常量，双重后果：

1. Rust 侧没人使用该常量 → `-D warnings` 下 dead_code 直接构建失败（`check.py` 可见）；
2. 即使构建通过，TS 写的原始类名匹配不上哈希后的选择器，样式静默失效。

报错时先查是不是这类选择器漏了 `:global`，**不要去 Rust 侧加 allow**。
