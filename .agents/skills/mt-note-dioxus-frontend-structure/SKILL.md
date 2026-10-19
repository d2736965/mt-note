---
name: "mt-note-dioxus-frontend-structure"
description: "Dioxus 工程目录结构、Stylance CSS Modules、class 规则、服务端分层。当涉及目录组织、组件/模块、样式方案时调用。（mt-note 项目自带版本）"
---

# Dioxus 全栈项目结构规范

## 核心原则

- **编译目标分离**：`frontend/` 编译为 wasm，`server/` 编译为原生，`shared/` 共用。但 workspace 中按 crate 分离（api、api-impl、models）更简洁
- **组件就近自治**：组件与样式同目录，`mod.rs` 入口
- **状态尽量下沉**：能用 `use_signal` 不放全局 store

## 目录结构

workspace 中每个平台 crate（web/desktop/mobile）内部采用统一布局（以 web 为例）：

```
crates/web/
├── src/
│   ├── frontend/
│   │   ├── components/      # 可复用 UI 组件（button/mod.rs + button.module.scss）
│   │   ├── views/           # 页面级组件（home/about/settings，各含 mod.rs + xxx.module.scss）
│   │   ├── store/           # 平台持久化与平台特有状态
│   │   │   ├── mod.rs
│   │   │   └── web_storage.rs   # web 存储后端
│   │   ├── hooks/
│   │   ├── app.rs           # 根布局组件
│   │   ├── app.module.scss  # 根布局样式
│   │   ├── router.rs
│   │   └── mod.rs
│   └── main.rs              # dioxus::launch(app)
├── assets/styles/
│   ├── dist/
│   │   └── bundle-web.scss  # stylance 输出（build.rs 生成）
│   ├── theme.scss
│   └── variables.scss
├── Cargo.toml
├── Dioxus.toml
└── build.rs
```

### 平台差异（store/ 与 assets）

各平台 `frontend/store/` 因持久化机制不同：
- **web**：`store/web_storage.rs`（localStorage）
- **desktop**：`store/desktop_storage.rs`（dirs 本地文件）
- **mobile**：`store/storage/`（`android.rs` + `jni.rs` + `mod.rs`，JNI 桥接 getFilesDir）+ `store/top_app_bar_state.rs`（移动端 TopAppBar 状态）+ `store/bottom_bar_state.rs`（移动端 BottomBar 状态）

**共享应用状态 `AppState`（主题等）在 `ui` crate**（`ui/src/store/app_state.rs`），各平台通过 `ui::hooks::use_app_state` 引用，**不放各平台 store/**。

`reset.scss` 在 **`ui` crate**（`ui/assets/styles/reset.scss`），不在各平台；各平台 `assets/styles/` 只有 `dist/bundle-*.scss`（stylance 输出）+ `theme.scss` + `variables.scss`。

服务端逻辑拆分为独立 workspace crates：

```
crates/api/          # 纯库：#[post]/#[get] 宏 + 共享类型（编译目标无关）
crates/api-impl/     # 服务端业务逻辑（tokio、db、kameo）
crates/models/       # 共享数据类型
crates/ui/           # 共享 UI 组件
crates/persist/      # 持久化存储基础设施
```

## 服务端分层

```
crates/api/src/           # 纯库：#[post]/#[get] 宏 + 共享类型
                          #   dioxus = { features = ["fullstack"] }
                          #   server = ["dioxus/server"]

crates/api-impl/src/      # server-only 业务逻辑
  ├── services/           # 业务逻辑层
  └── db/                 # 数据访问层
```

依赖方向 `api → api-impl`，不可反向。`#[post]` 宏自动将函数体包在 `#[cfg(feature = "server")]`，函数体内调用 `api_impl` 安全。

## 样式方案：Stylance + Manganis

| 职责 | 工具 |
|------|------|
| 类名处理、SCSS 编译、哈希化 | **Stylance** |
| 跨 crate 资产收集 | **Manganis** `asset!()` |

```toml
# Cargo.toml
[dependencies]
stylance = "0.8"
manganis = "0.7"
dioxus = { features = ["asset"] }

[build-dependencies]
stylance-cli = "0.8"

[package.metadata.stylance]
output_file = "./assets/styles/dist/bundle-web.scss"
scss_prelude = '''@use "../theme"; @use "../variables" as *;'''
```

```rust
// build.rs
fn main() {
    let config = stylance_cli::Config::load(manifest_dir.into()).expect("...");
    stylance_cli::run(&config).expect("Stylance failed");
}
```

```rust
// 组件
stylance::import_style!(styles, "button.module.scss");
button { class: styles::btn }

// 多类名
button { class: stylance::classes!(styles::btn, styles::primary) }
```

### class 属性规则

**`class:` 值只允许 stylance 常量，禁止任何字符串字面量：**

```rust
// ❌ 禁止
class: "active"
class: format!("{} active", styles::btn)

// ✅ 正确
class: stylance::classes!(styles::btn, styles::primary)

// ✅ 动态类名 — match 返回 stylance 常量
let class = match theme { Light => styles::light, Dark => styles::dark };
class: stylance::classes!(styles::layout, class)
```

全局类名（如 `theme-light`）也必须在 `.module.scss` 中定义含完整变量的样式，通过 stylance 常量引用。Stylance 会给类名加哈希后缀，不同文件同名类永不冲突。

### Workspace 跨 crate 样式

UI 组件库用 `asset!()` 注册 bundle，平台包通过 `ui::STYLES` 引用：

```rust
// ui/src/lib.rs
#[used] pub static STYLES: dioxus::prelude::Asset = asset!("/assets/styles/dist/bundle-ui.scss");

// web/src/app.rs
document::Stylesheet { href: ui::STYLES }
document::Stylesheet { href: asset!("/assets/styles/dist/bundle-web.scss") }
```

`ui::STYLES` 必须在平台 bundle 之前。Manganis `__ASSETS__` 段自动收集所有 crate 的资产。

## 状态管理

优先本地状态，避免全局 store 膨胀：

| 状态类型 | 位置 |
|---------|------|
| 组件 UI 状态（open/input） | 组件内 `use_signal` |
| 页面级数据（列表/表单） | `use_signal` 或 `use_resource` |
| 跨页面共享（用户/主题） | `frontend/store/` |
| 持久化数据 | `GlobalSignal` + localStorage |

## 文件命名

- 目录和文件：**snake_case**
- 入口文件：`mod.rs`
- 样式文件：`组件名.module.scss`，与 `mod.rs` 同目录
- 一个文件一个组件