# TS Bridge

Dioxus WebView 与原生层之间的 JS 桥接包。整套管线由 `build.rs` 编排，Rust 侧通过 `js_bridge.rs` 在运行时注入。

## 设计原理：一次注入，终身调用

核心思想是**把 npm 包当作 JS 运行时内的"微服务"**，Rust 只负责发号施令、不需要搬运数据。

### 三个阶段

```
阶段 1: 注入 JS Bundle（一次性，重）
  ensure_injected()
  → eval(整个 mt-note-mobile-ts.js)
  → npm 包代码在 JS 运行时中初始化
  → 工厂对象挂载到 window.__f

阶段 2: 创建实例（每个 API 一次，轻）
  createEditor({ textareaId: "#a", ... })
  → eval("__f.createEditor({...})")
  → npm 包实例化，挂载到 window.__editor
  → 参数通过 format! 内联到 JS 字符串中

阶段 3: 调用方法（高频，极轻）
  editor.takeVirtualRange()
  → eval("__editor.takeVirtualRange()")
  → 无参数或少量序列化参数
  → 返回 String/JSON
```

### 与传统 RPC 的对比

| | 传统 RPC | js-bridge |
|------|-----------|----------|
| 每次调用数据量 | 请求头 + 方法名 + 参数 + 序列化开销 | 一行 JS 字符串 |
| 对象生命周期 | Rust 侧管理，来回传递 | 常驻 JS 运行时 |
| 大数据传输 | 每次都要传 | 只在 `create` 时传一次 |
| 类型安全 | 运行时校验 | 编译期由 TS 类型推导 |

本质上是把 **RPC 的开销从"每次调用"降到了"仅初始化一次"**：

- 第一次 `eval(整个 xxx.js)` 把 npm 包的所有 API 代码一次性挂载到 JS 运行时
- 后续每次调用只是 `eval("__editor.doWork()")` —— 函数已在 JS 内存中，只是调一下
- 后续都是 JS 运行时内部的函数调用，Rust 只负责发指令，无需序列化/反序列化往返

### 具体示例：复杂 API，轻量参数

考虑一个典型的 JS API，其函数体可能非常庞大：

```ts
// TS 侧：api 函数体可能包含数百行复杂逻辑
some_api(elementRef: HTMLElement): SomeResult {
  // 内部可能涉及：
  //   - DOM 遍历与操作
  //   - 复杂的计算逻辑
  //   - 第三方库调用
  //   - 事件监听绑定
  //   ...数百行代码...
}
```

传统 RPC 每次调用都需要把整个请求体序列化传输，而 js-bridge 的方案：

```
传统 RPC（每次调用，重）:
  Rust ──[序列化 { method: "some_api", params: [elementRef序列化], ... }]──▶ JS
  
js-bridge（每次调用，轻）:
  Rust ──eval("__editor.some_api(elementRef)")──▶ JS
```

JS 函数体已经在阶段 1 注入到 JS 运行时中，Rust 只需提供**轻量的参数**（如 `elementRef`），
无需传输整个函数逻辑。函数体越复杂，节省的传输量越大。

## 完整管线

```
ts/src/index.ts          ← 手写的 TS 源码（Bridge 接口 + 事件队列）
    │
    ▼ ① pnpm build (Vite)
ts/dist/mt-note-mobile-ts.js     ← IIFE 格式的 JS bundle
    │
    ├──▶ ② js-bridge-bindgen::generate()
    │    OUT_DIR/js_api.rs   ← 从 TS 类型自动生成的 Rust 绑定代码
    │
    └──▶ ③ include_str!("../../ts/dist/mt-note-mobile-ts.js")
         js_bridge.rs        ← 编译时嵌入 bundle，运行时注入 WebView
```

### 各组件职责

| 组件 | 文件 | 何时运行 | 职责 |
|------|------|----------|------|
| Vite | `ts/` | `build.rs` → `pnpm build` | 打包 TS 源码为 IIFE bundle |
| js-bridge-bindgen | `crates/js-bridge-bindgen/` | `build.rs` 在 Vite 之后 | 解析 TS AST，生成类型安全的 Rust 调用代码 |
| build.rs | `crates/mobile/build.rs` | `cargo build` / `dx bundle` | 编排：Stylance → pnpm install/build → codegen |
| js_bridge.rs | `crates/mobile/src/frontend/js_bridge.rs` | 编译时 + 运行时 | `include_str!` 嵌入 bundle，运行时注入 WebView |

## 项目结构

```
ts/
├── src/
│   └── index.ts         # Bridge 接口定义 + 事件队列实现
├── dist/
│   └── mt-note-mobile-ts.js     # Vite 构建产物 (IIFE)
├── package.json
├── tsconfig.json
├── vite.config.ts
└── pnpm-lock.yaml
```

