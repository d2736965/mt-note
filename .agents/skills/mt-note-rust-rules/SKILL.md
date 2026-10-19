---
name: mt-note-rust-rules
description: Rust 项目开发规范。当编写或审查 Rust 代码、规划 Rust 项目架构、组织模块结构、处理错误处理策略、选择并发模型时使用。适用于所有 Rust 项目。（mt-note 项目自带版本）
---

# Rust 项目开发规范

本规范适用于所有 Rust 项目，定义了架构原则、代码质量标准、模块组织规则和开发流程约束。

## 平台适配

以下命令在不同平台上有区别：

| 平台 | dylint 命令 | 说明 |
|------|-----------|------|
| 标准 Linux/macOS（有 rustup） | `cargo dylint` | cargo 子命令 |
| Termux/Android（无 rustup） | `dylint` | `~/.cargo/bin/dylint` 包装脚本，自动处理 nightly 环境 |

判断方法：检查 `uname -o` 是否为 `Android`，或检查 `~/.cargo/bin/dylint` 是否存在。

本文档命令示例使用 `cargo dylint`（标准环境），Termux 下将 `cargo dylint` 替换为 `dylint`，参数不变。

## 架构原则

- 项目结构应保持简洁，根据项目复杂度选择合适的组织方式（单 crate 或 workspace）
- 采用模块化设计，按功能领域和职责划分模块
- 遵循所选框架的架构模式（如 ECS、MVC、组件化等）
- 数据存储格式应根据项目需求选择（JSON、TOML、YAML、数据库等），使用合适的序列化库
- 错误处理统一使用类型安全的错误处理机制（如 `thiserror`），**禁止使用 `anyhow`**
- **函数返回值默认为 `Result`**：定义的函数、方法只要不是以下情况，默认返回 `Result<T, SomeError>`：
  1. **在线程中**（spawn/tokio::spawn 闭包），函数签名受限无法返回 Result，但仍须设法将错误传出：通过 `ActorRef::tell` 发送错误消息、channel 推送错误事件、流式 yield 错误 chunk 等方式传播到上层消费者。严重到无法继续时用 `unreachable!()` 终止
  2. **顶层与用户直接接触的位置**（main 函数、UI 组件/hook、顶层日志），这些是 Result 的最终消费者
  3. **非常明确此函数中不会出现错误处理**（纯计算、字段访问等无 fallible 操作）
  
  一旦实现过程中需要处理错误（如调用 fallible 函数、IO 操作），函数签名必须改为返回 `Result`。禁止在返回值为非 Result 的函数中吞没错误

## 代码规范

