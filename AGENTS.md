# mt-note Agent 工作流规范

## 规则技能加载规则

- 本项目是 Dioxus workspace（构建走 `dx` 命令），命中的规则技能为 **`mt-note-dioxus-workspace-rules`**（技能随项目自带于 `.agents/skills/`，带 `mt-note-` 前缀）。只要适用就加载，保证规则技能在上下文中完整、按需加载；手头工作涉及构建/调试/样式/actor 等专项时，按需再加载对应配套技能（`mt-note-dioxus-android`、`mt-note-cdp` 等）
- **压缩后立即重载（先做后想）**：感知到上下文压缩（compaction/summarization）完成时，本轮的第一个工具调用就必须是 Skill 加载 `mt-note-dioxus-workspace-rules`，加载完成之前禁止展开任何任务思考；禁止凭压缩摘要残存记忆开工——摘要≠规范全文，残缺记忆继续干活必然违反硬性规则
- **操作前验证**：执行任何代码操作（编写、修改、审查、构建）前，检查上下文中是否存在 `mt-note-dioxus-workspace-rules` 的 `## 检查清单` 章节；不存在则立即重新加载该技能
- **Rust 硬性规则**：禁止 `cargo check`，一律用 lint 检查替代（`python lints/scripts/check.py`，即 clippy + dylint 双检）

## 终端复用规范

- 执行命令时尽量复用已有终端，最多同时开启 2 个选项卡

## SCSS 编码规范

- 强制使用 SCSS 嵌套语法，子选择器嵌套在父选择器内部，禁止平级书写
- 嵌套结构强制反映 DOM 层级关系，例如 `<div class="outer"><div class="inner"></div></div>` 对应的 SCSS 强制是 `.outer { .inner { } }`，禁止写成 `.outer { } .inner { }` 平级形式
- 正确：
  ```scss
  .parent {
    color: red;
    .child { color: blue; }
  }
  ```
- 错误：
  ```scss
  .parent { color: red; }
  .child { color: blue; }
  ```
- 模块 SCSS 中**只被 TS 切换**（TS 用字符串 add/remove class，Rust 不引用）的类，选择器必须写 `:global(.xxx)`（嵌套时 `&:global(.xxx)`）。原因：stylance 对普通类生成带哈希的类名 + 同名 Rust 常量——Rust 侧无人用该常量会在 `-D warnings` 下构建失败，且 TS 写的原始类名匹配不上哈希后的选择器。报 `constant ... is never used` 或 TS 类名样式静默失效时先查这里，不要去 Rust 侧加 allow

## 国际化规范（强制，界面禁止裸字符串）

- **一切用户可见文案禁止硬编码**（RSX 文本、toast/菜单/对话框文案、TS 渲染的工具条提示等）：先在 `crates/mobile/locales/en.ftl` 与 `zh-Hans.ftl` **同步**加键（键集由 build.rs 门禁强制一致，只加一边构建失败），再用取词 API 渲染
- Rust 侧：组件渲染体 `let i18n_ctx = i18n();`（`dioxus_i18n::prelude`，Copy 可入闭包），取词走 `frontend/i18n` 的 `translate`（无参）/ `translate_with`（插值）/ `translate_err`（附错误详情），闭包内传 `&i18n_ctx`
- TS 侧：`ts/src/i18n.ts` 的 `tr(key)`；语言切换由 Rust 经 `js_api::i18n::set_locale` 下发
- 键名小写中划线、按域加前缀（`common-`/`settings-`/`editor-`/`err-`/`md-` 等）；需进 Android 原生资源（系统层文案，如分享 chooser 标题）的键加 `native-` 前缀（build.rs 自动生成 strings-gen.xml）
- 例外（不算裸字符串）：tracing 日志（保持中文）、底层 crate 错误 Display（英文，展示点经 `i18n/localize.rs` 的 `Localize` 翻译）、品牌名与专有名词
- 新增语言：放 `<BCP-47 完整标签>.ftl`（如 `zh-Hans.ftl`、`pt-BR.ftl`）即自动注册为语言选项，禁止裸 `zh` 这类不完整标签；文件内用 `language-self-name` 键自名，键集必须与 `en.ftl` 完全一致（`en` 是回退基准，不可删除）

## TS 代码检查流程（强制，修改 TS 后强制执行）

TS 代码修改后强制按顺序执行：format → lint → typecheck

