---
name: "mt-note-rust-lint"
description: "Rust lint 规则初始化与配置。为新项目初始化 lint 环境、配置 panic/unwrap 禁止规则、禁用 anyhow/阻塞型同步原语时调用。（mt-note 项目自带版本）"
---

# Rust Lint 规则初始化指南

为新 Rust 项目初始化完整的 lint 规则环境，确保代码质量。

## 初始化步骤

### 1. 在 src/main.rs（或 src/lib.rs）顶部添加 lint 声明

```rust
// 默认全部 forbid
#![forbid(
    unused_must_use,
    clippy::panic,
    clippy::let_underscore_must_use,
    clippy::unwrap_used,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::allow_attributes,
    unfulfilled_lint_expectations
)]
// deny 而非 forbid：kameo 等外部 crate 宏会生成 #[allow(unused_variables)]，forbid 会触发 E0453
#![deny(unused_variables, clippy::expect_used)]
```

**`deny` vs `forbid` 原则**：所有 lint 统一使用 `forbid`，默认禁止降级。`clippy.toml` 的 `allow-*-in-tests = true` 与 `forbid` 兼容。

**例外：`clippy::disallowed_methods` 降为 `deny`**：当 `disallowed-methods` 清单中有规则需要 `#[expect]` 局部豁免时（如 `dioxus::document::eval` 仅允许在 `js_bridge.rs` 的 `JsBridge` trait 实现中使用），将 `clippy::disallowed_methods` 从 `forbid` 改为 `deny`。`forbid` 无法被 `#[expect]` 覆盖，`deny` 允许局部豁免。配合 dylint 的 `forbid_disallowed_methods_expect` 规则防止豁免被滥用。

**降级条件**：当且仅当外部 crate 的宏展开产生 `E0453`（`allow(X) incompatible with previous forbid`）时，**必须征得用户同意后**才能将冲突的 lint 降为 `deny`。未经用户确认不得自行降级。

- **外部 crate 的宏**（`clap::Parser`、`serde::Deserialize` 等）展开时可能内嵌 `#[allow(clippy::*)]`，第三方宏无法修改，降级需用户确认
- **自己 crate 内**产生 E0453 说明代码有问题（macro_rules! 或手写了 `#[allow]`），**禁止降级**，必须修复代码
- `clippy.toml` 的 `allow-*-in-tests` 在驱动层处理，不会产生 E0453，与 `forbid` 兼容

**`#![allow(unknown_lints)]` 条件添加**：

默认不添加。仅当 crate 中使用了 `#[expect(deny_lib_console_output)]` 豁免 dylint 规则时，才在 crate 根添加：

```rust
// 仅在使用了 #[expect(deny_lib_console_output)] 时添加
#![allow(unknown_lints)]  // dylint lint 名对 clippy 不可见，抑制 unknown_lints
```

原因：`deny_lib_console_output` 是 dylint lint，clippy 不认识该名称，会触发 `unknown_lints` 警告（`-D warnings` 下升级为错误）。`#![allow(unknown_lints)]` 是 crate 级属性，不触发 `clippy::allow_attributes`（该项级 `#[allow]` 禁止规则不适用于 crate 级 `#![allow]`）。

`#[expect(deny_lib_console_output)]` 适用场景：纯 `cdylib` 入口点（如 Android .so）中 `log::error!` 走平台日志系统（logcat），不污染控制台。不应滥用于普通 `lib` crate。

```bash
# 测试命令（遇到 E0453 时向用户确认后再降级）
cargo clippy -p ${PACKAGE_NAME} --all-targets [--features <FEATURES>] -- --no-deps -Dwarnings
```

### 2. 在项目根目录创建 clippy.toml

```toml
# ============================================================================
# `#[expect(dead_code)]` 规则
#
# - 统一使用 #[expect(dead_code)]，不使用 #[allow(dead_code)]
# - 枚举：在未使用的变体上标注 #[expect(dead_code)]，而非枚举整体
# - 函数/方法/struct 字段：在 item 上标注 #[expect(dead_code)]
# - 配合 #![forbid(unfulfilled_lint_expectations)]，expect 被满足时自动提醒移除
# ============================================================================

