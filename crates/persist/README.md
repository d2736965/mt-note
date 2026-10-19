# Persist

类型安全持久化存储基础设施。

## 目录结构

```
src/
├── lib.rs          # crate 入口
├── backend.rs      # StorageBackend trait（平台 raw k:v 读写）
└── storage.rs      # PersistStorage<B>（JSON 序列化引擎）
```

## 架构

| 角色 | 职责 |
|------|------|
| `StorageBackend` | 平台实现 raw k:v 读写（不含 JSON），如 `load_raw`/`save_raw`/`remove_raw` |
| `PersistStorage<B>` | 封装后端，提供类型安全的 `load::<T>()`/`save(&t)`/`remove::<T>()`，自动 JSON 序列化 |

数据类型和 `Persistable` trait 由 `models` crate 提供。

平台只需实现 `StorageBackend`，`PersistStorage` 自动提供 JSON 序列化层：

```rust
let backend = AndroidBackend::new()?;
let storage = PersistStorage::new(backend);
storage.save(&theme)?;
let t = storage.load::<Theme>()?;
```

## 依赖

`models` + `serde` + `serde_json`。可同时用于 wasm 和原生编译目标。