- 必须编写适当的 API 文档，使用中文注释
- 遵守 lint 规则：**禁止 panic/unwrap/anyhow/阻塞型同步原语/有锁容器（DashMap/DashSet）/MustUse 绕过（drop/forget/black_box/ManuallyDrop）/丢弃 Result/裸 return;/空 {}/#[allow,expect(unused_variables)]**；无锁原语（`Arc`、`AtomicBool`、`OnceLock`）允许使用
- **优先使用 `derive_more` 消除样板代码**：遇到标准库 trait 的手动实现时，先查阅 `derive_more` 是否提供了对应的派生宏，有则优先使用。派生宏无法满足的复杂场景才手写 trait 实现
- **Result 必须处理，不得静默丢弃**：错误要让人知道，**传播错误（`?`）优先于解包 Result**。dylint 规则 `forbid_dropped_result`、`forbid_underscore_must_use`、`forbid_bare_return`、`forbid_empty_block`、`forbid_unused_variables_attr` 联合拦截所有丢弃模式。无法传播时（spawn 闭包、顶层 main），用 `if let Err(e) = ... { ... }` 显式处理：一般错误 `error!(...)` 上报，严重错误 `unreachable!(...)` 终止
- **`Result<_, E>` 的 `#[must_use]` 合法消费者**：`Result<_, E>` 类型被标记为 `#[must_use]`，意味着它的值**不允许被静默丢弃**——必须有一个明确的操作来消费它。但不是任何消费操作都是合法的：

  **合法的消费者（允许消费 `Result` 的场所）**：
  | 消费者 | 消费方式 | 适用场景 |
  |--------|---------|---------|
  | **UI 层（bin crate 的页面/组件）** | 将 `Err(e)` 转为用户可见的错误提示、toast、error bar | 前端页面、CLI 输出 |
  | **bin crate 的日志** | `log::error!("{e:?}")` 写入日志 | 后台服务、daemon |
  | **调用链上层的业务逻辑** | `?` 传播后再决定如何处理 | 中间层透传 |
  | **测试代码** | `.unwrap()` / `.expect()` 直接 panic | `#[test]` 函数 |
  | **不可恢复的严重错误** | `unreachable!("{e:?}")` 终止进程 | 初始化失败、关键资源不可用 |

  **非法的消费者（被 lint 规则拦截）**：
  | 非法消费方式 | 拦截规则 | 为什么非法 |
  |-------------|---------|-----------|
  | `let _ = result;` | `forbid(clippy::let_underscore_must_use)` | `_` 是匿名丢弃 |
  | `let _err = result;` | `forbid_underscore_must_use` (dylint) | 下划线命名绑定 = 意图丢弃 |
  | `result.ok();` | `forbid_dropped_result` (dylint) | `.ok()` 转换后丢弃 |
  | `if let Err(_) = result {}` | `forbid_dropped_result` (dylint) | `Err(_)` 丢弃错误值 |
  | `if let Err(e) = result { }` | `forbid_empty_block` (dylint) | 匹配了错误但空块丢弃 |
  | 自定义 `fn noted(e) {}` | 本质仍在错误链中途消费 | 换了个名字的 `drop()` |
          | `std::mem::drop(result);` | `clippy::disallowed_methods` | `drop()` 绕过 `#[must_use]` |
          | `std::mem::forget(result);` | `clippy::disallowed_methods` | `forget()` 跳过 Drop 泄漏资源 |
          | `std::hint::black_box(result);` | `clippy::disallowed_methods` | `black_box()` 绕过 `#[must_use]` |
          | `std::mem::ManuallyDrop::new(result)` | `clippy::disallowed_types` | `ManuallyDrop` 静默析构 `#[must_use]` |
          | `dashmap::DashMap::new()` | `clippy::disallowed_types` | 有锁容器，用 kameo Actor 代替 |
  | lib crate 中 `log::error!(...)` | `forbid_lib_console_output` (dylint) | 库不配决定日志输出 |

  **核心原则**：`#[must_use]` 要求每个 `Result` 都有一个归宿，而**这个归宿必须是调用链上最靠近用户/外部系统的那个节点**——即"最终消费者"。不在最终消费者位置的代码，唯一能做的是**传播**（`?`）或**转发**（yield / channel / return），不能消费。

