# UI

跨平台共享 UI 组件库。提供各平台共用的组件、全局状态和自定义 hooks。

## 目录结构

```
src/
├── lib.rs                         # crate 入口，导出 STYLES
├── components/
│   ├── hello_world/               # 示例组件
│   │   ├── mod.rs
│   │   └── hello_world.module.scss
│   ├── icons/                     # 图标组件
│   └── overlay/                   # 遮罩层组件
│       ├── mod.rs
│       └── overlay.module.scss
├── store/
│   ├── mod.rs
│   └── app_state.rs               # AppState 全局状态定义 + provide()
└── hooks/
    ├── mod.rs
    └── use_app_state.rs           # 消费 AppState 的自定义 hook
```

## 依赖原则

此 crate 被 Web、Desktop、Android 三个平台同时引用，**不得添加任何平台特定依赖**（如 `web-sys`、`ndk-context`）。平台特定代码放在各自平台包的 `frontend/store/` 目录下。

## 样式导出

通过 Manganis `asset!()` 宏将 Stylance 编译产物注册到 `__ASSETS__` 段，平台包只需 `use ui::STYLES` 即可自动获得全部样式：

```rust
// crates/ui/src/lib.rs
pub static STYLES: dioxus::prelude::Asset =
    dioxus::prelude::asset!("/assets/styles/bundle-ui.scss");
```

```rust
// 平台包 app.rs 中按顺序加载
rsx! {
    document::Stylesheet { href: ui::STYLES }           // 先：组件库默认样式
    document::Stylesheet { href: asset!("/assets/...") }  // 后：平台定制覆盖
}
```

## 状态管理

`AppState` 存放跨页面共享的全局状态（当前只有 theme），通过 `use_context_provider` 提供，`use_app_state` hook 消费。页面私有状态放在各自页面组件内，不放入全局 store。
