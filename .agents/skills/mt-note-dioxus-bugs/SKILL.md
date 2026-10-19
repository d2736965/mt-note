---
name: "mt-note-dioxus-bugs"
description: "Dioxus 0.7 Android 已知缺陷记录（APP 层面无解或需绕行）。当用户遇到 Dioxus Android 端 UI 卡死/崩溃/Crash、快速返回-重开后菜单无响应、FORTIFY 退出崩溃、或 onscroll/onfocus/onblur 等非冒泡事件不触发时调用。"
---

# Dioxus 0.7 Android 已知缺陷

Dioxus 0.7.10 + wry 0.53.5 + tao 0.34.8 的 Android 支持尚不成熟。以下缺陷已经过源码追踪、CDP 验证、logcat 排查，**在 app 代码层面无法修复**。

## 缺陷 1：VirtualDom 事件静默丢失（菜单卡死）

### 症状

快速返回键 → 重开应用 2-6 次后，所有按钮/菜单点击无响应。但：
- 页面内容正常显示（非白屏）
- 触摸事件正常注册（MIUIInput MotionEvent）
- 无任何 panic 或 crash 日志

### 验证证据

CDP 远程调试确认：

```
点击 button → XHR POST /__events 200  ← Dioxus 收到了事件
            → {"preventDefault":false}  ← 事件已派发
            → DOM 无变化               ← VirtualDom 不产生 mutations
```

`location.reload()` 无法恢复（排除 WebSocket 断连）。

### 根因分析

Dioxus 的 `generational-box` 信号订阅系统在 Android Activity 快速生命周期切换（onPause/onStop/onResume）时内部损坏。事件处理器的信号写入不再触发 scope 的脏标记，VirtualDom 不重渲染。

非锁中毒（XHR 有 200 响应），非两个 VirtualDom 竞争（进程未退出时也发生）。

### 尝试过的修复

| 方案 | 结果 |
|------|------|
| GuardWebViewClient 兜底 panic | 无 panic 可兜（此路径不 panic） |
| `onDestroy` 杀进程（killProcess） | 无效——卡死发生在进程内，不触发 onDestroy |
| ndk-context panic hook + use_effect 迁移 | 修复了**另一个** panic 路径，不修复此问题 |
| `location.reload()` | 无效——排除 JS 侧问题 |

### 结论

**Dioxus 框架缺陷，APP 层面无解。** 需等待上游修复 Android 生命周期与信号系统的集成。

### 变体：handleRequest panic（卡死后 SIGABRT）

缺陷 1 记录的是"静默卡死、无 panic"，但 2026-06-23 实测发现**同一根因家族存在 panic 变体**：卡死后下一次 WebView 请求处理时 `handleRequest` panic，进程 SIGABRT。

**触发场景**：`startActivity` 跳转系统设置（授权页）→ 返回应用 → 点击 UI 触发 `use_resource` restart。本质上仍是 `onPause`/`onResume` 生命周期切换导致运行时状态损坏。

**验证证据**（dropbox `data_app_native_crash`，logcat `-b crash`）：

```
pid: 27994, tid: 30231, name: ThreadPoolForeg
signal 6 (SIGABRT), code -1 (SI_QUEUE)
      #09 panic_cannot_unwind          ← extern "C" 边界不能 unwind
      #10 Java_dev_dioxus_main_RustWebViewClient_handleRequest  ← wry 的 shouldInterceptRequest JNI 桥
```

CDP 同时确认缺陷 1 的典型症状（事件 200 但 DOM 无变化）在 panic 前就已出现。

**与缺陷 1 的关系**：根因同源（Android 生命周期 → Dioxus 运行时状态损坏），但表现更严重——不只是卡死，还会崩溃。panic 发生在 wry 生成的 JNI 函数内，APP 代码无法拦截（`extern "C"` 边界 panic 直接 `std::process::abort`）。

**panic 消息捕获失败**：`panic_hook`（直写 `__android_log_write`）未能在 abort 前刷出消息——`std::process::abort` 在 `extern "C"` 线程上执行过快，logcat 写入被截断。tombstone 不记录 Rust panic 消息（只记录 `si_value`，而 Rust abort 不传消息）。

**APP 层缓解**：减少不必要的 `use_resource` cancel/restart（如移除已授权后的无条件轮询），降低踩中损坏路径的概率。但不能根除。

---

## 缺陷 2：FORTIFY destroyed mutex（进程退出崩溃）

### 症状

退出应用时 SIGABRT：

```
FORTIFY: pthread_mutex_lock called on a destroyed mutex (0x7ea9b6eed8)
```

地址 `0x7ea9b6eed8` 跨进程稳定，说明是全局 static mutex。

### 根因分析

tao 在 Activity 销毁时调用 `std::process::exit`，在 bionic 的 `__cxa_finalize` 阶段（析构全局对象），tokio worker 线程上的 mailbox task（如 DataFetchActor）仍持有 mutex 锁。mutex 的析构和 worker 的 lock 产生竞态。

### 与缺陷 1 的关系

二者相互独立：
- 缺陷 1：**存活期内**信号系统损坏
- 缺陷 2：**退出时**进程级析构竞态

### 结论

**tao/wry 的 Android 事件循环未正确停止，APP 层面无解。**

---

