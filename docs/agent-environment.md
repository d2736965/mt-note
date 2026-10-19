# Agent 环境准备

本仓库自带 agent 工作所需的大部分资产（技能、MCP 配置、工作流规范）。克隆后按本文档准备外部工具链即可开始工作。

## 仓库自带资产

| 资产 | 位置 | 生效方式 |
|---|---|---|
| 工作流规范 | `AGENTS.md` | agent 工具打开仓库自动加载 |
| Agent 技能 ×12（`mt-note-` 前缀） | `.agents/skills/` | agent 工具自动扫描项目级目录，按各自约定读取 |
| MCP 配置 | `.agents/mcp.json` | agent 工具按各自约定自动连接 |
| Lint 体系 | `lints/` + 根 `clippy.toml` | `python lints/scripts/check.py` |
| 排查手册 | `docs/troubleshooting.md` | 动手排障前先查 |

MCP 两项均为 `chrome-devtools-mcp`：

- **`cdp-local`**（launch 模式）：本机 Chromium 系浏览器自动化。默认配置未指定浏览器，MCP 自动探测 Chrome；如需指定浏览器或保留登录态，参考 `.agents/skills/mt-note-cdp/SKILL.md` 追加 `--executablePath` 与 `--user-data-dir`。
- **`cdp-remote`**（connect 模式）：真机 WebView 调试入口，连 `127.0.0.1:9228`，依赖下文「CDP 调试链路」先建立。

## 前置工具链

除下表常规工具链外，本项目有两项特殊前置：**构建前置**为补丁版 dioxus-cli（官方 CLI 无法构建本项目），**调试前置**为 cdp-proxy（CDP 多路代理），安装见下文「外部项目依赖」。

| 工具 | 版本基线 | 说明 |
|---|---|---|
| Rust（rustup） | stable + target `aarch64-linux-android` | 其余 android target（armv7/i686/x86_64）按需添加 |
| dioxus-cli (`dx`) | 0.7.10 | 安装见下文「外部项目依赖」（官方 CLI 缺 res_dir 支持，无法构建本项目）；已知问题 #5719 见 `.agents/skills/mt-note-dioxus-cli-build-tool` |
| Android SDK + NDK | NDK 27.1.12297006 + platform-tools（adb） | Android Studio 或 cmdline-tools 安装 |
| Python | 3.x | 运行 `lints/scripts/check.py` |
| Node + pnpm | node 22 / pnpm 10+ | 构建 `crates/mobile/ts`（vite/oxlint/tsgo） |
| CDP 多路代理 | cdp-proxy 或任意等价代理 | 见「CDP 调试链路」 |
| 真机 | Android 8+（min_sdk 26），开启 USB 调试 | — |

### NDK 工具链

根 `.cargo/config.toml` 留空；`crates/mobile/.cargo/config.toml` 的四个 android target linker/ar 配置**默认整段注释**（仅作提示，绕过 dx linker wrapper，见 DioxusLabs/dioxus#5118），避免覆盖开发者的全局 `~/.cargo/config.toml`。三选一：

1. **全局配置已就绪**：`~/.cargo/config.toml` 已声明绝对路径 linker，无需任何改动
2. **取消注释**：把模板里的裸名/路径改成自己 NDK 的实际路径
3. **全局 `~/.cargo/config.toml` 配 linker**，例如：

```toml
[target.aarch64-linux-android]
linker = "<NDK>/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android28-clang"
ar = "<NDK>/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar"
```

什么都不配时 rustc 回退宿主 `cc`/`ld`，典型报错 `unable to find library -llog`。Termux 侧 NDK prebuilt 目录名为 `linux-aarch64`。

## 外部项目依赖（两项）

| 项目 | 用途 | 获取 |
|---|---|---|
| [cdp-proxy](https://github.com/d2736965/cdp-proxy) | CDP 多路代理（WebView devtools 单连接 → 多客户端） | `pnpm add -g github:d2736965/cdp-proxy`；或任意等价 CDP 代理 |
| [dioxus-fork](https://github.com/d2736965/dioxus-fork) 补丁分支 `d2736965-patch-v0.7` | dx 补丁（Android 自定义 res_dir——**必需**，manifest 引用的自定义资源由它供给，官方 CLI 无法构建本项目；Termux NDK 路径、腾讯 gradle 镜像、移除 Google Fonts） | `cargo install --git https://github.com/d2736965/dioxus-fork --branch d2736965-patch-v0.7 dioxus-cli` |

## CDP 调试链路（真机 WebView）

```
chrome-devtools-mcp（cdp-remote，connect 模式）
        │  http://127.0.0.1:9228
        ▼
   CDP 多路代理（9228 监听；推荐 cdp-proxy，任意等价代理皆可）
        │  tcp:9223
        ▼
   adb forward tcp:9223 localabstract:webview_devtools_remote_<App PID>
        ▼
   Android WebView（仅 debug 构建开启 CDP）
```

逐条操作命令见 `AGENTS.md`「5. WebView 调试」。代理的作用：WebView devtools socket 只支持单连接，多路代理才能让 MCP 与 Edge inspect（`edge://inspect` → `localhost:9228`）同时连。

## 测试文件约定

推送到设备的临时测试文件（测试用 md、样张等）放在设备上的专用 tmp 目录（如 `/sdcard/Download/mt-note-tests/`），测试完删除。不要放进用户的笔记同步目录，避免产生同步噪音污染笔记库。
