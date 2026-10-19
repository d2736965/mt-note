# Models

共享数据模型与持久化类型标记。

## 目录结构

```
src/
├── lib.rs                 # crate 入口
├── persistable.rs         # Persistable trait（类型级存储键声明）
└── data/
    ├── mod.rs
    └── theme.rs           # Theme 枚举（Light / Dark）+ #[derive(Persistable)]
```

## 职责

| 内容 | 说明 |
|------|------|
| `Persistable` | 标记可持久化类型，通过 `#[derive(Persistable)]` 派生，`#[persistable(key = "...")]` 声明存储键 |
| 数据类型 | `Theme` 等纯数据结构 |

存储引擎（`StorageBackend`、`PersistStorage`）由 `persist` crate 提供。

## 依赖

仅依赖 `serde`，可同时用于 wasm 和原生编译目标。
