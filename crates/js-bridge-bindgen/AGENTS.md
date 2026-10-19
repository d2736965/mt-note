# AGENTS.md — js-bridge-bindgen

> 面向 AI 编程助手的开发指南。

## 项目概述

`js-bridge-bindgen` 是一个**通用库**，从 TypeScript 源码自动生成 Rust 类型安全绑定代码。它不耦合任何特定框架（如 Dioxus），通过 `JsBridge` trait 让使用者自行实现 JS 运行时桥接。

## 设计原理：一次注入，终身调用

核心思想是**把 npm 包当作 JS 运行时内的"微服务"**，Rust 只负责发号施令、不需要搬运数据。

### 三个阶段

```
阶段 1: 注入 JS Bundle（一次性，重）
  ensure_injected()
  → eval(整个 xxx.js)  // npm 包打包产物
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

| | 传统 RPC | js-bridge-bindgen |
|------|-----------|-------------------|
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

## 架构

```
TS 源码 (index.ts)
    │
    ▼
parser.rs  ──(oxc_parser AST)──▶  ParsedModule (types.rs)
    │                                    │
    ▼                                    ▼
validation.rs  ──(模式校验)──▶  ValidationResult
                                         │
                                         ▼
                                  codegen.rs  ──▶  js_api.rs (Rust 绑定代码)
```

### 模块职责

| 文件 | 职责 |
|------|------|
| `lib.rs` | 公开 API：`generate()`、`parse_from_str()`、`JsBridge` trait、`Config` |
| `types.rs` | 所有数据结构：`ParsedModule`、`InterfaceInfo`、`MethodSignature`、`RustLintAttr` 等 |
| `parser.rs` | 用 `oxc_parser` 解析 TS 源码 AST，提取导出签名、JSDoc 注释、`@rust-lint` 注解 |
| `codegen.rs` | 从 `ParsedModule` 生成 Rust 代码字符串，含 `emit_rust_lint_attrs`、`emit_doc_comment` 等辅助函数 |
| `config.rs` | `Config` 结构体：入口文件、输出目录、工厂对象名、实例变量映射 |
| `validation.rs` | 工厂模式校验规则（E001–E009、W001），确保 TS 源码符合约定 |
| `error.rs` | `Error`（thiserror）、`ValidationError`、`ValidationResult` 类型 |

## 核心概念

### 工厂模式约定

TS 源码必须遵循以下约定：

1. **`declare global` 块** — 扩展 `Window` 接口，声明全局工厂对象和实例变量
2. **工厂对象** — `Window` 上包含 `createXxx` 方法的对象（如 `__mtNote`）
3. **`export function`** — 每个工厂方法必须有对应的导出函数声明
4. **`export interface`** — 工厂方法返回类型必须有对应的导出接口
5. **实例变量** — `Window` 上的 `__xxx?: Interface` 类型必须与工厂方法返回类型一致

### 命名约定

- 工厂函数：`createXxx` → 实例变量：`__xxx`（去掉 `create` 前缀，首字母小写，加 `__` 前缀）
- 模块名：`createEditor` → `editor`（去掉 `create` 前缀，首字母小写）
- Rust 方法名：驼峰转 snake_case（`takeVirtualRange` → `take_virtual_range`）
- 嵌套路径：`render.setScale` → `render_set_scale`

### 生成的代码结构

```rust
pub mod editor {
    pub fn create<B: super::JsBridge>(...) { ... }
    pub fn take_virtual_range<B: super::JsBridge>() -> String { ... }
    pub fn destroy<B: super::JsBridge>() { ... }
}
pub struct EditorConfig { ... }
```

### `@rust-lint` 注解系统

在 TS 源码的 JSDoc 注释中使用 `@rust-lint` 为生成的 Rust 代码添加 lint 属性，支持多粒度：

```typescript
// 接口级 — 应用到 pub mod 块
/** @rust-lint allow(dead_code) */
export interface Editor { ... }

