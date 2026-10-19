# Dioxus Native 平台非冒泡事件 Bug 技术参考

## 问题概述

Dioxus 0.7 的 Desktop/Mobile 平台（native interpreter）上，所有 `bubbles=false` 的 DOM 事件（`scroll`、`focus`、`blur` 等）的 Dioxus 事件处理器**不会触发**。Web (wasm) 平台不受影响。

## 根因分析（源码追踪）

### 因果链

```
1. bubbles.rs: "scroll" => false
   ↓
2. core.ts createListener(): bubbles=false → 在元素上直接注册 addEventListener ✅ 注册正确
   ↓
3. native.ts 第158-159行: handler 硬编码 bubbles=true
   const handler = (event) => this.handleEvent(event, event.type, true);
                                                          ^^^^ 硬编码！
   ↓ ❌ bubbles 参数被篡改
4. native.ts handleEvent(): 将 bubbles=true 传给 sendSerializedEvent
   body = { name, data, element, bubbles: true }  ← 应该是 false
   ↓
5. webview.rs: 从 JS 接收 HtmlEvent { bubbles: true }
   ↓
6. events.rs: Event::new(data, propagates=true)  ← propagates 被设为 true
   ↓
7. runtime.rs handle_event():
   if event.propagates() {           ← true，走冒泡分支
       handle_bubbling_event(...)    ← 错误！scroll 应该走 non_bubbling
   } else {
       handle_non_bubbling_event(...)
   }
   ↓
8. handle_bubbling_event(): 从 target 元素向上遍历 DOM 树查找监听器
   → 但 scroll 监听器注册在 target 元素上（local 监听器）
   → 冒泡遍历逻辑与 local 监听器的注册方式不匹配
   → 找不到监听器 → 事件丢失！
```

### 关键源码位置

| 文件 | 位置 | 说明 |
|------|------|------|
| `core-types/src/bubbles.rs` | `event_bubbles()` | 定义事件冒泡属性，`scroll => false` |
| `interpreter/src/ts/core.ts` | `createListener()` | bubbles=false 时在元素上注册 ✅ 正确 |
| `interpreter/src/ts/native.ts` | 第158-159行 | handler 硬编码 `bubbles=true` ❌ BUG |
| `interpreter/src/ts/native.ts` | `handleEvent()` | 将 bubbles 传给 `sendSerializedEvent` |
| `mobile/src/webview.rs` | `HtmlEvent` | 反序列化 JS 传来的 bubbles 字段 |
| `core/src/events.rs` | `Event::new()` | 用 propagates 控制分发路径 |
| `core/src/runtime.rs` | `handle_event()` | 根据 propagates 选择冒泡/非冒泡分发 |

### 为什么 Web 平台不受影响

Web 平台走 `web.ts` 代码路径，直接在浏览器事件回调中读取 `event.bubbles`，没有硬编码问题。此 bug 只影响 Desktop/Mobile（native interpreter）。

### 上游修复方案

`native.ts` 第158-159行，一行修复：

```diff
- const handler: EventListener = (event) => this.handleEvent(event, event.type, true);
+ const handler: EventListener = (event) => this.handleEvent(event, event.type, event.bubbles);
```

## 受影响的完整事件列表

`core-types/bubbles.rs` 中 `bubbles=false` 的所有事件均受影响：

| 分类 | 事件 | 说明 |
|------|------|------|
| 滚动 | `scroll`、`scrollend` | 滚动/滚动结束 |
| 焦点 | `focus`、`blur` | 获得/失去焦点 |
| 鼠标 | `mouseenter`、`mouseleave` | 鼠标进入/离开 |
| 指针 | `pointerenter`、`pointerleave` | 指针进入/离开 |
| 拖拽 | `dragenter`、`dragexit` | 拖拽进入/退出 |
| 表单 | `cancel` | 取消 |
| 布局 | `resize` | 尺寸变化 |
| 切换 | `toggle`、`beforetoggle` | 切换状态 |
| 媒体 | `abort`、`canplay`、`canplaythrough`、`durationchange`、`emptied`、`ended`、`error`、`loadeddata`、`loadedmetadata`、`loadstart`、`load`、`pause`、`play`、`playing`、`progress`、`ratechange`、`seeked`、`seeking`、`stalled`、`suspend`、`timeupdate`、`volumechange`、`waiting` | 媒体播放生命周期 |

**冒泡事件（onclick、oninput、onchange、onkeydown 等）不受影响**，可以正常使用 Dioxus 事件系统。

## APP 层绕行方案

### 方案：JS 侧原生监听 + js-bridge-bindgen

不依赖 Dioxus 事件系统，改用 JS 侧原生 `addEventListener` + `js-bridge-bindgen` 通信。

#### JS 侧

```typescript
// scroll.ts — JS 侧原生监听，不经过 Dioxus 事件系统
export function initHomeScroll(): void {
  const el = document.getElementById("home_view");
  if (el) {
    el.addEventListener("scroll", () => {
      // 存储滚动位置到 sessionStorage
      sessionStorage.setItem("home_scroll_top", String(el.scrollTop));
    });
  }
}

export function setHomeScrollTop(scrollTop: number): void {
  const el = document.getElementById("home_view");
  if (el) el.scrollTop = scrollTop;
}
```

#### Rust 侧

```rust
// 通过 js-bridge-bindgen 调用 JS 函数
js_api::scroll::init_home_scroll::<DioxusBridge>();
js_api::scroll::set_home_scroll_top::<DioxusBridge>(target);
```

#### 适用场景

此绕行方案适用于所有受影响的非冒泡事件：
- `scroll`/`scrollend`：用 JS 原生监听 + bridge 传递滚动位置
- `focus`/`blur`：用 JS 原生监听 + bridge 传递焦点状态
- `mouseenter`/`mouseleave`：用 JS 原生监听 + bridge 传递悬停状态
- `resize`：用 JS 原生监听 + bridge 传递尺寸
- 媒体事件：用 JS 原生监听 + bridge 传递播放状态

模式统一：**JS `addEventListener` → `js-bridge-bindgen` → Rust 处理**。

**禁止使用裸 JS**（`document::eval` 内联字符串、手动 JSON 序列化），必须使用 `js-bridge-bindgen` 获得类型安全的 Rust 绑定。`js-bridge-bindgen` 的完整用法见 SKILL.md「js-bridge-bindgen：TS → Rust 类型安全绑定」章节。