# 测试中允许使用 unwrap/expect/panic
allow-unwrap-in-tests = true
allow-expect-in-tests = true
allow-panic-in-tests = true

# 对应 #![forbid(clippy::disallowed_types)]
# 禁止使用 anyhow；禁止阻塞型同步原语（Mutex/RwLock/mpsc）；禁止有锁容器（DashMap/DashSet）；禁止 ManuallyDrop 绕过 must_use
disallowed-types = [
  { path = "anyhow::Error", reason = "使用 thiserror 处理错误" },
  { path = "anyhow::Result", reason = "使用 thiserror 处理错误" },
  { path = "std::sync::Mutex", reason = "禁用阻塞型同步原语，使用 kameo Actor 模型代替" },
  { path = "std::sync::RwLock", reason = "禁用阻塞型同步原语，使用 kameo Actor 模型代替" },
  { path = "std::sync::mpsc::Sender", reason = "禁用阻塞型同步原语，使用 kameo Actor 模型代替" },
  { path = "std::sync::mpsc::Receiver", reason = "禁用阻塞型同步原语，使用 kameo Actor 模型代替" },
  { path = "tokio::sync::Mutex", reason = "禁用阻塞型同步原语，使用 kameo Actor 模型代替", allow-invalid = true },
  { path = "tokio::sync::RwLock", reason = "禁用阻塞型同步原语，使用 kameo Actor 模型代替", allow-invalid = true },
  { path = "tokio::sync::mpsc::Sender", reason = "禁用阻塞型同步原语，使用 kameo Actor 模型代替", allow-invalid = true },
  { path = "tokio::sync::mpsc::Receiver", reason = "禁用阻塞型同步原语，使用 kameo Actor 模型代替", allow-invalid = true },
  { path = "dashmap::DashMap", reason = "禁止使用有锁容器，使用 kameo Actor 模型代替", allow-invalid = true },
  { path = "dashmap::DashSet", reason = "禁止使用有锁容器（内部为 DashMap<K,()> 的分片 RwLock），使用 kameo Actor 模型代替", allow-invalid = true },
  { path = "std::mem::ManuallyDrop", reason = "禁止用 ManuallyDrop 静默析构 #[must_use] 值。优先用 ? 传播错误，if let Err 仅在线程/顶层等无法传播时使用" },
]

