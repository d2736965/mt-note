---
name: mt-note-dioxus-workspace-rules
description: Dioxus workspace 多平台项目开发规范。当在 workspace 多包结构的 Dioxus 项目中编写代码、组织模块、配置依赖、处理跨平台逻辑时使用。（mt-note 项目自带版本）
---

# Dioxus Workspace 项目开发规范

## 架构原则

- **按平台分包**（`web`/`desktop`/`mobile`）+ **按功能分包**（`ui`/`api`/`api-impl`/`models`），避免单 crate + 条件编译
- 平台差异隔离在 `Cargo.toml` 的 feature/dependency 中，避免代码中 `#[cfg(feature = "web")]`
- **api crate 纯库**：只放 `#[post]`/`#[get]` 宏定义 + 共享类型。server-only 业务逻辑（tokio、db、kameo）放 `api-impl`，通过 `dep:api-impl` 按 server feature 引入。`#[post]` 宏自动将函数体包在 `#[cfg(feature = "server")]`，前端不编译
- **`fullstack` 在所有包安全**：`fullstack` ≠ `web` + `server`，只引入桥接层（宏、类型、客户端桩）。写在 `dioxus = { features = ["fullstack"] }` 对所有 target 安全。详见 `mt-note-dioxus-reference` 的 Feature 分层
- 共享 UI 组件放 `ui`，**只在各平台都适合的通用组件才放 ui**。平台特有组件/风格留在各自平台包
- 各包内部遵循 `mt-note-dioxus-frontend-structure` 的目录规范

## Feature 配置

### api crate（纯库）

```toml
[dependencies]
dioxus = { workspace = true, features = ["fullstack"] }
api-impl = { workspace = true, optional = true }

[features]
server = ["dioxus/server"]                    # 无 api-impl
server = ["dioxus/server", "dep:api-impl"]    # 有 api-impl
```

### 平台 crate（web 为例，desktop/mobile 类推替换 feature 名）

```toml
[dependencies]
dioxus = { workspace = true, features = ["router", "fullstack"] }
api = { workspace = true }
ui = { workspace = true }

[features]
default = []
web = ["dioxus/web"]
server = ["dioxus/server", "api/server", "ui/server"]
```

### main.rs（所有平台统一）

```rust
fn main() { dioxus::launch(app); }
fn app() -> Element { rsx! { Router::<Route> {} } }
```

`dioxus::launch(app)` 根据 feature 自动选择启动路径。不需要 `dioxus_server::launch` 或 `set_server_url`。

## 项目结构

```
project-root/
├── crates/
│   ├── web/          # Web 平台 + bin 入口
│   ├── desktop/      # 桌面平台 + bin 入口
│   ├── mobile/       # 移动平台 + bin 入口
│   ├── ui/           # 共享 UI 组件
│   ├── api/          # 共享服务函数定义（纯库）
│   ├── api-impl/     # 服务端业务逻辑（server-only）
│   ├── models/       # 共享数据类型
│   └── utils/        # 共享工具
├── lints/            # clippy.toml + dylint 规则
└── tmp/              # 临时文件
```

## 代码规范

- 编写 API 文档，使用中文注释

### JS → Rust 类型安全桥接（js-bridge-bindgen）

**所有 Rust → JS 调用必须通过 `js-bridge-bindgen` 生成的类型安全绑定，禁止绕过机制直接使用裸 JS。**

#### 三禁令

1. **禁止 `document::eval`** — 禁止在 Rust 代码中直接调用 `dioxus::document::eval(js)` 执行裸 JS 字符串。**clippy 强制执行**：`clippy.toml` 中 `disallowed-methods` 已配置 `dioxus::document::eval`，唯一合法调用点在 `js_bridge.rs` 的 `JsBridge` trait 实现中（已 `#[expect]` 豁免）。**dylint 强制执行**：`#[expect(clippy::disallowed_methods)]` 仅允许在 `js_bridge.rs` 中使用，其他文件使用会触发 `forbid_disallowed_methods_expect` 错误
2. **禁止内联 JS 字符串** — 禁止将 JS 代码写在 `format!` / `r#"..."#` 等 Rust 字符串字面量中
3. **禁止 `include_str!` + `.replace()` 模式** — 禁止将 JS 放 `.js` 文件用 `include_str!` 嵌入再拼接变量；这是 `js-bridge-bindgen` 出现前的旧模式，已被取代

