---
name: "mt-note-dioxus-logger"
description: "Dioxus 0.7 logger (tracing) 配置参考。当涉及 dioxus 日志初始化、tracing subscriber 配置、Android logcat 输出、tracing-android layer、dioxus::logger::tracing re-export 使用时调用。（mt-note 项目自带版本）"
---

# Dioxus Logger (tracing) 配置参考

## 核心架构

`dioxus-logger` 是 Dioxus 对 `tracing` 日志系统的封装，提供跨平台日志初始化。

### ⚠️ 版本一致性（最重要）

**项目必须使用与 dioxus 框架内部相同的 `tracing` 版本。** 这是硬性要求，原因：

1. `tracing` 的全局 subscriber 是单例，由 `tracing-core` 的全局变量管理
2. 如果项目直接依赖的 `tracing` 版本与 `dioxus-logger` re-export 的版本不一致，会导致：
   - 宏展开后调用的是不同版本的 `tracing-core`，全局 subscriber 不共享
   - 项目设置的 subscriber 对 dioxus 内部的日志不生效，反之亦然
   - 编译可能通过（semver 兼容），但运行时日志丢失或 subscriber 冲突

**正确做法**：永远通过 `dioxus::logger::tracing` 使用 tracing，**禁止直接依赖 `tracing` crate**。这样 Cargo 自动保证版本一致：

```toml
# ❌ 绝对禁止：引入可能不一致的 tracing 版本
tracing = "0.1"
tracing.workspace = true

# ✅ 唯一正确方式：通过 dioxus re-export
dioxus = { workspace = true }
# 代码中：use dioxus::logger::tracing;
```

对于 `tracing-subscriber` 和 `tracing-android`，它们依赖 `tracing-core`，Cargo 的 semver 解析会自动对齐到 dioxus 使用的 `tracing-core` 版本，无需额外处理。

### re-export

```rust
// dioxus-logger 源码
pub use tracing;
```

项目中**不需要直接依赖 `tracing` crate**，统一通过 `dioxus::logger::tracing` 使用：

```rust
use dioxus::logger::tracing;

tracing::error!("something failed: {e}");
tracing::info!("status: {s}");
tracing::debug!("dbg: {v:?}");
```

### API

| 函数 | 说明 |
|------|------|
| `dioxus::logger::initialize_default()` | 自动初始化：debug → `Level::DEBUG`，release → `Level::INFO`。`dioxus::launch` 自动调用，检测到已设置则跳过 |
| `dioxus::logger::init(level)` | 指定级别初始化，返回 `Result<(), SetGlobalDefaultError>` |

### 非 wasm 平台默认 subscriber

`dioxus-logger::init()` 在非 wasm 平台使用 `tracing_subscriber::FmtSubscriber`：

```rust
let sub = tracing_subscriber::FmtSubscriber::builder()
    .with_max_level(level)
    .with_env_filter(
        tracing_subscriber::EnvFilter::builder()
            .with_default_directive(level.into())
            .from_env_lossy()
            .add_directive("hyper_util=warn".parse().unwrap())
    );
// dx 下：去掉时间和 target（CLI 友好）
// 非 dx 下：完整 fmt 输出
```

**不支持 Mobile 和 TUI 平台**——`dioxus-logger` 的默认初始化仅覆盖 Web (wasm) 和 Desktop，Mobile 和 TUI 平台没有对应的 subscriber 实现。`FmtSubscriber` 输出到 stdout/stderr，Android 上不可见。

## Android 配置（logcat 输出）

Android 必须在 `dioxus::launch(app)` **之前**手动初始化 `tracing-android` layer。`dioxus::launch` 内部会调用 `dioxus::logger::initialize_default()`，但它检测到 `has_been_set() == true` 时会跳过，不会重复初始化，两者不冲突。

### Cargo.toml 依赖