# 对应 #![forbid(clippy::disallowed_methods)]
# 禁止使用 anyhow；禁止阻塞型同步原语的构造方法；禁止 drop()/forget()/black_box() 绕过安全检查
disallowed-methods = [
  { path = "anyhow::anyhow", reason = "使用 thiserror 处理错误", allow-invalid = true },
  { path = "anyhow::bail", reason = "使用 thiserror 处理错误", allow-invalid = true },
  { path = "anyhow::ensure", reason = "使用 thiserror 处理错误", allow-invalid = true },
  { path = "anyhow::format_err", reason = "使用 thiserror 处理错误", allow-invalid = true },
  { path = "anyhow::Context::context", reason = "使用 thiserror 处理错误" },
  { path = "anyhow::Context::with_context", reason = "使用 thiserror 处理错误" },
  { path = "std::sync::mpsc::channel", reason = "禁用阻塞型同步原语，使用 kameo Actor 模型代替" },
  { path = "std::sync::mpsc::sync_channel", reason = "禁用阻塞型同步原语，使用 kameo Actor 模型代替" },
  { path = "tokio::sync::mpsc::channel", reason = "禁用阻塞型同步原语，使用 kameo Actor 模型代替", allow-invalid = true },
  { path = "tokio::sync::mpsc::unbounded_channel", reason = "禁用阻塞型同步原语，使用 kameo Actor 模型代替", allow-invalid = true },
  { path = "std::mem::drop", reason = "禁止用 drop() 绕过 #[must_use] 检查。优先用 ? 传播错误，if let Err 仅在线程/顶层等无法传播时使用" },
  { path = "std::mem::forget", reason = "forget 跳过 Drop 析构，导致资源泄漏。如需提前释放，让变量自然离开作用域" },
  { path = "std::hint::black_box", reason = "禁止用 black_box() 绕过 #[must_use] 检查。优先用 ? 传播错误，if let Err 仅在线程/顶层等无法传播时使用" },
  { path = "dioxus::document::eval", reason = "禁止直接使用裸 JS，必须通过 js-bridge-bindgen 生成的类型安全 API 调用。仅 js_bridge.rs 的 JsBridge trait 实现可 #[expect] 豁免", allow-invalid = true },
]
```

### 3. 在 Cargo.toml 中添加 dylint 库配置

dylint 6.x 使用 `[workspace.metadata.dylint]` 格式（不再是 `dylint.toml`）。在 `Cargo.toml` 的 `[workspace]` 节之后添加：

```toml
[workspace.metadata.dylint]
libraries = [
    { path = "lints/rust-dylint" },
]
```

dylint 库源码位于项目内 `lints/` 目录。如果项目中不存在此目录，将 `~/pkgs/rust/lints` 整个目录（包括 `rust-dylint/` 和 `scripts/`）复制到项目根目录。

**包含的规则**：

| Lint                       | 级别     | 作用                                                                                       |
| -------------------------- | -------- | ------------------------------------------------------------------------------------------ |
| `forbid_underscore_must_use` | `Forbid` | 禁止 `let _named =` 绑定 `#[must_use]` 类型。优先用 `?` 传播，无法传播时用 `if let Err`    |
| `forbid_mpsc_channel`      | `Forbid` | 非 wasm32 目标禁止任何 mpsc channel 构造函数                                                |
| `forbid_dropped_result`    | `Forbid` | 禁止丢弃 Result 或其错误值：`.ok()`/`.err()`/`.is_err()`/`.is_ok()`+`;`、`Err(_)`、`if is_err()`、`let Ok(...) = ... else`。优先用 `?` 传播，无法传播时 `if let Err(e) =` 显式处理 |
| `forbid_bare_return`       | `Forbid` | 禁止裸 `return;`，使用 `?` 传播或 `return Ok(())` / `return Err(...)`                       |
| `forbid_empty_block`       | `Forbid` | 禁止空的 `{}` 分支/arm body（循环体除外），每个分支都必须有显式操作                          |
| `forbid_unused_variables_attr` | `Forbid` | 禁止 `#[allow(unused_variables)]` / `#[expect(unused_variables)]`，这是 `deny(unused_variables)` 下唯一绕过检查的方式 |
| `deny_lib_console_output` | `Deny` | 库 crate 禁止 `log::*!`/`println!`/`eprintln!`/`print!`/`eprint!`/`dbg!` 输出到控制台。bin crate、`tests/` 目录豁免。`Deny` 级别允许 `#[expect(deny_lib_console_output)]` 局部豁免（如 cdylib Android 入口点） |
| `forbid_disallowed_methods_expect` | `Forbid` | 禁止在非 `js_bridge.rs` 文件中使用 `#[expect(clippy::disallowed_methods)]`。`clippy::disallowed_methods` 已禁止 `dioxus::document::eval`，唯一合法豁免点在 `js_bridge.rs` 的 `JsBridge` trait 实现中 |

`forbid_underscore_must_use` 是 clippy `let_underscore_must_use` 的补充——clippy 只检查 `let _ =`（单独的 `_`），不检查 `let _send =` 等命名下划线变量。

### 4. 验证

Lint 检查分两步，两命令必须先后通过：