## 缺陷 3：ndk-context 在 Activity 销毁时 panic（已可防御）

### 症状

panic hook 捕获：

```
RUST PANIC: android context was not initialized
```

### 根因

`tao::ndk_glue::destroy()` → `release_android_context()` 将 `ANDROID_CONTEXT` 置为 `None`，此后 Dioxus 清理渲染时组件函数体仍会执行 → 如果函数体中调用了 `android_context()` → panic。

### 防御措施

**已在 `pub-trade-mobile` 中实施**（commit `1dc41ab`）：

1. **panic_hook.rs**：直写 Android logcat 捕获 panic（绕过 `log` crate 规范限制）
2. **home/mod.rs**：`AndroidBackend::new()` 从组件函数体移到 `use_effect` 回调（effect 在 scope 销毁时不执行）

详见 `mt-note-dioxus-android` skill 的 `ndk-context.md` → panic 窗口与 Dioxus 避坑指南。

### 残余风险

`std::process::exit` 的 FORTIFY 竞态（缺陷 2）不由此修复，因为 exit 发生在 destroy 之后，与 ndk-context 无关。

---

## 缺陷 4：Native 平台非冒泡事件（scroll/focus/blur 等）onscroll 不触发

### 症状

在 Desktop/Mobile（native WebView）上，`onscroll`、`onfocus`、`onblur` 等 Dioxus 事件处理器永远不会被调用。Web (wasm) 平台正常。

### 受影响事件

`core-types/bubbles.rs` 中标记为 `bubbles=false` 的所有事件均受影响：

| 事件 | 说明 |
|------|------|
| `scroll` / `scrollend` | 滚动 |
| `focus` / `blur` | 焦点 |
| `mouseenter` / `mouseleave` | 鼠标进出 |
| `pointerenter` / `pointerleave` | 指针进出 |
| `dragenter` / `dragexit` | 拖拽进出 |
| `cancel` | 取消 |
| `resize` | 尺寸变化 |
| `toggle` / `beforetoggle` | 切换 |
| 所有媒体事件 | `canplay`、`pause`、`play`、`ended` 等 |

### 根因分析

**`interpreter/src/ts/native.ts` 第 158-159 行**，事件处理函数硬编码 `bubbles=true`：

```typescript
const handler: EventListener = (event) =>
    this.handleEvent(event, event.type, true);  // ← BUG: 应该是 event.bubbles
```

完整因果链：

1. `bubbles.rs`: `"scroll" => false` — 正确标记 scroll 为非冒泡
2. `core.ts createListener()`: `bubbles=false` → 在元素上直接注册 `addEventListener` — ✅ 注册正确
3. **`native.ts`**: handler 硬编码 `bubbles=true` — ❌ 篡改了冒泡属性
4. `handleEvent()` → `sendSerializedEvent()`: 将 `bubbles: true` 传给 Rust — ❌ 应该是 false
5. `webview.rs`: `Event::new(data, propagates=true)` — propagates 被设为 true
6. `runtime.rs handle_event()`: `event.propagates()` 返回 true → 走 `handle_bubbling_event` — ❌ 应该走 `handle_non_bubbling_event`
7. `handle_bubbling_event()`: 从 target 向上遍历 DOM 树查找监听器，但非冒泡监听器注册在元素本身（local），冒泡遍历逻辑找不到 → **事件丢失**

### 正确的修复

```diff
- this.handleEvent(event, event.type, true);
+ this.handleEvent(event, event.type, event.bubbles);
```

### 为什么 Web 平台不受影响

Web 平台走 `web.ts` 代码路径，直接在浏览器事件回调中读取 `event.bubbles`，没有硬编码问题。此 bug 仅影响 Desktop/Mobile（native interpreter）。

### APP 层面绕行方案

**使用 JS 侧原生事件监听 + `js-bridge-bindgen` 替代 Dioxus `onscroll`**，完全绕过 Dioxus 事件分发机制：

```typescript
// scroll.ts — JS 侧原生监听
export function setHomeScrollTop(scrollTop: number): void {
  const el = document.getElementById("home_view");
  if (el) el.scrollTop = scrollTop;
}
```

```rust
// Rust 侧通过 js-bridge-bindgen 调用
js_api::scroll::set_home_scroll_top::<DioxusBridge>(target);
```

JS 侧的 `addEventListener('scroll', ...)` 由浏览器直接触发，不经过 Dioxus 的 native handler，因此不受此 bug 影响。

### 结论

**Dioxus 框架 bug（一行硬编码），APP 层面无法修复 Dioxus 事件分发，但可以通过 JS 侧原生监听绕行。** 应向上游报告。

---

## APP 层面的约束

由于以上缺陷，Dioxus Android 目前在以下场景不可靠：

1. **快速返回-重开**（生命周期快速切换）
2. **正常退出**（FORTIFY 崩溃，概率性问题）

建议向 Dioxus/wry/tao 上游报告，追踪以下 issue：

- tao: `Event::Destroy` 不被 event loop 处理（`ndk_glue.rs` 注释："not used by event loop yet"）
- wry: `onActivityDestroy` 只注销 MainPipe fd，不停止 VirtualDom
- dioxus-desktop: 缺乏 Android Activity 生命周期 → VirtualDom 生命周期的映射