## 脚本

| 命令 | 说明 |
|------|------|
| `pnpm build` | Vite 构建 → `dist/mt-note-mobile-ts.js` |
| `pnpm typecheck` | tsgo 类型检查 |
| `pnpm lint` | oxlint 检查 |
| `pnpm lint:fix` | oxlint 自动修复 |
| `pnpm format` | prettier 格式化 |
| `pnpm format:check` | prettier 格式检查 |

## Bridge 接口

```ts
interface Bridge {
  greet: (name: string) => string;
  waitForEvent: (eventName: string) => Promise<string>;
  cleanupAll: () => void;
  destroy: () => void;  // 由 Rust 侧调用
}
```

### 方法说明

- **`greet`** — 示例方法，供 Rust 侧测试调用
- **`waitForEvent`** — 异步等待 DOM 事件。Rust 侧通过 `CustomEvent` 发消息，JS 侧 `await` 接收
- **`cleanupAll`** — 清空所有事件队列（不移除 listener）
- **`destroy`** — 完全销毁，清空队列 + 移除 listener

## 事件队列机制

```
Rust (eval)                          JS (Bridge)
    │                                    │
    │  dispatchEvent(CustomEvent)        │
    │ ─────────────────────────────────▶ │ listener 收到
    │                                    │ ├─ 有等待者 → resolve(json)
    │                                    │ └─ 无等待者 → 入队
    │                                    │
    │  waitForEvent("event-name") ──────▶│
    │                                    │ ├─ 队列有数据 → 立即返回
    │                                    │ └─ 队列为空 → 入 resolvers 等待
    │  ◀── Promise<json> ────────────── │
```

## Rust 侧集成

```rust
// build.rs — 构建时自动：
//   1. Stylance（CSS Modules → Rust）
//   2. pnpm install/build（TS → IIFE bundle）
//   3. js-bridge-bindgen（TS 类型 → Rust 绑定代码）
// js_bridge.rs — include_str! 加载产物，注入 WebView
const BUNDLE_JS: &str = include_str!("../../ts/dist/mt-note-mobile-ts.js");
```

构建流程：`dx bundle` → `build.rs` → `pnpm build` → `js-bridge-bindgen` → `include_str!` 嵌入产物。

## 静态资源处理（字体、PDF worker 等）

Vite 的 IIFE 构建模式只打包 JS 逻辑，**不会自动打包** npm 依赖中的静态资源（字体文件、PDF.js worker、图片等）。

### 限制：`asset!()` 不能引用 `node_modules/`

Dioxus 的 `asset!()` 宏**不允许**直接引用 `node_modules/` 下的文件，因此不能这样写：

```rust
// ❌ 不允许：asset! 不能引用 node_modules
const FONT: &str = asset!("./ts/node_modules/pdfjs-dist/cmaps/");
```

### 方案：build.rs 复制到 `ts/dist/`，保持包结构

`build.rs` 在 `pnpm build` 之后，将 npm 包中的静态资源按**包名分组**复制到 `ts/dist/`，保持原有目录结构：

```
ts/dist/
├── mt-note-mobile-ts.js              ← Vite 构建产物
├── pdfjs-dist/               ← 从 node_modules/pdfjs-dist/ 复制
│   ├── cmaps/
│   │   └── ...
│   └── standard_fonts/
│       └── ...
└── @some-pkg/                ← 从 node_modules/@some-pkg/ 复制
    └── fonts/
        └── font.woff2
```

然后在 Rust 代码中通过 `asset!()` 引用 `ts/dist/` 下的路径：

```rust
// ✅ 正确：asset! 引用 ts/dist 下已复制的资源
const CMAPS: &str = asset!("/ts/dist/pdfjs-dist/cmaps/");
```

### build.rs 复制逻辑示例

```rust
// 在 pnpm build 之后，js-bridge-bindgen 之前执行
fn copy_npm_assets(ts_dir: &str) {
    // 按包名分组，保持原有结构
    let assets: &[(&str, &[&str])] = &[
        ("pdfjs-dist", &["cmaps", "standard_fonts"]),
        // ("@some-pkg", &["fonts"]),
    ];
    for (pkg, subdirs) in assets {
        let src_base = format!("{ts_dir}/node_modules/{pkg}");
        let dst_base = format!("{ts_dir}/dist/{pkg}");
        if !std::path::Path::new(&src_base).exists() {
            continue;
        }
        for sub in *subdirs {
            let src = format!("{src_base}/{sub}");
            let dst = format!("{dst_base}/{sub}");
            if std::path::Path::new(&src).exists() {
                copy_dir(&src, &dst);
            }
        }
    }
}
```

当前项目暂未使用 npm 静态资源依赖，上述逻辑已预留但未启用。