// 工厂函数级 — 应用到 pub fn create
/** @rust-lint expect(dead_code, reason = "由 Rust 侧调用") */
export function createEditor(): Editor { ... }

// 方法级 — 应用到具体方法
export interface Editor {
  /** @rust-lint expect(dead_code, reason = "由 Rust 侧调用") */
  destroy: () => void
}

// 多属性
/**
 * @rust-lint expect(dead_code, reason = "未使用")
 * @rust-lint allow(clippy::too_many_arguments)
 */
configure(...): void
```

解析逻辑在 `parser.rs` 的 `extract_rust_lint_attrs_map()`，生成逻辑在 `codegen.rs` 的 `emit_rust_lint_attrs()`。

## 开发流程

### 修改 TS 源码后

1. 修改 `crates/mobile/ts/src/index.ts`
2. 运行 `cargo test -p js-bridge-bindgen` 确保解析和生成正确
3. 运行 `python lints/scripts/check.py -p js-bridge-bindgen` 通过 lint

### 修改绑定生成逻辑后

1. 修改 `parser.rs` / `codegen.rs` / `types.rs` 等
2. 运行 `cargo test -p js-bridge-bindgen`（127+ 单元测试 + 18 集成测试）
3. 运行 `python lints/scripts/check.py -p js-bridge-bindgen`
4. 如果测试通过，`cargo build -p mt-note-mobile` 验证下游编译

### 添加新类型

在 `types.rs` 中添加新结构体/枚举时：
- 实现 `Debug, Clone`
- 考虑是否需要 `Default` 实现
- 在 `parser.rs` 中添加对应的 AST 提取逻辑
- 在 `codegen.rs` 中添加对应的代码生成逻辑
- 在 `tests/integration.rs` 中添加合成测试

### 添加新校验规则

在 `validation.rs` 中：
1. 在 `mod rules` 中添加规则编号常量
2. 实现校验函数
3. 在 `validate()` 主函数中调用
4. 在 `validation.rs` 的 `tests` 模块中添加测试

## 测试

### 单元测试（`src/` 内联）

- `parser.rs`：解析器测试（`test_parse_*`、`test_extract_*`）
- `types.rs`：类型转换测试（`test_is_*`、`test_to_rust_type_*`）
- `codegen.rs`：代码生成测试（`test_emit_*`）
- `validation.rs`：校验规则测试（`test_validate_*`）

### 集成测试（`tests/integration.rs`）

合成测试完整流程：手写 TS 源码 → `parse_from_str()` → `generate_code()` → `syn::parse_file()` 验证语法 → 断言关键代码片段。

每个测试包含：
- 完整的 TS 源码（`export interface` + `export function` + `declare global`）
- 调用 `parse_synthetic()` 和 `generate_code()`
- `assert_valid_rust_syntax()` 验证生成的 Rust 代码是合法语法
- `assert!()` 断言生成的代码包含特定字符串

### 运行测试

```bash
cargo test -p js-bridge-bindgen
```

## 依赖

- `oxc_parser` / `oxc_ast` / `oxc_allocator` / `oxc_span`（0.139）— TS/JS AST 解析
- `regex`（1）— JSDoc 注释和 `@rust-lint` 注解提取
- `thiserror` — 错误类型 derive
- `serde_json` — 生成代码中的 JSON 序列化
- `syn`（dev）— 集成测试中验证生成代码的 Rust 语法合法性
- `tempfile`（dev）— 测试中的临时文件

## 注意事项

- **禁止使用 `cargo check`**，必须用 `python lints/scripts/check.py`（clippy + dylint）
- 修改 `types.rs` 中的结构体字段后，必须同步更新所有 `Default` 实现、测试代码中的初始化、以及 `parser.rs` 中的 AST 提取逻辑
- `@rust-lint` 注解的提取正则位于 `parser.rs` 的 `extract_rust_lint_attrs_map()`，修改时需确保兼容 `export function`、`export interface`、普通方法签名等多种场景
- 集成测试使用 `syn::parse_file()` 验证生成代码语法，语法错误会导致测试失败