- **lint 拦截是信号，不是障碍**：当 clippy/dylint 禁止某种用法时（如 `let _ =`、`drop()`、`black_box()`、空 `{}`），说明该用法在**隐藏错误**。正确的思路不是找 hack 绕过 lint，而是问自己：**"lint 为什么不让我这么做？因为用户应该知道出错了。那我应该把错误展示在哪里？"** 答案：写入用户可见的状态（如 UI 错误提示、CLI 输出等），让用户看到。错误被用户看到了，Result 也被正确处理了，lint 自然通过。**不要用 `log::error!` 替代用户可见的错误展示**——lib crate 中 `log::error!` 已被"上游型 crate 禁止向控制台输出"规则禁止；bin crate 中 `log::error!` 可用（控制台应用中它就是让用户看到错误的方式），但若应用有 UI，错误还需写入 UI 状态让用户看到。
- **上游型 crate（库）禁止向控制台输出**：库代码不得使用任何输出到 stdout/stderr 的机制，包括 `log::*!`（所有级别）、`println!`、`eprintln!`、`print!`、`eprint!`、`dbg!`。理由：库不知道下游的运行环境——控制台输出可能污染下游的 TUI 界面、管道数据、日志格式。错误只能通过两种方式处理：① `Result` 向上传播让调用者决定；② 严重到无法继续时 `unreachable!()` 终止。**bin crate 不受此限制**。`#[cfg(test)]` 模块和 `tests/` 目录可豁免
- 禁止在 crate 代码中使用 `Drop` 执行业务逻辑：Drop 仅用于资源清理（内存、文件句柄等），不应包含网络请求、数据库写入、消息发送等 fallible 操作
- 依赖使用 `cargo add` 添加，确保使用最新的依赖版本
- **workspace 依赖规范（三条）**：
  1. **只版本来源**：`[workspace.dependencies]` 只声明 `version`/`path`/`git`，不写 features、default-features、cfg 条件（由成员自决）
  2. **version-only 简写**：只有 version 时用 `crate = "x"`，不用 `{ version = "x" }`；有 path/package 等才用表语法
  3. **不收纳单成员专用依赖**：只一个成员用的依赖不放 workspace，由该成员自声明 `version`；workspace 只放**多成员共享**的依赖

  理由：workspace 是多成员共享的版本集中表，不为某成员需求加约束，也不收纳单成员专用依赖。
  ```toml
  # ✅ workspace — 多成员共享，只版本来源，version-only 简写
  [workspace.dependencies]
  serde = "1.0"                       # 简写（多成员用）
  ui = { path = "crates/ui" }         # path 用表（多成员用）

  # ✅ 成员 — features/cfg/version 自决
  [dependencies]
  tokio = { workspace = true, features = ["rt-multi-thread", "macros"] }

  # ✅ 单成员专用 — 成员自声明，不进 workspace
  [target.'cfg(target_os = "android")'.dependencies]
  reqwest = { version = "0.13", default-features = false, features = ["native-tls"] }
  ```
  ```toml
  # ❌ workspace 写 features/条件 — 耦合成员需求
  [workspace.dependencies]
  tokio = { version = "1", features = ["rt-multi-thread"] }

  # ❌ version-only 用表语法 — 啰嗦
  [workspace.dependencies]
  serde = { version = "1.0" }         # 应 serde = "1.0"

  # ❌ 单成员专用进 workspace — 无其他成员共享
  [workspace.dependencies]
  reqwest = "0.13"                    # 只一个成员用，应放该成员自声明
  ```
- 代码有变动必须通过 lint 检查，必须 **0 错误 0 警告**
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
- dylint 补充规则，详见 `mt-note-rust-lint` skill 第 3 步
- 项目全部开发完成后执行最终检查：
  ```
  cargo machete
  cargo fmt --check
  cargo clippy --no-deps --all-targets [-p ${PACKAGE_NAME}] [--target ${TARGET}] [--features <FEATURES>] -- -Dwarnings
  cargo dylint --all --no-deps [-p ${PACKAGE_NAME}] -- [--features <FEATURES>] --all-targets
  cargo build -p ${PACKAGE_NAME} --target ${TARGET} [--features <FEATURES>]
  ```
