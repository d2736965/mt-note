# Dioxus Native JS/Rust IPC 交互机制与性能

基于 Dioxus 0.7.9 源码（`packages/interpreter/src/ts/`、`packages/desktop/src/`）分析。

## 双向通信架构

```
┌──────────────────────────────────────────────────────────────────┐
│                        WebView (JS)                              │
│                                                                  │
│  JS→Rust (事件):                                                 │
│  用户事件 → serializeEvent() → JSON → base64                     │
│    → XHR POST /__events (header: dioxus-data)                   │
│    → 同步等待 Rust 返回 SynchronousEventResponse                │
│                                                                  │
│  Rust→JS (DOM 更新):                                            │
│  WebSocket 接收二进制 → rAF 队列 → run_from_bytes()             │
│    → sledgehammer 解码 → 直接 DOM 操作                          │
└──────────────────────────────┬───────────────────────────────────┘
                               │
                    ┌──────────┴──────────┐
                    │   IPC 边界           │
                    │  (WebView ↔ Rust)    │
                    └──────────┬──────────┘
                               │
┌──────────────────────────────┴───────────────────────────────────┐
│                        Rust 侧                                   │
│                                                                  │
│  事件接收:                                                       │
│  wry custom protocol /__events                                   │
│    → base64 解码 header → serde_json 反序列化 → HtmlEvent        │
│    → EventData 转换 → Event::new(data, bubbles)                  │
│    → runtime.handle_event() → 冒泡/非冒泡分发                    │
│    → 返回 SynchronousEventResponse (preventDefault/stopPropagation)│
│                                                                  │
│  DOM 更新发送:                                                   │
│  VirtualDom diff → MutationState (sledgehammer 二进制编码)       │
│    → export_memory() → Vec<u8>                                   │
│    → WebSocket send → JS 侧接收                                  │\�──────────────────────────────────────────────────────────────────┘
```

## JS→Rust：同步 XHR 事件通道

### 事件传递流程

```
1. 用户触发 DOM 事件（click/scroll/input/...）
2. JS handler 被调用 → handleEvent(event, name, bubbles)
3. serializeEvent(event, target) → 提取事件数据为 JSON 对象
4. sendSerializedEvent({ name, data, element, bubbles })
5. handleVirtualdomEventSync():
   - new XMLHttpRequest()
   - xhr.open("POST", "/__events", false)  ← 同步！
   - 事件数据 base64 编码后放入 dioxus-data header
   - xhr.send() → 阻塞等待 Rust 处理
   - 返回 JSON.parse(xhr.responseText)
6. 根据 response 执行 preventDefault / stopPropagation
```

### 关键源码

**native.ts `handleVirtualdomEventSync()`**：

```typescript
function handleVirtualdomEventSync(endpoint, contents) {
    const xhr = new XMLHttpRequest();
    xhr.open("POST", endpoint, false);  // false = 同步
    xhr.setRequestHeader("Content-Type", "application/json");
    // Android hack: body 放 header（wry shouldInterceptRequest 不支持 POST body）
    const contents_bytes = new TextEncoder().encode(contents);
    const contents_base64 = btoa(String.fromCharCode.apply(null, contents_bytes));
    xhr.setRequestHeader("dioxus-data", contents_base64);
    xhr.send();
    return JSON.parse(xhr.responseText);
}
```

**webview.rs `try_handle_event()`**：

```rust
pub fn try_handle_event(&self, request: wry::http::Request<Vec<u8>>) -> Result<Vec<u8>, serde_json::Error> {
    let data = request.headers().get("dioxus-data")
        .ok_or_else(|| Error::custom("dioxus-data header not set"))?;
    let as_utf = std::str::from_utf8(data.as_bytes())?;
    let data_from_header = BASE64_STANDARD.decode(as_utf)?;
    let response = match serde_json::from_slice(&data_from_header) {
        Ok(event) => {
            #[cfg(target_os = "android")]
            let _lock = crate::android_sync_lock::android_runtime_lock();
            self.handle_html_event(event)
        }
        Err(err) => SynchronousEventResponse::new(false),
    };
    serde_json::to_vec(&response)
}
```

