# rust-dylint

项目自定义 dylint 规则库，补充 clippy 无法覆盖的强制规范检查。通过 Cargo 的 `[workspace.metadata.dylint]` 被宿主项目加载，与 clippy 配合构成完整 lint 体系。

## 使用方式

### 1. 在宿主项目加载规则库

在项目根 `Cargo.toml` 配置（workspace 与单包项目均适用）：

```toml
[workspace.metadata.dylint]
libraries = [{ path = "lints/rust-dylint" }]
```

- **workspace 项目**：写在根 `Cargo.toml`（已含 `[workspace]`）。
- **单包项目**：根 `Cargo.toml` 仅有 `[package]` 时，`[workspace.metadata.dylint]` 会隐式声明 workspace 根，无需额外 `[workspace]` 节。

随后将本目录（`lints/rust-dylint/`）与 `lints/scripts/check.py` 一并放入项目。

### 2. 运行检查

直接用 dylint：

```bash
cargo dylint --all
```

或用 `lints/scripts/check.py`（自动检测 workspace 成员、推断 target/feature，依次运行 clippy 与 dylint，合并输出 rustc JSON 诊断）：

```bash
python lints/scripts/check.py          # 全项目，人类可读输出
python lints/scripts/check.py --json   # JSON 输出，供 rust-analyzer 解析
python lints/scripts/check.py -p PKG   # 单包模式
```

> `check.py` 兼容 workspace 与单包：单包项目（无 `[workspace]` members）会自动回退到检查根 package 自身。

VS Code 集成（rust-analyzer）：

```json
"rust-analyzer.check.overrideCommand": [
    "python3", "${workspaceFolder}/lints/scripts/check.py", "--json"
]
```

## Lint 规则

规则名前缀即其级别：`forbid_*` 为 Forbid（连 `#[allow]`/`#[expect]` 都无法覆盖），`deny_*` 为 Deny（可被 `#[expect]` 豁免）。

### Forbid 级别

#### `forbid_underscore_must_use`

禁止用下划线前缀变量（`let _ =` / `let _x =`）静默丢弃值。

`let _x = must_use_value` 会绕过编译器 `unused_must_use` 检查——值被绑定到变量后编译器认为已“使用”。clippy 的 `let_underscore_must_use` 只检查单独的 `_`，不检查 `_named` 模式。

```rust
// ❌ 错误
let _send = tx.unbounded_send(msg);

// ✅ 正确：显式处理错误
if let Err(_) = tx.unbounded_send(msg) {}
```

#### `forbid_mpsc_channel`

非 wasm32 目标禁止使用任何 mpsc channel 构造函数。桌面/移动端应使用 kameo Actor 模型管理并发状态；wasm32 目标（kameo 不可用）允许 `futures_channel::mpsc`。

```rust
// ❌ 错误（非 wasm32 目标）
let (tx, rx) = std::sync::mpsc::channel();
let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
let (tx, rx) = futures_channel::mpsc::unbounded();

// ✅ 正确：使用 kameo Actor 模型
```

#### `forbid_dropped_result`

禁止丢弃 `Result` 或其错误值。涵盖：`.ok()`/`.err()`/`.is_err()`/`.is_ok()` 后用 `;` 丢弃、`Err(_) =>` 用 `_` 丢弃错误、`if result.is_err() {...}` 销毁错误值、`let Ok(...) = ... else` 丢弃。优先用 `?` 传播，无法传播时用 `if let Err(e) =` 显式处理。

#### `forbid_bare_return`

禁止裸 `return;`（无返回值的早期退出）。正常函数末尾隐式返回 `()`，显式 `return;` 只在吞错误的早期退出中出现。函数应返回 `Result`，用 `?` 传播。

#### `forbid_empty_block`

禁止空的 `{}` 代码块（`if`/`if let` 的 then/else 分支、`match` arm body）。空块等于隐式丢弃状态。循环体（`loop`/`while`/`for`）的空块不在此列——无限循环、自旋等待有意义。

#### `forbid_unused_variables_attr`

禁止用 `#[allow(unused_variables)]` / `#[expect(unused_variables)]` 绕过未使用变量检查。应删除未使用的变量，而非用属性抑制。

### Deny 级别

#### `deny_lib_console_output`

库 crate 禁止向控制台输出（`println!`/`eprintln!`/`dbg!` 及 `log` 宏 `info!`/`warn!`/`error!` 等）。库不知道下游运行环境——控制台输出会污染 TUI 界面、管道数据、日志格式。优先用 `?` 传播。

**例外**：`tests/` 目录与 `#[cfg(test)]` 模块（编译为 bin 目标，自动豁免）。

设为 Deny 而非 Forbid，是为了让特定入口能用 `#[expect(deny_lib_console_output)]` 豁免——例如 Android 入口 `android_main` / `logger::init()`（日志走 logcat，桌面端 `eprintln!` 仅作 `env_logger` 初始化失败的 fallback）。Forbid 级别无法被 `#[expect]` 覆盖，故此处必须用 Deny。

## 技术细节

- 基于 [dylint](https://github.com/trailofbits/dylint)（`dylint_linting = "6.0.1"`）
- 使用 `#![feature(rustc_private)]`，需 nightly 工具链（由 `rust-toolchain` 指定版本）
- 编译产物为 `cdylib`（`.dll`/`.so`），由 `cargo dylint` 加载到 rustc 驱动中运行
- 因需链接编译器内部 crate，`cargo clippy` 无法直接运行（会报链接错误），lint 检查通过 `cargo dylint` 间接完成
- `clippy.toml` 启用 `allow-unwrap/expect/panic-in-tests`（unstable 特性，依赖 nightly + `-Z unstable-options`）

## 开发

工具链由 `rust-toolchain` 自动指定，无需手动传 `+nightly`。

```bash
cargo build
cargo clippy --all-features -- --no-deps -Dwarnings
```

## 添加新规则

1. 在 `src/lib.rs` 中用 `rustc_session::declare_lint!` 声明 lint（变量名前缀与级别一致：`FORBID_*`/`DENY_*`）
2. 用 `rustc_session::declare_lint_pass!` 声明 lint pass
3. 实现 `LateLintPass` / `EarlyLintPass` trait（选择适当的 `check_*` 方法）
4. 在 `register_lints` 函数中注册 lint 与 pass
5. 更新本 README 的规则列表与 `Cargo.toml` 的 `description`