```bash
# clippy 检查
cargo clippy --no-deps --all-targets [-p ${PACKAGE_NAME}] [--target ${TARGET}] [--features <FEATURES>] -- -Dwarnings

# dylint 检查（-- 之后的参数传递给底层 cargo check，--all-targets 包含测试代码）
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

## 规则详解

### src/main.rs 中的 lint 声明

| Lint                              | 级别     | 作用                                                                 |
| --------------------------------- | -------- | -------------------------------------------------------------------- |
| `unused_must_use`                 | `forbid` | 禁止丢弃 `#[must_use]` 返回值（如 `eval.await;`）                    |
| `clippy::panic`                   | `forbid` | 禁止 `panic!()`                                                      |
| `clippy::unwrap_used`             | `forbid` | 禁止 `.unwrap()`                                                     |
| `clippy::expect_used`             | `forbid` | 禁止 `.expect()`                                                     |
| `clippy::let_underscore_must_use` | `forbid` | 禁止 `let _ = ...` 丢弃 `#[must_use]` 返回值。优先用 `?` 传播，无法传播时用 `if let Err` |
| `clippy::disallowed_types`        | `forbid` | 启用 clippy.toml 中的 `disallowed-types` 配置                        |
| `clippy::disallowed_methods`      | `forbid` | 启用 clippy.toml 中的 `disallowed-methods` 配置                      |
| `clippy::allow_attributes`        | `forbid` | 禁止 `#[allow]`，强制使用 `#[expect]` 代替                           |
| `unfulfilled_lint_expectations`   | `forbid` | 当 `#[expect]` 的 lint 不再触发时报警，用于提醒移除已失效的 `expect` |

`unused_must_use` 和 `clippy::let_underscore_must_use` 必须同时使用，因为 `let _ = ...` 是 Rust 合法语法，单独使用 `unused_must_use` 无法检测到。

### `let _ =` 的处理

`forbid(clippy::let_underscore_must_use)` 意味着 **所有** `let _ =` 忽略 `#[must_use]` 返回值都是编译错误，`#[expect]` 也无法覆盖。必须显式处理：

```rust
// ✅ 正确：显式处理错误
if let Err(e) = storage.set_item("key", "val") {
    log::warn!("save failed: {e:?}");
}

// ❌ 错误：forbid 下 #[expect] 也无法覆盖
#[expect(clippy::let_underscore_must_use, reason = "...")]
let _ = storage.set_item("key", "val");
```

项目需要添加 `log` 依赖用于错误日志。

### clippy.toml 中的配置

**测试豁免**：`allow-*-in-tests = true` 在 clippy 驱动层面工作（不走 `#[allow]` 属性机制），可正常与 `forbid` 配合。只对 `unwrap_used`、`expect_used`、`panic` 生效。`disallowed_types` / `disallowed_methods` 不支持测试豁免。

**`allow-invalid = true`**：当路径在当前编译上下文中不可达时（如 wasm32 target 下 tokio 无 sync 模块、宏不是函数/类型），cliipy 默认警告配置无效。加此参数告诉 clippy 沉默跳过，路径可达时仍然检测。

### build.rs 豁免

`build.rs` 是独立的二进制 crate，`src/main.rs` 中的 lint 声明**不会**应用到 `build.rs`。因此：

- `build.rs` 中可以使用 `.expect()` / `.unwrap()` / `panic!()`
- 构建脚本失败应直接中断构建，不需要优雅降级
- 这与 `#[test]` 函数的豁免逻辑一致：失败时快速暴露问题

## 效果总览