#### 正确做法：js-bridge-bindgen 四步流程

**步骤 1**：在 `ts/src/` 下创建 TS 模块，导出纯函数

```ts
// ts/src/scroll.ts
export function setHomeScrollTop(scrollTop: number): void {
  const el = document.getElementById("home_view");
  if (el) { el.scrollTop = scrollTop; }
}
```

**步骤 2**：在 `ts/src/index.ts` 中注册工厂

```ts
import { setHomeScrollTop } from "./scroll";

export interface Scroll {
  setHomeScrollTop: (scrollTop: number) => void;
}

export function createScroll(): Scroll {
  return { setHomeScrollTop };
}

declare global {
  interface Window {
    __mt_note_mobile: {
      createScroll: () => Scroll;
    };
  }
}

window.__mt_note_mobile = { createScroll, /* ... */ };
```

**步骤 3**：`build.rs` 自动生成 Rust 绑定（`js_bridge_bindgen::generate()`），无需手动维护

**步骤 4**：Rust 侧通过生成的 API 调用

```rust
use crate::frontend::js_bridge::{js_api, DioxusBridge};

// 初始化（app.rs 中调用一次，幂等）
js_api::scroll::create::<DioxusBridge>();

// 业务代码中调用
js_api::scroll::set_home_scroll_top::<DioxusBridge>(target);
```

#### 详细参考

完整机理、工厂模式规范、校验规则、类型映射见 `mt-note-dioxus-reference` 的 **js-bridge-bindgen** 章节。
- **函数返回值默认为 `Result`**：定义的函数、方法只要不是以下情况，默认返回 `Result<T, SomeError>`：
  1. **在线程中**（spawn 闭包），函数签名受限无法返回 Result，但仍须设法将错误传出：通过 `ActorRef::tell` 发送错误消息、channel 推送错误事件、流式 yield 错误 chunk 等方式传播到上层消费者。严重到无法继续时用 `unreachable!()` 终止
  2. **顶层与用户直接接触的位置**（main 函数、UI 组件/hook），这些是 Result 的最终消费者
  3. **非常明确此函数中不会出现错误处理**（纯计算、字段访问等无 fallible 操作）
  
  一旦实现过程中需要处理错误（如调用 fallible 函数、IO 操作），函数签名必须改为返回 `Result`。禁止在返回值为非 Result 的函数中吞没错误
- **禁止 panic/unwrap/anyhow/阻塞型同步原语/`document::eval`/`const`/否定式 cfg**
- **禁止轮询范式，必须事件驱动**：当需要从外部（如 WebView JS、系统回调、硬件传感器等）获取数据或响应变化时，禁止使用定时轮询（`setInterval`/`sleep` 循环/`poll` 循环）。必须使用事件驱动范式——由事件源主动通知消费者，消费者被动响应。具体做法：① JS→Rust：通过 DOM 事件冒泡到 Dioxus 容器元素触发 `onclick` 等处理器，在处理器中用 `eval` 读取事件数据；② 异步数据：通过 channel/Stream/Signal 等推送机制；③ 系统回调：通过回调函数/Future 通知。轮询浪费 CPU、增加延迟、难以保证及时性
- 条件编译必须正向 feature 门控（`#[cfg(feature = "desktop")]`），禁止 `not(feature = "web")`
- 异步耗时任务用 `tokio::spawn_blocking`
- 并发状态用 kameo actor。wasm 中用 `futures_channel::mpsc` (CSP)
- Lint 检查分两步，两命令必须先后通过：
  ```
  cargo clippy --no-deps --all-targets [-p ${PACKAGE_NAME}] [--target ${TARGET}] [--features <FEATURES>] -- -Dwarnings
  cargo dylint --all --no-deps [-p ${PACKAGE_NAME}] -- [--features <FEATURES>] --all-targets
  ```
  - `[]` 中括号参数表示可选：需检查 `Cargo.toml` 中 `[features]` 定义 + 代码中实际启用的 feature，以及 `Cargo.toml` 中是否有特定 target 定义来决定是否添加 `--features` 和 `--target`
  - 不要随意填 `--all-features`（可能导致全栈项目 web+server 同时启用编译失败）
  - `--features` 和 `--target` 必须对应，不可配错：
    | `--features` | `--target` |
    |---|---|
    | `web` | `--target wasm32-unknown-unknown` |
    | `server` | 无需 `--target`（默认 host target） |
    | `desktop` | `--target x86_64-pc-windows-msvc`（示例，按实际桌面平台确定；若 host == desktop_target 则无需 `--target`） |
    | `mobile` | `--target aarch64-linux-android`（示例，按实际移动平台确定；若 host == mobile_target 则无需 `--target`） |
  - Termux 环境下 `cargo dylint` → `dylint`