```toml
[dependencies]
dioxus = { workspace = true, features = ["router", "asset"] }
# 不需要直接依赖 tracing（通过 dioxus::logger::tracing 使用）
tracing-subscriber = { version = "0.3", default-features = false, features = ["registry"] }

[target.'cfg(target_os = "android")'.dependencies]
tracing-android = "0.2.0"
```

### main.rs 初始化

```rust
fn main() {
    // Android 下在 launch 前初始化 tracing subscriber（logcat layer）。
    // dioxus::launch 内部会调 dioxus_logger::initialize_default()，
    // 但它检测到 has_been_set() == true 会跳过，不冲突。
    #[cfg(target_os = "android")]
    {
        use dioxus::logger::tracing;
        use tracing_subscriber::prelude::*;
        let layer = match tracing_android::layer("MyApp") {
            Ok(l) => l,
            Err(e) => unreachable!("tracing-android layer 创建失败: {e}"),
        };
        if let Err(e) = tracing::subscriber::set_global_default(
            tracing_subscriber::Registry::default().with(layer),
        ) {
            unreachable!("tracing subscriber 初始化失败: {e}");
        }
    }

    dioxus::launch(app);
}
```

**关键点**：
1. 必须在 `dioxus::launch` **之前**设置 global subscriber
2. `tracing_android::layer("Tag")` 的 tag 是 logcat 中显示的 tag
3. `tracing_subscriber::Registry::default().with(layer)` 组合 layer
4. `dioxus::launch` 内部调 `initialize_default()`，检测到 `has_been_set()` 为 true 会跳过

## 日志宏使用规范

### 统一使用 `tracing::` 前缀

区分 `tracing` 日志和其他日志系统（如 `log::`），所有 tracing 日志宏必须加 `tracing::` 前缀：

```rust
use dioxus::logger::tracing;

// ✅ 正确
tracing::error!("file not found: {path:?}");
tracing::warn!("deprecated API used");
tracing::info!("request completed: {ms}ms");
tracing::debug!("internal state: {state:?}");

// ❌ 错误：裸宏无法区分是 tracing 还是 log
error!("file not found: {path:?}");
```

### 每个使用日志的文件必须添加 import

```rust
use dioxus::logger::tracing;
```

## 平台对照

| 平台 | 默认 subscriber | 输出目标 | 需要手动初始化? |
|------|----------------|---------|:---:|
| Web (wasm) | `tracing_wasm::WASMLayer` | 浏览器 console | 否 |
| Desktop | `FmtSubscriber` | stdout/stderr | 否 |
| Android | **无默认** | **需要 tracing-android** | **是** |
| Server | `FmtSubscriber` | stdout/stderr | 否 |

## 依赖关系

```
dioxus
  └── dioxus-logger
        ├── tracing (re-export → 项目不需要直接依赖)
        └── tracing-subscriber (registry + std + env-filter + fmt)
              └── 项目 Android 初始化需要 tracing-subscriber 的 Registry

项目额外依赖：
  ├── tracing-subscriber (仅 registry feature) — Android 初始化用
  └── tracing-android (cfg(android)) — logcat layer
```

## 常见错误

### 1. 直接依赖 tracing crate（冗余）

```toml
# ❌ 冗余，dioxus-logger 已 re-export
tracing.workspace = true

# ✅ 通过 dioxus::logger::tracing 使用
dioxus = { workspace = true }
```

### 2. Android 上日志不输出到 logcat

原因：未手动初始化 `tracing-android` layer，`dioxus::launch` 的默认 `FmtSubscriber` 输出到 stdout（Android 上不可见）。

### 3. 初始化顺序错误

必须在 `dioxus::launch` 之前设置 global subscriber。之后设置会返回 `Err(SetGlobalDefaultError)`。

### 4. 裸日志宏与 log crate 混淆

裸 `error!`/`warn!` 可能来自 `tracing` 也可能来自 `log`，必须用 `tracing::error!` 明确来源。