| 场景       | `unwrap` | `expect` | `panic!` | anyhow | 阻塞型同步原语 | `let _ =` | `let _named =` | mpsc channel | 丢弃 Result | `return;` | 空 `{}` | `#[allow/expect(unused_variables)]` | `drop()` | `forget()` | `black_box()` | `ManuallyDrop` | `DashMap`/`DashSet` | `dioxus::document::eval` | `#[expect(clippy::disallowed_methods)]` 非 js_bridge |
| ---------- | -------- | -------- | -------- | ------ | -------------- | --------- | -------------- | ------------ | ----------- | --------- | ------- | ------------------------------------ | -------- | ---------- | ------------- | -------------- | ------------------- | ------------------------ | ---------------------------------------------- |
| 生产代码   | ❌       | ❌       | ❌       | ❌     | ❌             | ❌        | ❌             | ❌           | ❌          | ❌        | ❌      | ❌                                   | ❌       | ❌         | ❌            | ❌             | ❌                  | ❌                       | ❌                                             |
| `#[test]`  | ✅       | ✅       | ✅       | ❌     | ❌             | ❌        | ❌             | ❌           | ❌          | ❌        | ❌      | ❌                                   | ❌       | ❌         | ❌            | ❌             | ❌                  | ❌                       | ❌                                             |
| `build.rs` | ✅       | ✅       | ✅       | ❌     | ❌             | ❌        | ❌             | ❌           | ❌          | ❌        | ❌      | ❌                                   | ❌       | ❌         | ❌            | ❌             | ❌                  | ❌                       | ❌                                             |
| examples   | ❌       | ❌       | ❌       | ❌     | ❌             | ❌        | ❌             | ❌           | ❌          | ❌        | ❌      | ❌                                   | ❌       | ❌         | ❌            | ❌             | ❌                  | ❌                       | ❌                                             |

### 同步原语分类：阻塞型 vs 无锁型

本规则禁止的是**阻塞型同步原语**（会导致线程挂起等待锁），而非所有 `std::sync` 下的类型。无锁原语不在禁止范围内，因为它们不会引起死锁或锁竞争。

| 分类       | 类型                   | 是否禁止 | 原因                                |
| ---------- | ---------------------- | -------- | ----------------------------------- |
| **阻塞型** | `std::sync::Mutex`     | ❌ 禁止  | 互斥锁，线程阻塞等待，易死锁        |
| **阻塞型** | `std::sync::RwLock`    | ❌ 禁止  | 读写锁，写锁阻塞所有读者            |
| **阻塞型** | `std::sync::mpsc::*`   | ❌ 禁止  | 阻塞式通道                          |
| **阻塞型** | `tokio::sync::Mutex`   | ❌ 禁止  | 异步互斥锁，持有跨 await 导致死锁   |
| **阻塞型** | `tokio::sync::RwLock`  | ❌ 禁止  | 异步读写锁，同上                    |
| **阻塞型** | `tokio::sync::mpsc::*` | ❌ 禁止  | 异步通道，应用 kameo Actor 消息替代 |
| **无锁型** | `std::sync::Arc`       | ✅ 允许  | 引用计数共享所有权，不涉及锁        |
| **无锁型** | `std::sync::atomic::*` | ✅ 允许  | CPU 原子指令，无锁无阻塞            |
| **无锁型** | `std::sync::OnceLock`  | ✅ 允许  | 一次性初始化，内部用原子操作        |

**替代方案**：阻塞型场景使用 kameo Actor 模型的消息传递替代锁；WASM 环境下 kameo 不可用，使用 `futures_channel::mpsc` 的 CSP 模型替代。

## 正确写法

### `#[expect]` vs `#[allow]` — 选择指南

**统一使用 `#[expect]`，禁止 `#[allow]`。** `#![forbid(clippy::allow_attributes)]` 会在编译期拦截所有 `#[allow]` 用法。

| 属性                   | 适用场景                                                     | 原因                                                                     |
| ---------------------- | ------------------------------------------------------------ | ------------------------------------------------------------------------ |
| `#[expect(dead_code)]` | **所有未使用的代码**（枚举变体 / 函数 / 方法 / struct 字段） | 未来使用后自动提醒移除；配合 `#![forbid(unfulfilled_lint_expectations)]` |