- 最终检查顺序：清理未使用依赖（machete） → 格式检查（fmt） → clippy → dylint → 构建（build），任一步失败即停止
- UI 组件必须提供用户交互反馈（按钮悬停效果、点击响应等）
- UI 组件使用框架提供的类型安全 API，避免直接操作底层接口或执行不安全的代码
- 使用 **kameo actor 模型**来管理异步并发的共享状态，避免使用阻塞型同步原语（`Mutex`/`RwLock`/`mpsc`）；**必须优先使用宏语法消除样板代码**：`#[derive(Actor)]` 定义 Actor、`#[messages]` 定义消息、`#[derive(Reply)]` 定义自定义 Reply 类型；仅当宏无法满足时（自定义生命周期钩子、`DelegatedReply`/`ForwardedReply`、泛型关联类型参数如 `B::Message`）才手动实现 trait，详见 `mt-note-kameo-actor` skill
- 在 wasm 中 kameo 不可用，使用 `futures_channel::mpsc` 中的通道来使用 CSP 模型替代 actor 模型
- 异步任务 future 中的阻塞类任务必须使用 `tokio::spawn_blocking`，避免阻塞异步线程池
- 尽量适当地使用独立上下文的子代理操作，避免上下文膨胀
- **需要串行运行测试时使用 `serial_test` crate**：集成测试涉及真实 API 调用且有频率限制（如 LLM API 的 RPM/TPM 限制）时，使用 `#[serial]` 属性确保测试串行执行，避免并发请求触发 429 错误。使用方式：在 workspace `Cargo.toml` 添加 `serial_test = "3"` 到 `[workspace.dependencies]`；在 crate `Cargo.toml` 添加 `serial_test = { workspace = true }` 到 `[dev-dependencies]`；在需要串行的测试函数上添加 `use serial_test::serial;` 和 `#[serial]`。`--test-threads=1` 只能保证**单 crate 内**串行，workspace 多 crate 测试时需用 `#[serial]` 保证跨 crate 互斥
- **禁止引入常量（`const`）**，全局共享状态统一通过框架提供的状态管理机制（如 Bevy Resource、Dioxus Signal 等）管理，保持状态管理风格一致
- **禁止轮询范式，必须事件驱动**：当需要从外部（如 WebView JS、系统回调、硬件传感器等）获取数据或响应变化时，禁止使用定时轮询（`setInterval`/`sleep` 循环/`poll` 循环）。必须使用事件驱动范式——由事件源主动通知消费者，消费者被动响应。具体做法：① UI 框架事件：通过框架提供的事件机制（DOM 事件冒泡、回调函数等）触发处理器，在处理器中读取事件数据；② 异步数据：通过 channel/Stream 等推送机制；③ 系统回调：通过回调函数/Future 通知。轮询浪费 CPU、增加延迟、难以保证及时性

## 项目结构

- 推荐的模块划分原则：
  - **数据模型层**：定义领域数据结构和类型
  - **状态管理层**：管理应用状态和状态转换逻辑
  - **UI 组件层**：封装可复用的 UI 组件
  - **系统逻辑层**：实现业务逻辑和系统功能
- **应用（bin-only）默认不需要 lib.rs**：crate 根声明 `mod` 即项目模块化。唯一例外：bin crate 里「仅测试引用」的代码存在跨目标 `dead_code` 死结（bin 目标判定死、test 目标判定活，无属性可解，编译层无解）——遇到即把该逻辑拆到 lib crate，详见 mt-note-rust-lint 技能「`#[expect(dead_code)]` 跨目标死结」一节
- **组织代码：模块化 + 面向对象封装抽象，避免扁平化**：状态进结构体、行为进方法（含关联函数）、常量挂拥有它的类型，入口文件（`main.rs`）只做薄壳装配，不堆模块级自由函数
- 每个模块应包含：模块入口文件（`mod.rs`）、相关的类型定义、实现和测试、必要的文档注释
- 静态资源（如 JSON 数据文件）应放在 `assets/` 目录下
- 遵循单一职责原则，每个模块专注于特定的功能领域

## 样式规范

- 使用框架提供的样式系统进行样式定义
- UI 组件应使用合理的定位策略（相对定位、绝对定位、弹性布局等）
- 颜色使用标准格式（RGBA、HSLA 等），支持透明度
- 样式应保持一致性，遵循设计系统或风格指南
- **禁止使用 CSS `overflow: hidden`**，内容溢出应通过框架提供的布局机制处理

## 参考资源

- 优先参考所用框架的官方文档和示例代码
- 加载相关 skill 了解详细使用方法和最佳实践
- 关注社区资源和开源项目，学习成熟的架构模式

## 开发环境维护

- 调试应用时注意查看控制台输出，错误信息有助于定位问题
- 根据框架特性利用热重载或快速编译功能，提高开发效率
- 定期更新依赖版本，关注安全公告和 breaking changes