### Android 特殊处理

Android 上 wry 的 `shouldInterceptRequest` 无法读取 POST body（[Google issue #119844519](https://issuetracker.google.com/issues/119844519)），所以事件数据 base64 编码后放入 `dioxus-data` HTTP header。

- Header 大小限制：通常 16KB/单 header，32KB/总 header
- 大表单数据可能超限（罕见但可能）
- Android 上还需要 `android_runtime_lock()` 确保线程安全

### 同步阻塞的性能影响

每次事件触发都**阻塞 JS 主线程**，等待 Rust 侧完成：

1. base64 解码
2. JSON 反序列化 → HtmlEvent
3. EventData 类型转换（Mounted → DesktopElement、Form → DesktopFormData、Drag → DesktopFileDragEvent 等）
4. `runtime.handle_event()` → 冒泡/非冒泡分发 → 执行用户回调
5. 回调可能触发 re-render → diff → 生成 DOM mutations
6. 序列化 `SynchronousEventResponse` 返回

**高频事件的性能风险**：

| 事件 | 频率 | 风险 |
|------|------|------|
| `onclick` | 低频（用户点击） | ✅ 安全 |
| `onchange` | 低频 | ✅ 安全 |
| `onkeydown` | 中频（打字速度） | ⚠️ 可能卡顿 |
| `oninput` | 高频（每次按键） | ❌ 可能卡顿 |
| `onscroll` | 极高频（每帧） | ❌ 严重卡顿 |
| `onmousemove` | 极高频（每帧） | ❌ 严重卡顿 |
| `onwheel` | 极高频（每帧） | ❌ 严重卡顿 |

## Rust→JS：WebSocket 二进制 DOM 更新

### DOM 更新流程

```
1. Rust 侧 VirtualDom diff 产生 mutations
2. MutationState (sledgehammer) 编码为二进制
3. export_memory() → Vec<u8>
4. WebSocket send(edits)
5. JS 侧 WebSocket.onmessage 接收
6. rafEdits(): 通过 requestAnimationFrame 调度
7. flushQueuedBytes(): 批量执行 run_from_bytes()
8. sledgehammer 解码 → 直接 DOM 操作（createElement/setAttribute/...）
9. markEditsFinished(): 发送空 ArrayBuffer 通知 Rust
```

### 关键源码

**native.ts `waitForRequest()`**：

```typescript
waitForRequest(editsPath: string, required_server_key: string) {
    this.edits = new WebSocket(editsPath);
    let authenticated = false;
    this.edits.onclose = () => {
        setTimeout(() => {
            if (this.edits.url != editsPath) return;
            this.waitForRequest(editsPath, required_server_key);  // 自动重连
        }, 100);
    };
    this.edits.onmessage = (event) => {
        const data = event.data;
        if (data instanceof Blob) {
            if (!authenticated) return;
            data.arrayBuffer().then((buffer) => {
                this.rafEdits(buffer);  // rAF 调度
            });
        } else if (typeof data === "string") {
            if (data === required_server_key) {
                authenticated = true;  // 认证 key
            }
        }
    };
}
```

**native.ts `rafEdits()`**：

```typescript
rafEdits(bytes: ArrayBuffer) {
    if (this.headless) {
        this.run_from_bytes(bytes);
        this.markEditsFinished();
    } else {
        this.enqueueBytes(bytes);  // 入队
        requestAnimationFrame(() => {
            this.flushQueuedBytes();  // rAF 时批量执行
            this.markEditsFinished();
        });
    }
}
```

### 设计亮点

- **二进制协议**：sledgehammer 编码，比 JSON 紧凑高效
- **rAF 批处理**：多次 mutations 合并到一帧执行，避免多次 reflow
- **自动重连**：WebSocket 断开后 100ms 重连（iOS 后台恢复场景）
- **认证机制**：随机 key 防止未授权连接
- **背压信号**：`markEditsFinished()` 发送空 ArrayBuffer 通知 Rust 可以发下一批

## 事件序列化细节

### serializeEvent() 按类型提取

| 事件类型 | 序列化内容 | 数据量 |
|----------|-----------|--------|
| `MouseEvent` | 坐标、按钮、修饰键、屏幕坐标、clientXY | ~200B |
| `KeyboardEvent` | key、code、修饰键、repeat、keyCode | ~150B |
| `InputEvent` | 表单值、valid、files | ~300B+ |
| `ScrollEvent` | scrollLeft/Top/Width/Height、clientWidth/Height | ~150B |
| `WheelEvent` | deltaX/Y/Z、deltaMode + Mouse 数据 | ~300B |
| `PointerEvent` | 指针坐标 + pressure + tiltX/Y + Mouse 数据 | ~400B |
| `TouchEvent` | touches 列表（坐标、force、radius） | ~500B+ |
| `FocusEvent` | **空对象 `{}`** | ~2B |
| `ClipboardEvent` | **空对象 `{}`** | ~2B |
| `DragEvent` | dataTransfer.files + Mouse 数据 | ~300B+ |
| `AnimationEvent` | animationName、elapsedTime | ~50B |
| `TransitionEvent` | propertyName、elapsedTime | ~50B |
| `CompositionEvent` | data | ~20B |
| `CustomEvent` | detail（ResizeObserver/IntersectionObserver） | ~100B |

### 特殊事件

- **`resize`**：通过 `ResizeObserver` 监听，触发 `CustomEvent("resize", { detail: entry })`
- **`visible`**：通过 `IntersectionObserver` 监听，触发 `CustomEvent("visible", { detail: entry })`
- **`scroll`/`scrollend`**：原生事件，序列化 scroll 位置信息
- **`focus`/`blur`**：原生事件，但序列化为空对象（无额外数据）

## 与 Web (wasm) 平台的对比

| 维度 | Web (wasm) | Native (WebView) |
|------|-----------|-----------------|
| JS→Rust | 直接 wasm 函数调用，零拷贝 | XHR 同步 HTTP + base64 + 反序列化 |
| Rust→JS | 直接操作 DOM（wasm 绑定） | WebSocket 二进制 + rAF |
| 事件延迟 | 微秒级 | 毫秒级（IPC 开销） |
| DOM 更新 | 直接 | 需要编解码 |
| 高频事件 | 无压力 | 同步阻塞导致卡顿 |
| 整体性能 | 接近原生 JS | 有明显 IPC 开销 |

## 性能优化建议

### 1. 高频事件必须绕行 Dioxus 事件系统

`onscroll`、`onmousemove`、`onwheel`、`oninput` 等高频事件，不要使用 Dioxus 事件处理器，改用 **JS 侧原生 `addEventListener` + `js-bridge-bindgen`** 通信。

原因：同步 XHR 每次事件都阻塞 JS 主线程，高频事件会导致严重卡顿。

**禁止使用裸 JS**（`document::eval` 内联字符串、手动 JSON 序列化），必须使用 `js-bridge-bindgen` 获得类型安全的 Rust 绑定。`js-bridge-bindgen` 的完整用法见 SKILL.md「js-bridge-bindgen：TS → Rust 类型安全绑定」章节。

### 2. 低频事件可安全使用 Dioxus 事件系统

`onclick`、`onchange`、onsubmit`、`onfocus`/`onblur`（bug 修复后）等低频事件，同步 XHR 的延迟可接受。

### 3. 减少 re-render 触发

事件回调中避免不必要的状态更新。每次状态更新都会触发 VirtualDom diff + WebSocket DOM 更新。

### 4. 大列表虚拟化

长列表不要一次性渲染所有节点，使用虚拟滚动减少 DOM 节点数量和 diff 开销。

### 5. JS 侧计算下沉

纯 UI 计算（滚动位置存储、动画帧、拖拽坐标变换）尽量在 JS 侧完成，只将最终结果通过 bridge 传给 Rust。