```rust
// ✅ 枚举：在未使用的变体上标注 expect
pub enum Icon {
    ArrowBack,   // ← 被使用了，无需标注
    #[expect(dead_code, reason = "图标变体作为公共 API 预定义，页面按需引用")]
    Camera,      // ← 未被使用，标注 expect
    #[expect(dead_code, reason = "图标变体作为公共 API 预定义，页面按需引用")]
    Settings,    // ← 未被使用，标注 expect
}

// ✅ expect：整个函数/方法完全未被调用
#[expect(dead_code, reason = "保留给未来场景使用")]
pub fn toggle_theme(&mut self) { /* ... */ }

// ✅ expect：struct 字段完全未被读取
pub struct Foo {
    pub title: String,
    #[expect(dead_code, reason = "保留给未来场景使用")]
    pub unused_field: u32,
}

// ❌ 错误：枚举整体上用 allow — 静默抑制，将来变体被使用也不会提醒
#[allow(dead_code)]
pub enum Icon { /* ... */ }

// ❌ 错误：枚举整体上用 expect — 部分变体被引用导致 expect 不满足
#[expect(dead_code, reason = "...")]
pub enum Icon { /* ... */ }     // 触发 unfulfilled_lint_expectations
```

**工作原理：** `#[expect(dead_code)]` = 预期此处有 `dead_code` 警告 → 抑制它。当 item 被引用后不再有 `dead_code` 警告时，编译器发出 `unfulfilled_lint_expectations` 提醒移除该属性。枚举整体被部分引用时 item 级别不触发 `dead_code`，因此必须在**变体级别**标注。

### `#[expect(dead_code)]` 跨目标死结 — 禁止在 bin crate 用属性打补丁

**现象**：某 API 在 bin crate（`pub mod` / `pub fn`）中声明，正式代码不调用，但 `#[cfg(test)]` 测试引用了它。`--all-targets` 检查下：

- 不标 `#[expect(dead_code)]` → bin 目标报 `dead_code`（`-Dwarnings` 失败）
- 标 `#[expect(dead_code)]` → test 目标里该 API"活着" → `dead_code` 不触发 → `unfulfilled_lint_expectations` 报错

两个目标的 `dead_code` 判定相反，**任何属性标注都无法同时满足**。

**根因**：bin crate 的语义是"只有 `main` 调用链上的东西才算活"。在 bin 里声明不被 `main` 触达的 API，它天然是 dead code。`#[expect]` 假设单一目标的确定性判断，无法表达"bin 目标里死、test 目标里活"这种跨目标分歧。

**判别信号**：当一段代码"标了 `#[expect(dead_code)]` 却有测试在调用它"，说明它是**库语义的代码（公共 API + 配套测试）放进了 bin 语义的容器**——`#[expect]` 在这里是自相矛盾的（"预期它死"，但测试明明让它活着）。

**解法**：把 API 移到 lib crate，成为该 crate 的 `pub` 公共 API。

```rust
// ❌ 错误：bin crate 里声明不被 main 调用的 API + 测试引用它
// crates/app/src/main.rs
pub mod highlight;          // 正式代码不调用
// crates/app/src/highlight.rs
pub fn tokenize(...) {}     // dead_code（bin 目标）
#[cfg(test)]
mod tests { use super::tokenize; }  // 但 test 目标又"活"→ 跨目标死结

// ✅ 正确：API 移到 lib crate，成为其公共契约
// crates/editor-lsp/src/lib.rs
pub mod highlight;          // lib 的 pub API，结构性"活"
// crates/editor-lsp/src/highlight.rs
pub fn tokenize(...) {}     // 被 mobile 跨 crate 调用 + 测试调用 → 两目标一致地"活"
```

`#[expect(dead_code)]` 的正用场景仅一种：**该 API 在所有目标里都暂时没人用、但保留合理（预留功能）**，且配合 `unfulfilled_lint_expectations` 在将来被使用时自动提醒移除。一旦任何目标（含 test）在用，它就不再是 dead，`#[expect]` 不成立——此时该做的是让它成为 lib crate 的公共 API，而非在 bin 里打补丁。

### 常规写法