```bash
cd crates/mobile/ts
pnpm format           # prettier 格式化
pnpm lint             # oxlint 检查
pnpm typecheck        # tsgo 类型检查
```

### 编译安装运行完整流程

### 0. 前置（两项特殊依赖）

| 前置 | 用途 | 获取 |
|---|---|---|
| **构建前置**：[补丁版 dioxus-cli](https://github.com/d2736965/dioxus-fork)（分支 `d2736965-patch-v0.7`：Android res_dir、Termux NDK、腾讯 gradle 镜像等补丁） | 安装后命令为 `dx`；manifest 引用的自定义资源由 res_dir 供给，官方 CLI 无法构建本项目 | `cargo install --git https://github.com/d2736965/dioxus-fork --branch d2736965-patch-v0.7 dioxus-cli` |
| **调试前置**：[cdp-proxy](https://github.com/d2736965/cdp-proxy)（CDP 多路代理） | WebView devtools 仅单连接，步骤 5 的多客户端调试依赖它 | `pnpm add -g github:d2736965/cdp-proxy`；或任意等价 CDP 多路代理 |

### 1. Lint 检查（强制 clippy + dylint，禁止 cargo check）
```bash
python lints/scripts/check.py               # 全工作区检查，自动检测 target
python lints/scripts/check.py -p mt-note-mobile  # 检查单个 crate
python lints/scripts/check.py --json        # JSON 输出，供 rust-analyzer 解析
```

### 2. 编译

在 `Dioxus.toml` 所在的 `crates/mobile` 目录执行：

```bash
dx bundle -p mt-note-mobile --android --target aarch64-linux-android --package-types apk
```

APK 输出路径（相对工作区根，dioxus 0.7.9 目录结构）：

```
target/dx/mt-note-mobile/debug/android/app/app/build/outputs/apk/debug/app-debug.apk
```

### 3. 安装
```bash
adb install -r target/dx/mt-note-mobile/debug/android/app/app/build/outputs/apk/debug/app-debug.apk
```

### 4. 启动
```bash
adb shell am start -n com.d2736965.app.mt_note_mobile/dev.dioxus.main.MainActivity
```

### 5. WebView 调试（CDP 代理，支持同时多客户端）

WebView devtools socket 只支持单连接，需要 CDP 代理才能让 Edge inspect 和 MCP 同时使用。

| 端口 | 用途 |
|------|------|
| 9223 | adb forward → WebView socket（代理后端） |
| 9228 | CDP 代理监听（cdp-remote MCP → 代理 → 9223 → WebView） |

#### 5a. 转发 WebView socket

**前置：App 强制已启动**（见步骤 4），否则 `pidof` 返回空、转发无效。socket 名为 `webview_devtools_remote_<App PID>`，PID 强制用包名查询。`/proc/net/unix` 中可能有多个同名 socket（其他 App、残留进程），grep 无法区分。

```bash
adb forward tcp:9223 localabstract:webview_devtools_remote_$(adb shell pidof com.d2736965.app.mt_note_mobile)
```

#### 5b. 启动 CDP 代理
```bash
cdp-proxy start          # 后台启动（安装：pnpm add -g github:d2736965/cdp-proxy；任意等价 CDP 多路代理亦可）
cdp-proxy status         # 查看状态（exit 0=运行中, exit 1=未运行）
cdp-proxy stop           # 停止
cdp-proxy restart        # 重启
```

#### 5c. 连接方式
- **cdp-remote MCP**：连 `http://127.0.0.1:9228`（通过代理，支持多人同时看）
- **Edge inspect**：`edge://inspect` → Configure → 添加 `localhost:9228`

#### 5d. AI 使用 cdp-remote MCP 前的强制检查
**使用 `cdp-remote` MCP 工具前，强制先确保 `cdp-proxy` 服务在运行，但是当前会话中打开过或者曾经验证过则不必再验证：**
```bash
cdp-proxy status          # 退出码 0 = 运行中
# 如果未运行（exit 1），先启动：
cdp-proxy start
```

#### 5e. CDP 调试的截图/快照/脚本选用
**能够识别图片的视觉模型可用 `take_screenshot` 直接查看页面渲染效果**；无法识别图片的模型/场景强制使用 `take_snapshot` 替代。
- `take_screenshot`：返回页面位图，视觉模型可据此判断布局、颜色与渲染效果（透明、遮挡、模糊等只有截图才能发现的问题）
- `take_snapshot`：返回可读的 DOM 树文本，适合分析结构、内容、层级与可达性
- 如需检查 CSS 计算样式、元素尺寸等，使用 `evaluate_script`（参数 `function` 传箭头函数，如 `() => document.title`）

## Git 命令规范

- 所有 `git` 命令强制加 `--no-pager` 参数，避免 pager 阻塞终端（如 `git --no-pager log`、`git --no-pager diff`、`git --no-pager status` 等）

## 组件目录组织规范

每个组件由一个目录构成，目录内至少包含 `mod.rs` 和 `xxx.module.scss`。如果组件内部还有子组件，组织到该组件目录内的 `components/` 子目录。

### 公共组件 vs 私有组件

两者都放在 `components/` 目录下，但**位置不同**：

| 类型 | 位置 | 示例 |
|------|------|------|
| **公共组件**（跨页面复用） | `crates/mobile/src/frontend/components/` | `crates/mobile/src/frontend/components/dir_picker/` |
| **私有组件**（仅某页面使用） | `crates/mobile/src/frontend/views/<view>/components/` | `crates/mobile/src/frontend/views/home/components/tab1/` |

### 目录结构示例

```
crates/mobile/src/frontend/
├── components/                    # 公共组件（跨页面复用）
│   ├── dir_picker/
│   │   ├── mod.rs
│   │   └── dir_picker.module.scss
│   └── top_app_bar/
│       ├── mod.rs
│       ├── top_app_bar.module.scss
│       └── components/            # 公共组件内部的子组件
│           └── top_app_bar_menu/
│               ├── mod.rs
│               └── menu.module.scss
├── views/
│   └── home/
│       ├── mod.rs
│       ├── home.module.scss
│       └── components/            # 私有组件（仅 home 页面使用）
│           ├── tab1/
│           │   ├── mod.rs
│           │   ├── tab1.module.scss
│           │   └── components/    # 私有组件内部的子组件
│           │       └── entry_item/
│           │           ├── mod.rs
│           │           └── entry_item.module.scss
│           ├── tab2/
│           │   ├── mod.rs
│           │   └── tab2.module.scss
│           └── tab3/
│               ├── mod.rs
│               └── tab3.module.scss
```

### 规则总结

- 组件目录名：**snake_case**
- 入口文件：`mod.rs`
- 样式文件：`<组件名>.module.scss`，与 `mod.rs` 同目录
- 一个文件一个组件，**禁止在同一个 `.rs` 文件中定义多个组件函数**
- 组件内子组件放在 `components/` 子目录，结构同上

## 仓库自带 Agent 资产

本仓库自带 agent 工作环境，克隆后即可用，无需额外配置：

- **技能（`.agents/skills/`，12 个，均带 `mt-note-` 前缀）**：`mt-note-dioxus-workspace-rules`（项目规范总纲：js-bridge-bindgen、错误处理分层、workspace 依赖规范、lint 命令）、`mt-note-dioxus-android`、`mt-note-dioxus-reference`、`mt-note-dioxus-frontend-structure`、`mt-note-dioxus-toml`、`mt-note-dioxus-bugs`、`mt-note-dioxus-cli-build-tool`、`mt-note-dioxus-logger`、`mt-note-kameo-actor`、`mt-note-rust-lint`、`mt-note-rust-rules`、`mt-note-cdp`。构建/lint/调试前先加载对应技能；agent 工具自动扫描项目级 `.agents/skills`，按各自约定读取。
- **MCP（`.agents/mcp.json`）**：`cdp-local`（本机浏览器调试）、`cdp-remote`（真机 WebView 调试，连 `127.0.0.1:9228`，使用前先完成上文 5a/5b）。agent 工具按各自约定自动连接。
- **文档**：`docs/agent-environment.md`（外部工具链准备）、`docs/troubleshooting.md`（构建/安装/调试常见坑，动手排障前先查）。

## 搜索工具规范

- 文本搜索一律使用 `rg`（ripgrep），替代 `grep`；递归默认生效，**禁用 `-r`/`--replace`**（那是文本替换不是递归）；要行号用 `-n`，要改文件用编辑工具
- 文件名搜索一律使用 `fdfind`（Debian/Ubuntu 命令名，部分发行版为 `fd`），替代 `find`