- **一键 lint 脚本**（可选，项目含 `lints/scripts/check.py` 时可用）：
  ```
  python lints/scripts/check.py [-p ${PACKAGE_NAME}] [--target ${TARGET}] [-f ${FEATURES}]
  ```
  该脚本自动执行 clippy + dylint 两步检查，等价于手动执行上述两条命令。默认人类可读输出，加 `--json` 输出 JSON（供 VS Code 用）
- **推荐不指定参数执行全量 lint**：`python lints/scripts/check.py` 不带 `-p`/`--target`/`-f` 会检查整个 workspace，避免遗漏某个 package/target/feature 组合。指定参数容易导致 lint 不完整。1 分钟内修改并重新执行时只检查增量部分，速度很快
- 构建检测：`dx bundle -p ${PKG} --${PLATFORM}`
- 依赖用 `cargo add`。**workspace 依赖规范（三条）**：① **只版本来源**——`[workspace.dependencies]` 只 version/path/git，features/default-features/cfg 由成员自决；② **version-only 简写**——只有 version 用 `crate = "x"`，不用 `{ version = "x" }`；③ **不收纳单成员专用依赖**——只一个成员用的不放 workspace，成员自声明 version。workspace 是多成员共享的版本表，不耦合具体成员需求。成员用 `workspace = true` 继承版本后加 features/`[target.'cfg']`

## 错误处理分层

**服务端 lib crate**（api、api-impl）→ 禁止 log/println/静默丢弃。错误通过 `?` 传播、流式事件 yield、`Result<Streaming<...>, ServerFnError>` 返回。

**前端 bin crate**（web/desktop/mobile）→ 接收流式事件，写入 UI 状态，展示 error bar/toast。是最终消费者。

> 不在错误链中途"消费"错误。無法 `?` 传播时，通过流式事件/channel 送到最终消费者。

**lint 拦截是信号，不是障碍**：当 clippy/dylint 禁止某种用法时（如 `let _ =`、`drop()`、`black_box()`、空 `{}`），说明该用法在**隐藏错误**。正确的思路不是找 hack 绕过 lint，而是问自己：**"lint 为什么不让我这么做？因为用户应该知道出错了。那我应该把错误展示在哪里？"** 答案：写入 UI 状态（如 `Signal<Option<String>>`），在界面上渲染错误提示，让用户看到。错误被用户看到了，Result 也被正确处理了，lint 自然通过。**不要用 `log::error!` 替代用户可见的错误展示**——lib crate 中 `log::error!` 已被"服务端 lib crate 禁止向控制台输出"规则禁止；bin crate 中 `log::error!` 可用（控制台应用中它就是让用户看到错误的方式），但若应用有 UI，错误还需写入 UI 状态让用户看到。

## 组件副作用规则

**总则：组件渲染必须是纯函数——同样的 props + 状态 → 同样的输出。任何外部状态都不能影响渲染结果。** 副作用必须隔离到 `use_effect` 中，且随依赖变化清理旧副作用、重建新副作用。

Dioxus 在 props 变化时**复用组件而非卸载重建**（路由参数变化是典型场景，但任何父组件传给子组件的 prop 变化都一样）。以下三条规则是总则的具体落地：

### 1. Effect 依赖必须完整

`use_effect` 通过读取 signal 建立响应式依赖。若 effect 内未读取与 prop 关联的 signal，prop 变化时 effect 不会重执行。引用 prop 本身（`&path`）不建立响应式追踪。

**做法**：为需要追踪的 prop 创建 `use_signal`，在组件体顶部同步 prop 值，在 `use_effect` 中读取该 signal 建立依赖。

```rust
let mut path_signal = use_signal(|| path.clone());
if path_signal() != path {
    path_signal.set(path.clone());
}
use_effect(move || {
    let current_path = path_signal(); // 读取 signal → 建立依赖
    // ... 基于 current_path 的副作用
});
```