## 临时文件

- 产生的临时文件放到项目根目录的 `tmp/` 目录下，不能被 git 跟踪

## Shell 命令执行规范

- **git 命令必须加 `--no-pager`**：`git diff`、`git log`、`git show`、`git reflog` 等输出较长的命令默认启用 `less` 分页器，需要用户按 `q` 才能退出，会卡住操作。执行时必须使用 `git --no-pager diff`、`git --no-pager log` 等形式
- **禁止执行会产生交互式等待的命令**：任何需要键盘输入才能继续或退出的命令都不允许执行，避免把自身卡住

## 检查清单

在提交代码前，确认以下事项：

### 上游型 crate（库）禁止项

> **原则**：库代码不得向控制台输出任何内容，因为会污染下游的 stdout/stderr。错误只能通过 `Result` 返回或 `unreachable!()` 终止。

- [ ] 没有使用 `unwrap()` 或 `expect()`
- [ ] 没有使用 `anyhow`
- [ ] 没有使用 `const` 定义全局常量
- [ ] **库代码未向控制台输出**：`log::*!`（`info!`/`warn!`/`error!`/`debug!`）、`println!`、`eprintln!`、`print!`、`eprint!`、`dbg!` 全部禁止。唯一例外：`tests/` 目录和 `#[cfg(test)]` 模块
- [ ] **错误未被吞没**：所有 `Result` 通过 `?` 传播，或由调用者显式处理；无法传播时用 `unreachable!()`。禁止 `let _ =`、`.ok()`、`.is_err()` + `;`、`Err(_) => ...` 等丢弃模式
- [ ] **lint 拦截时优先让用户看到错误**：当 clippy/dylint 禁止某种用法时，不是找 hack 绕过，而是把错误展示给用户（UI 错误提示、CLI 输出等）。lib crate 中 `log::error!` 已被禁止；bin crate 中 `log::error!` 可用（控制台应用中它就是让用户看到错误的方式），但若应用有 UI，错误还需写入 UI 状态
- [ ] **错误在正确层级消费**：未在 spawn task / helper function / 初始化闭包等错误链路中途用 `let _xxx =`、`noted()`、空 match arm 等方式"消化"错误。错误已传递给最终消费者（UI 层、bin crate）处理
- [ ] **库 crate 错误通过返回值传播**：库代码中的错误通过 `Result`、流 chunk（如 `ChatChunk::Error`）、channel 等方式传递给下游，而非在库内部自行消费
- [ ] Drop 仅用于资源清理，不含业务逻辑（网络请求、文件写入、消息发送等）
- [ ] 没有使用阻塞型同步原语（`Mutex`/`RwLock`/`std::mpsc`）
- [ ] **无轮询范式**：外部数据通过框架事件机制/channel/Stream 推送获取，未使用 `setInterval`/`sleep` 循环/`poll` 循环

### 下游应用 crate（bin）允许项

> 应用层可以使用 `log` 记录日志，因为它是最终消费者，控制台输出是预期行为。

### 通用项

- [ ] 异步阻塞任务使用了 `tokio::spawn_blocking`
- [ ] **workspace 依赖规范**：① 只 version/path/git（无 features/default-features/cfg）；② version-only 简写 `crate = "x"`；③ 单成员专用依赖不进 workspace（成员自声明）
- [ ] `cargo clippy --no-deps --all-targets [-p ${PACKAGE_NAME}] [--target ${TARGET}] [--features <FEATURES>] -- -Dwarnings` 通过，0 错误 0 警告
- [ ] `cargo dylint --all --no-deps [-p ${PACKAGE_NAME}] -- [--features <FEATURES>] --all-targets` 通过，0 错误 0 警告
- [ ] `cargo machete` 通过，无未使用的依赖
- [ ] `cargo build` 通过
- [ ] **函数返回值默认为 `Result`**：非线程、非顶层消费者、非纯计算的函数均已返回 `Result`。实现过程中遇到错误处理需求的函数已改为返回 `Result`