```rust
fn example() -> Result<(), MyError> {
    let val = fallible_op().map_err(MyError::OpFailed)?;  // ✅ 优先 ? 传播

    // 无法传播时（spawn 闭包、顶层 main），用 if let Err(e) = 显式处理
    if let Err(e) = some_side_effect() {
        error!("副作用失败: {e:?}");     // 一般错误用 error! 上报
    }

    Ok(())
}

// 严重错误（不可恢复）
if let Err(e) = critical_op() {
    unreachable!("关键操作失败: {e:?}");  // 触发 panic，但表达"不可达"语义
}

#[test]
fn test_example() {
    let val = fallible_op().unwrap();
    assert_eq!(val, 42);
}
```

### build.rs 写法

```rust
fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    // build.rs 中 expect/unwrap 是正确的——构建失败应直接中断
}
```

## 错误写法

```rust
fn example() -> Result<(), MyError> {
    let val = fallible_op().unwrap();        // ❌ forbid(clippy::unwrap_used)
    let _ = side_effect().await;             // ❌ forbid(clippy::let_underscore_must_use)
    panic!("不该到这");                       // ❌ forbid(clippy::panic)
}

// ❌ forbid(forbid_dropped_result)
fallible_op().ok();                          // .ok() 转换 Result 后丢弃
fallible_op().is_err();                      // .is_err() 转换 Result 后丢弃
match result { Err(_) => {} }                // Err(_) 丢弃错误值
if result.is_err() {}                        // is_err() 销毁错误值
let Ok(x) = result else { break; };          // let-else 丢弃 Err

// ❌ clippy::disallowed_methods
std::mem::drop(value);                       // drop() 绕过 #[must_use] 检查，让变量自然离开作用域
core::mem::forget(value);                    // forget() 跳过 Drop 析构导致资源泄漏
std::hint::black_box(value);                 // black_box() 绕过 #[must_use] 检查

// ❌ clippy::disallowed_types
let _ = std::mem::ManuallyDrop::new(result); // ManuallyDrop 静默析构 #[must_use] 值
let map = dashmap::DashMap::new();           // 有锁容器，使用 kameo Actor 代替

// ❌ forbid(forbid_bare_return)
if cond { return; }                          // 裸 return;，用 ? 或 return Err(...)

// ❌ forbid(forbid_empty_block)
if cond {}                                   // 空 if body
match x { _ => {} }                          // 空 match arm

// ❌ forbid(forbid_unused_variables_attr)
#[allow(unused_variables)]
let x = fallible_op();
#[expect(unused_variables)]
let y = fallible_op();

// ❌ clippy::disallowed_methods (dioxus::document::eval)
dioxus::document::eval("document.title = 'hi'")  // 禁止裸 JS 调用

// ❌ forbid(forbid_disallowed_methods_expect) — 非 js_bridge.rs 中使用
#[expect(clippy::disallowed_methods)]
fn some_fn() { /* ... */ }
```

## 参考资源

- [Clippy lint 配置](https://doc.rust-lang.org/nightly/clippy/lint_configuration.html)
- [Clippy lint 列表](https://rust-lang.github.io/rust-clippy/master/index.html)
- [unused_must_use](https://doc.rust-lang.org/rustc/lints/listing/warn-by-default.html#unused-must-use)
- [let_underscore_must_use](https://rust-lang.github.io/rust-clippy/master/index.html#let_underscore_must_use)
- [unwrap_used](https://rust-lang.github.io/rust-clippy/master/index.html#unwrap_used)
- [expect_used](https://rust-lang.github.io/rust-clippy/master/index.html#expect_used)
- [panic](https://rust-lang.github.io/rust-clippy/master/index.html#panic)
- [disallowed_types](https://rust-lang.github.io/rust-clippy/master/index.html#disallowed_types)
- [disallowed_methods](https://rust-lang.github.io/rust-clippy/master/index.html#disallowed_methods)
- [dylint](https://github.com/trailofbits/dylint) — 自定义 lint 框架