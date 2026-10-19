# Models Macros

`models` crate 的派生宏。提供 `#[derive(Persistable)]` proc macro。

## 用法

```rust
use models::Persistable;

#[derive(Persistable)]
#[persistable(key = "theme")]
pub enum Theme {
    #[default]
    Light,
    Dark,
}
```

展开后自动实现 `Persistable` trait（设置 `KEY = "theme"`）。若枚举变体标注 `#[default]`，同时生成 `impl Default`。

## 依赖

仅依赖 `proc-macro` 标准库，无外部运行时依赖。
