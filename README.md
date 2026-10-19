# mt-note

基于 [Dioxus 0.7](https://dioxuslabs.com) 的 **Android 笔记应用**，支持 Markdown / Typst 编辑与排版预览。

## 功能

- **文件浏览**：目录遍历（遵循 `.gitignore`）、文件夹收藏、系统分享文件直接打开
- **编辑器**：代码编辑 + 语义高亮、光标信息面板、日期时间插入
- **预览**：Typst 实时预览、SVG / PDF 文件预览，支持捏合缩放与多档吸附
- **文件变更响应**：文件系统变更自动感知，列表自动刷新、编辑器弹出变更提示
- **主题**：米黄护眼纸色 + 暗色模式，系统栏 edge-to-edge 适配
- **Android 原生体验**：返回拦截、全部文件访问权限引导、分享文件待处理队列

## 构建

构建前置：安装补丁版 dioxus-cli（安装后命令为 `dx`；官方 CLI 无法构建本项目——manifest 引用的自定义资源由它的 res_dir 支持供给）：

```bash
cargo install --git https://github.com/d2736965/dioxus-fork --branch d2736965-patch-v0.7 dioxus-cli
```

```bash
# 开发热重载
dx serve -p mt-note-mobile

# Debug APK（arm64）
dx bundle -p mt-note-mobile --android --target aarch64-linux-android --package-types apk

# Release 签名 APK（构建 + apksigner 签名，输出 tmp/apk/）
python scripts/build_release_apk.py -p <密码> -k <keystore路径> -a <alias>

# 仅签名已有 APK
python scripts/build_release_apk.py -p <密码> -k <keystore路径> -a <alias> --sign-only <apk路径>
```

## 许可证

本项目以 `MIT OR Apache-2.0` 双授权发布（[LICENSE-MIT](LICENSE-MIT) / [LICENSE-APACHE](LICENSE-APACHE)），任选其一遵守。