### 2. 外部副作用必须在 props 变化时清理

WebView 全局 JS 变量（`window.*`）、DOM 事件监听器等外部副作用不受 Dioxus 组件生命周期管理。props 变化时组件复用而非卸载，这些外部状态会残留。必须在 `use_effect` 中随依赖变化清理旧副作用、重建新副作用。

### 3. 派生值直接从 props 计算，不要缓存到 signal

能从 props 直接计算的值，应每次渲染时重新计算。不要用 `use_signal` 缓存——props 变化时 signal 不会自动更新，手动同步反而增加复杂度和出错概率。

## Android Activity JNI 规则

- **Activity 暴露给 Rust JNI 调用的方法必须用实例方法 + `@Keep`，禁止用 `companion object` + `@JvmStatic`**
  - `@JvmStatic` 生成的桥接实例方法在 JNI `call_method` 查找时可能返回 `MethodNotFound`（ProGuard/R8 可能优化掉桥接方法，或 JNI 方法查找机制不认该桥接）
  - 实例方法 + `@Keep` 与 Rust 侧 `call_method`（实例方法调用）天然匹配，且 Activity 生命周期保证实例存在时才调用
  - `@Keep` 同时防止 ProGuard/R8 混淆方法名，确保 Rust 侧按名称查找时方法符号不变
  - 已踩坑：`getAndClearPendingSharedFile` 放在 `companion object` 中导致冷启动分享文件始终失败

## 关联 Skill

- `mt-note-dioxus-reference`：Feature 分层、hooks 时机、server function、流式传输
- `mt-note-dioxus-frontend-structure`：各包内部目录组织规范
- `mt-note-dioxus-toml`：Dioxus.toml 配置
- `mt-note-dioxus-android`：Android 构建
- `mt-note-kameo-actor`：kameo 框架
- `mt-note-rust-lint`：lint 规则初始化

## Shell 命令执行规范

- **git 命令必须加 `--no-pager`**：`git diff`、`git log`、`git show`、`git reflog` 等输出较长的命令默认启用 `less` 分页器，需要用户按 `q` 才能退出，会卡住操作。执行时必须使用 `git --no-pager diff`、`git --no-pager log` 等形式
- **禁止执行会产生交互式等待的命令**：任何需要键盘输入才能继续或退出的命令都不允许执行，避免把自身卡住

## 检查清单

- [ ] **JS → Rust 类型安全桥接**：无 `document::eval` 裸 JS 调用、无内联 JS 字符串、无 `include_str!` + `.replace()` 模式；所有 Rust → JS 调用通过 `js-bridge-bindgen` 生成的类型安全 API；`#[expect(clippy::disallowed_methods)]` 仅限 `js_bridge.rs`（clippy + dylint 双重强制执行）
- [ ] 无 `unwrap`/`expect`/`anyhow`/`const`/阻塞同步原语
- [ ] **无轮询范式**：JS→Rust 数据传递通过 DOM 事件冒泡 + `onclick` 处理器，异步数据通过 channel/Stream/Signal 推送，未使用 `setInterval`/`sleep` 循环/`poll` 循环
- [ ] 无否定式 cfg，条件编译用正向 feature 门控
- [ ] **workspace 依赖规范**：① 只 version/path/git；② version-only 简写；③ 单成员专用依赖不进 workspace（成员自声明）
- [ ] **服务端 lib crate 未向控制台输出**（无 `log::*!`/`println!`）
- [ ] **错误未中途消费**（无 `let _ =`/`.ok()`/空 match arm）
- [ ] **组件副作用规范**：路由参数组件的 `use_effect` 必须读取关联 signal 建立依赖；WebView `window.__*` 等外部副作用必须在 props 变化时清理；派生值直接从 props 计算不缓存到 signal
- [ ] `cargo clippy --no-deps --all-targets [-p ${PACKAGE_NAME}] [--target ${TARGET}] [--features <FEATURES>] -- -Dwarnings` 0 错误 0 警告
- [ ] `cargo dylint --all --no-deps [-p ${PACKAGE_NAME}] -- [--features <FEATURES>] --all-targets` 0 错误 0 警告
- [ ] **函数返回值默认为 `Result`**：非线程、非顶层消费者、非纯计算的函数均已返回 `Result`。实现过程中遇到错误处理需求的函数已改为返回 `Result`