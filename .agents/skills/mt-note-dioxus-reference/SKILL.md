---
name: "mt-note-dioxus-reference"
description: "Dioxus 0.7 开发参考。Fullstack 架构、feature 配置、hooks 执行时机、异步 Future 处理（spawn/use_resource/use_future/use_effect+spawn 选择与依赖追踪）、server function、流式传输、打包分发、CDP 远程调试。响应式系统内部机制（SignalData/ReactiveContext 依赖追踪引擎、读订阅写通知闭环、use_hook 非响应式、use_hook+Cell 替代 useRef）。asset! 完整参考（AssetOptions 全类型、Folder 目录引用、hash 机制与第三方库运行时 fetch、CSS 字体 404 解决方案、禁止直接引用 node_modules）。Asset Handler 本地文件访问（local-file 协议）。Native 平台非冒泡事件（scroll/focus/blur）不触发及绕行方案。Native JS/Rust IPC 交互机制与性能（同步XHR事件通道、WebSocket DOM更新、高频事件绕行）。CustomEvent + Promise JS→Rust 事件驱动通信。"
---

# Dioxus 0.7 开发参考

## Dioxus Feature 分层（源码确证，0.7.10）

`fullstack` **不包含** `web`，也**不包含** `server`：

```toml
fullstack = ["dep:dioxus-fullstack", "dioxus-config-macro/fullstack", "dep:serde", ...]
web     = ["dep:dioxus-web"]
desktop = ["dep:dioxus-desktop"]
mobile  = ["dep:dioxus-mobile"]
server  = ["dep:dioxus-server", "dep:dioxus-fullstack", ...]
```

| Feature | 作用 | 所有 target 都安全? |
|---------|------|:---:|
| `fullstack` | `#[post]`/`#[get]` 宏、`ServerFnError`、客户端 HTTP 桩 | **是** |
| `web` | wasm 客户端启动路径 | 仅 wasm |
| `desktop`/`mobile` | 原生窗口启动路径 | 仅对应平台 |
| `server` | axum 服务端启动路径 | 仅 server |

**结论**：`fullstack` 写在依赖行（`dioxus = { features = ["fullstack"] }`）对所有 target 都安全。`dioxus::prelude` re-export `dioxus_fullstack` 的类型，**不需要显式依赖 `dioxus-fullstack`**。

## 推荐 Feature 配置

### api crate（纯库）

```toml
[dependencies]
dioxus = { workspace = true, features = ["fullstack"] }

[features]
server = ["dioxus/server"]
```

### 平台 crate（web / desktop / mobile）

```toml
[dependencies]
dioxus = { workspace = true, features = ["router", "fullstack"] }

[features]
default = []
web     = ["dioxus/web"]
desktop = ["dioxus/desktop"]
mobile  = ["dioxus/mobile"]
server  = ["dioxus/server", "api/server", "ui/server"]
```

### main.rs 统一入口

```rust
fn main() {
    dioxus::launch(app);
}
```

`dioxus::launch(app)` 根据 feature 自动选择启动路径。不需要 `dioxus_server::launch` 或 `set_server_url`。

### 服务端地址配置

`dioxus::launch(app)` 启动的服务端通过 `IP` 和 `PORT` 环境变量配置监听地址，由 `dioxus_cli_config::fullstack_address_or_localhost()` 读取：

```rust
// dioxus-cli-config 源码
pub const SERVER_IP_ENV: &str = "IP";
pub const SERVER_PORT_ENV: &str = "PORT";

pub fn fullstack_address_or_localhost() -> SocketAddr {
    let ip = server_ip().unwrap_or_else(|| IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
    let port = server_port().unwrap_or(8080);
    SocketAddr::new(ip, port)
}
```

| 优先级 | 来源 | 说明 |
|--------|------|------|
| 1 | `IP` / `PORT` 环境变量 | 手动运行时显式设置 |
| 2 | 默认值 | `127.0.0.1:8080` |

```bash
# Linux
IP=192.168.1.100 PORT=8080 ./server
```

- **`dx serve --addr` 同时用于 bind 地址和前端连接地址**。`0.0.0.0` 只能被 bind，不能作为连接目标，前端会报 `os error 10049`。要对外暴露时用实际 IP：`dx serve --addr '192.168.1.100'`
- 手动运行服务端二进制时，用 `IP`/`PORT` 环境变量控制地址

## web-sys 可用性

| 平台 | `web_sys` 可用? |
|------|:---:|
| Web (wasm) | **是** |
| Desktop/Mobile (native) | **否** |
| Server (SSR) | **否** |

native 上调用 `web_sys` 导致 SIGABRT。用 `#[cfg(feature = "web")]` 守卫，不要用 `#[cfg(target_arch = "wasm32")]`。

## Hooks 执行时机速查

| 上下文 | SSR 期间执行? | web_sys 安全 (wasm)? |
|--------|:---:|:---:|
| `use_effect` 回调 | **否** | 安全 |
| `use_future` 异步块 | **否** | 安全 |
| `use_resource` **外部闭包** | **是** | **不安全** |
| `use_resource` 异步块 | **否** | 安全 |
| `spawn` 异步块 | **否** | 安全 |
| 事件处理器 (onclick 等) | **否** | 安全 |
| 组件函数体 | **是** | **不安全** |

> **详细的 Scope 生命周期与 hooks 执行时序**（`run_scope` 内部步骤、ScopeStatus 状态机、effect 调度优先级、scope 销毁流程）见 **[hooks-lifecycle.md](hooks-lifecycle.md)**。
>
> **什么场景需要查阅 hooks-lifecycle.md**：
>
> | 问题 / 症状 | 需要的信息 | 查阅章节 |
> |------------|-----------|---------|
> | "use_effect 在组件销毁时会不会执行？" | 销毁时 effect 被跳过 | §5.3 销毁 + §3 表格 |
> | "为什么函数体在销毁时还会跑？" | run_scope 每次 rerun 都调 props.render() | §2 run_scope 流程 |
> | "use_effect 回调到底在什么时候执行？" | 不在 run_scope 中，在 poll_tasks 阶段 | §4.4 Effect 实际执行位置 |
> | "effect 和 task 谁先执行？" | dirty scopes > tasks > effects | §4.4 优先级 |
> | "use_drop 和 use_effect 销毁时有什么区别？" | use_drop 的 Drop 触发，use_effect 的 callback 不执行 | §5.3 销毁流程 + §7 结论 #2 #4 |
> | "首次渲染时 scope 是 Mounted 还是 Unmounted？" | 首次是 Unmounted，mount 后才变 Mounted | §4.3 状态机 |
> | "组件函数体、use_effect、use_drop 分别在什么时候执行/不执行？" | 完整时机对比 | §3 表格 + §7 关键结论 |
> | "为什么 UI 卡死（render panic 后静默失败）？" | 函数体 panic 毒化锁 | hooks-lifecycle.md 引申：`ndk-context.md` 避坑规则 |

### use_resource 陷阱

`use_resource(move || { /* 外部闭包 */ async move { /* 异步块 */ } })`：

- **外部闭包**在 SSR 期间执行（用于建立响应式依赖追踪），不能放 `web_sys`
- **异步块**仅在客户端执行，可以放 `web_sys`

```rust
// ❌ 外部闭包中调用 web_sys
use_resource(move || {
    let window = web_sys::window().unwrap();  // SSR panic!
    async move { /* ... */ }
});

// ✅ web_sys 放异步块里
use_resource(move || {
    let backend = backend();
    async move {
        let window = web_sys::window().unwrap();  // 安全
    }
});
```

## 异步 Future 处理（源码确证，0.7.10）

Dioxus 中处理异步操作有三种机制：`spawn`、`use_resource`、`use_future`。它们的底层共享同一套任务管理系统，但提供的抽象层级不同。

### 核心机制：spawn 与 spawned_tasks

所有异步 Future 最终都通过 `spawn` 提交到 Runtime。`spawn` 将 Task ID 注册到当前 scope 的 `spawned_tasks` 集合，**组件卸载时自动取消所有已注册的任务**。

**源码链路**（`global_context.rs` → `scope_context.rs` → `runtime.rs`）：

```
spawn(fut)                                    // global_context.rs:137
  → Runtime::with_current_scope(|cx| cx.spawn(fut))  // 委托给 ScopeState
    → Runtime::with(|rt| rt.spawn(self.id, fut))     // 提交到 Runtime
    → self.spawned_tasks.borrow_mut().insert(id)      // 注册到 scope 的任务集合

组件卸载时:
remove_scope(id)                              // runtime.rs:179
  → for id in scope.spawned_tasks.take() {
      self.remove_task(id)                    // 取消所有 spawn 的任务
    }
```

**结论**：`spawn` 本身就是受组件生命周期管理的。组件卸载时，所有通过 `spawn` 创建的任务都会被取消。

### spawn / spawn_isomorphic / spawn_forever 对比

| 函数 | 注册 scope | 组件卸载时取消 | SSR 期间运行 | Suspense 期间运行 | 典型用途 |
|------|-----------|:---:|:---:|:---:|---------|
| `spawn` | 当前 scope | ✅ | ❌ | ❌ | 一般异步任务 |
| `spawn_isomorphic` | 当前 scope | ✅ | ✅ | ✅ | 日志、响应状态变化（**禁止**做 IO 请求） |
| `spawn_forever` | ROOT scope | ❌ | ❌ | ❌ | 需要超越组件生命周期的后台任务 |

**源码**（`tasks.rs:136-155`）：

```rust
// spawn → TaskType::ClientOnly（SSR 和 Suspense 期间不运行）
pub fn spawn(&self, scope: ScopeId, task: impl Future<Output = ()> + 'static) -> Task {
    self.spawn_task_of_type(scope, task, TaskType::ClientOnly)
}

// spawn_isomorphic → TaskType::Isomorphic（SSR 和 Suspense 期间都运行）
pub fn spawn_isomorphic(&self, scope: ScopeId, task: impl Future<Output = ()> + 'static) -> Task {
    self.spawn_task_of_type(scope, task, TaskType::Isomorphic)
}

// spawn_forever → 注册到 ROOT scope，组件卸载不取消
pub fn spawn_forever(fut: impl Future<Output = ()> + 'static) -> Task {
    Runtime::with_scope(ScopeId::ROOT, |cx| cx.spawn(fut))
}
```

**TaskType 决定 Suspense 行为**（`tasks.rs:363-375`）：

```rust
enum TaskType {
    ClientOnly,                          // spawn — Suspense 期间不运行
    Suspended { boundary: SuspenseLocation }, // 被 suspensify 的任务
    Isomorphic,                          // spawn_isomorphic — Suspense 期间运行
}
impl TaskType {
    fn runs_during_suspense(&self) -> bool {
        matches!(self, TaskType::Isomorphic | TaskType::Suspended { .. })
    }
}
```

### Task 的生命周期管理

Task 有 active/paused 状态（`tasks.rs:72-120`）：

```rust
impl Task {
    pub fn cancel(self) { remove_future(self); }  // 立即丢弃
    pub fn pause(&self) { self.set_active(false); }  // 暂停（不 poll，但不丢弃）
    pub fn resume(&self) { self.set_active(true); }  // 恢复
    pub fn paused(&self) -> bool { ... }
    pub fn wake(&self) { ... }   // 通知调度器重新 poll
    pub fn poll_now(&self) -> Poll<()> { ... }  // 立即 poll
}
```

**Task 完成时自动清理**（`tasks.rs:284-292`）：

```rust
// handle_task_wakeup 中，poll 返回 Ready 时：
if poll_result.is_ready() {
    // 从 spawned_tasks 移除，避免卸载时 double drop
    self.get_state(task.scope).spawned_tasks.borrow_mut().remove(&id);
    self.remove_task(id);
}
```

### use_resource — 带依赖追踪和值缓存的异步资源

**源码**（`hooks/src/use_resource.rs:23-80`）：

`use_resource` 内部机制：

1. **外部闭包**：每次组件 rerun 时执行，用于建立响应式依赖（读取 signal 会自动订阅）
2. **异步块**：通过 `spawn` 提交，在 `ReactiveContext` 内 poll 以追踪异步块内的依赖
3. **依赖变化时自动重跑**：内部 `spawn` 了一个监听循环，`changed.next().await` 等待依赖变化，然后 cancel 旧任务、启动新任务
4. **值通过 Signal 暴露**：`Resource<T>` 实现了 `Deref<Target = dyn Fn() -> Option<T>>`，可直接 `resource()` 读取

```rust
// use_resource 内部结构（简化）
pub fn use_resource<T, F>(mut future: impl FnMut() -> F + 'static) -> Resource<T> {
    let mut value = use_signal(|| None);           // Option<T> — 结果值
    let mut state = use_signal(|| Pending);         // UseResourceState
    let (rc, changed) = use_hook(|| ReactiveContext::new_with_origin(location));

    let cb = use_callback(move |_| {
        state.set(Pending);
        let fut = rc.reset_and_run_in(&mut future);  // 外部闭包在此执行，追踪依赖
        spawn(async move {
            pin_mut!(fut);
            let res = future::poll_fn(|cx| {
                rc.run_in(|| fut.poll_unpin(cx))     // 每次 poll 都在 ReactiveContext 内
            }).await;
            state.set(Ready);
            value.set(Some(res));                     // 写入结果
            waker.wake(());
        })
    });

    let mut task = use_hook(|| Signal::new(cb(())));  // 首次启动

    use_hook(|| {
        let mut changed = changed.take().unwrap();
        spawn(async move {
            loop {
                let _ = changed.next().await;          // 等待依赖变化
                task.write().cancel();                 // 取消旧任务
                task.set(cb(()));                      // 启动新任务
            }
        });
    });

    Resource { task, value, state, waker, callback: cb }
}
```

**Resource\<T\> 的 API**：

| 方法 | 说明 |
|------|------|
| `resource()` | 读取 `Option<T>`（等价于 `resource.with(\|v\| v.clone())`），订阅变化 |
| `resource.restart()` | 取消当前任务，重新启动 |
| `resource.cancel()` | 取消且标记 Stopped |
| `resource.pause()` / `.resume()` | 暂停/恢复（不丢弃） |
| `resource.clear()` | 清空值，不影响运行中的任务 |
| `resource.pending()` / `.finished()` | 查询状态（`finished` 用 `peek` 不订阅） |
| `resource.state()` | 返回 `ReadSignal<UseResourceState>` |
| `resource.value()` | 返回 `ReadSignal<Option<T>>` |
| `resource.suspend()` | 挂起渲染直到 Ready（配合 Suspense 使用） |
| `resource.await` | `impl Future`，等待下次 wake |

**Resource\<Result\<T, E\>\> 的额外方法**：

| 方法 | 说明 |
|------|------|
| `resource.result()` | 返回 `Option<Result<MappedSignal<T>, MappedSignal<E>>>`，按 Ok/Err 分解 |

### use_future — 带依赖追踪但无值缓存的异步任务

**源码**（`hooks/src/use_future.rs:45-80`）：

`use_future` 与 `use_resource` 的关键区别：

1. **不缓存结果值**：没有 `Signal<Option<T>>`，Future 的 `Output` 是 `()` — 你需要自己在闭包里写 signal
2. **有 early return 保护**：通过 `use_hook_did_run` 实现 — 如果组件 early return 导致 hook 未执行，future 自动暂停
3. **依赖追踪方式相同**：通过 `use_callback` + `ReactiveContext` 追踪外部闭包中的信号读取

```rust
pub fn use_future<F>(mut future: impl FnMut() -> F + 'static) -> UseFuture {
    let mut state = use_signal(|| UseFutureState::Pending);
    let callback = use_callback(move |_| {
        let fut = future();                          // 外部闭包执行
        dioxus_core::spawn(async move {
            state.set(UseFutureState::Pending);
            fut.await;
            state.set(UseFutureState::Ready);
        })
    });
    let task = use_hook(|| CopyValue::new(callback(())));

    // early return 保护：hook 没执行时暂停 future
    use_hook_did_run(move |did_run| match did_run {
        true => task.peek().resume(),
        false => task.peek().pause(),
    });

    UseFuture { task, state, callback }
}
```

**use_hook_did_run 的实现**（`hooks/src/use_hook_did_run.rs`）：

```rust
pub fn use_hook_did_run(mut handler: impl FnMut(bool) + 'static) {
    let mut did_run_ = use_hook(|| CopyValue::new(false));
    use_before_render(move || did_run_.set(false));  // 每次 render 前重置
    did_run_.set(true);                               // hook 执行了，标记为 true
    use_after_render(move || handler(did_run_()));    // render 后根据状态回调
}
```

**UseFuture 的 API**：

| 方法 | 说明 |
|------|------|
| `future.restart()` | 取消旧任务，启动新任务 |
| `future.cancel()` | 取消且标记 Stopped |
| `future.pause()` / `.resume()` | 暂停/恢复 |
| `future.finished()` | 查询是否完成（用 `peek` 不订阅） |
| `future.state()` | 返回 `ReadSignal<UseFutureState>` |

### use_effect + spawn — 响应式副作用模式

`use_effect` 内部也用 `spawn`（`hooks/src/use_effect.rs:11-50`）：

```rust
pub fn use_effect(mut callback: impl FnMut() + 'static) -> Effect {
    let callback = use_callback(move |_| callback());
    use_hook(|| {
        let (rc, mut changed) = ReactiveContext::new_with_origin(location);
        let effect_queued = Rc::new(Cell::new(false));

        let queue_effect_for_next_render = move || {
            if effect_queued.get() { return; }       // 去重
            effect_queued.set(true);
            queue_effect(move || {
                rc.reset_and_run_in(|| callback(())); // 在 ReactiveContext 内执行
                effect_queued.set(false);
            });
        };

        queue_effect_for_next_render();              // 首次排队
        spawn(async move {
            loop {
                let _ = changed.next().await;         // 等待依赖变化
                queue_effect_for_next_render();       // 重新排队
            }
        });
        Effect { rc }
    })
}
```

**`use_effect` + `spawn` 模式**适用于"响应状态变化、执行异步副作用"的场景：

```rust
use_effect(move || {
    if let Some(Ok(data)) = resource() {    // 读取 resource 触发订阅
        state_signal.set(derived_value);     // 同步副作用
        if let Some(el) = mounted_el() {
            spawn(async move {               // 需要异步时用 spawn
                let rect = el.get_client_rect().await;
                other_signal.set(computed);
            });
        }
    }
});
```

### 选择决策表

| 场景 | 推荐方案 | 原因 |
|------|---------|------|
| 异步获取数据，UI 需要消费结果 | `use_resource` | 自动依赖追踪 + 值缓存 + 状态管理 |
| 异步任务，结果自己写 signal | `use_future` | 依赖追踪 + early return 保护 |
| 响应状态变化，执行同步副作用 | `use_effect` | 依赖追踪 + effect 调度 |
| 响应状态变化，需要异步操作后写信号 | `use_effect` + `spawn` | effect 负责依赖追踪，spawn 负责 async |
| 事件处理器中的异步操作 | `spawn` | 不需要依赖追踪，一次性任务 |
| 需要超越组件生命周期的后台任务 | `spawn_forever` | 注册到 ROOT scope |
| SSR 期间也需要运行的任务 | `spawn_isomorphic` | TaskType::Isomorphic |

### 依赖追踪机制详解

`use_resource`、`use_future`、`use_effect` 都使用 `ReactiveContext` 实现依赖追踪：

1. **外部闭包在 `ReactiveContext` 内执行**（`rc.reset_and_run_in` / `rc.run_in`）
2. 闭包内读取的 signal 自动注册为依赖
3. 依赖变化时，`changed` stream 收到通知
4. 监听循环被唤醒，触发重新执行

**`use_resource` 的双层追踪**：
- 外部闭包追踪：每次组件 rerun 时执行，决定是否需要重新创建 Future
- 异步块内追踪：每次 poll 都在 `rc.run_in` 内，异步块中读取的 signal 也被追踪

**`use_future` 的追踪**：
- 仅外部闭包追踪（`use_callback` 内部用 `ReactiveContext`）
- 异步块内**不追踪** — 如果需要在异步块内响应信号变化，应手动读取 signal

**`use_effect` 的追踪**：
- 回调在 `rc.reset_and_run_in` 内执行
- 每次执行都重新建立依赖（因为回调可能条件性地读取不同信号）

### spawn 的常见编译错误

**1. 忘记 `move`**：

```rust
// ❌ async 块借用 signal，但 signal 在函数结束时 drop
spawn(async {
    println!("{}", signal());
});

// ✅ move 将 signal 所有权移入 async 块
spawn(async move {
    println!("{}", signal());
});
```

**2. 非 Copy 类型被 move 两次**：

```rust
// ❌ String 不实现 Copy，第一次 move 后第二次编译错误
spawn(async move { println!("{string}"); });
spawn(async move { println!("{string}"); });

// ✅ 方案 1：用 Copy 类型（如 Signal/ReadSignal）
// ✅ 方案 2：先 clone 再 move
let s = string.clone();
spawn(async move { println!("{string}"); });
spawn(async move { println!("{s}"); });
```

### 陷阱与注意事项

1. **`use_resource` 外部闭包在 SSR 期间执行**：不能放 `web_sys` 等 wasm-only API，应放异步块内
2. **`use_future` 有 early return 保护，`use_resource` 没有**：如果组件可能 early return，`use_future` 更安全
3. **`spawn` 在 `use_effect` 回调内使用时，每次 effect 重新执行都会创建新 task**：如果 effect 频繁触发，注意 task 累积问题。旧 task 不会自动取消（除非你手动 cancel）
4. **`Resource<T>` 读取 `resource()` 会订阅变化**：在 `use_effect` 内读取会建立依赖，在事件处理器内读取不会
5. **`spawn_forever` 运行在 ROOT scope**：内部调用 `consume_context` 等全局方法时，作用域是 ROOT 而非创建它的组件

## 响应式系统内部机制（源码确证）

Dioxus 的响应式系统由三层构成：`SignalData`（数据存储 + 订阅者列表）、`ReactiveContext`（谁在读）、`Signal`（读时订阅、写时通知）。

### 核心三层架构

#### 1. SignalData\<T\> — 值的家

```rust
// dioxus/packages/signals/src/signal.rs
pub struct SignalData<T> {
    pub(crate) subscribers: Arc<Mutex<HashSet<ReactiveContext>>>,  // 订阅者集合
    pub(crate) value: T,                                            // 实际值
}
```

两个字段：值和"谁订阅了我"的集合。

#### 2. ReactiveContext — 谁在"看"信号

```rust
// dioxus/packages/core/src/reactive_context.rs
pub struct ReactiveContext {
    scope: ScopeId,                          // 所属组件
    inner: GenerationalBox<Inner>,           // 包含 update 回调
}
```

每个作用域（组件、effect 等）都有一个 `ReactiveContext`，通过 thread-local 栈存储：

```rust
thread_local! {
    static CURRENT: RefCell<Vec<ReactiveContext>> = const { RefCell::new(vec![]) };
}
```

**`ReactiveContext::current()` 返回当前正在执行的 context**——这就是"谁在读信号"的答案。

#### 3. 读 → 订阅，写 → 通知

**读信号时订阅**（`Readable::try_read_unchecked`）：

```rust
// dioxus/packages/signals/src/signal.rs
fn try_read_unchecked(&self) -> ... {
    let inner = self.inner.try_read_unchecked()?;
    if let Some(reactive_context) = ReactiveContext::current() {
        reactive_context.subscribe(inner.subscribers.clone());  // 把当前 context 加入订阅者
    }
    Ok(S::map(inner, |v| &v.value))
}
```

**写信号时通知**（`SignalSubscriberDrop::drop` — 写锁释放时触发）：

```rust
// dioxus/packages/signals/src/signal.rs
impl<T, S: Storage<SignalData<T>>> Drop for SignalSubscriberDrop<T, S> {
    fn drop(&mut self) {
        self.signal.update_subscribers();  // 通知所有订阅者
    }
}

fn update_subscribers(&self) {
    let inner = self.inner.read();
    let mut subscribers = std::mem::take(&mut *inner.subscribers.lock().unwrap());
    subscribers.retain(|reactive_context| reactive_context.mark_dirty());
    inner.subscribers.lock().unwrap().extend(subscribers);
}
```

**`mark_dirty()` 执行 update 回调**：

```rust
// dioxus/packages/core/src/reactive_context.rs
pub fn mark_dirty(&self) -> bool {
    if let Ok(mut self_write) = self.inner.try_write() {
        (self_write.update)();  // 执行回调
        true
    } else {
        false
    }
}
```

对于组件 scope，`update` 回调是 `SchedulerMsg::Immediate(scope_id)`，触发该组件重新渲染。

### 完整闭环

```
组件渲染
  → 设置 ReactiveContext::current() 为当前 scope
  → 组件代码执行，读取 Signal
    → Signal::read() 调用 ReactiveContext::current() 拿到当前 scope
    → 把当前 scope 加入 SignalData.subscribers
  → 渲染完成

后续：
  Signal::set() 修改值
    → 写锁释放时触发 SignalSubscriberDrop::drop()
    → update_subscribers() 遍历 subscribers
    → 每个 subscriber.mark_dirty()
      → 执行 update 回调 → SchedulerMsg::Immediate(scope_id)
        → 调度器重新渲染该 scope
```

### use_hook 不是响应式的

`use_hook` 只是跨渲染持久化缓存，**不参与响应式系统**：

```rust
// dioxus/packages/core/src/scope_context.rs
pub fn use_hook<State: Clone + 'static>(
    &self,
    initializer: impl FnOnce() -> State,
) -> State {
    // 用 hook_index 记住当前是第几个 hook
    // 从 hooks 列表中按索引取出之前存储的值，Clone 一份返回
    // 首次调用时用 initializer 创建值并存入列表
    // 没有任何订阅/通知机制
}
```

`use_signal` 就是 `use_hook` 包了一个 `Signal`——响应式能力来自 `Signal` 类型，不是 `use_hook`。

### 响应式类型一览

所有响应式类型都基于 `dioxus-signals` 的同一套依赖追踪引擎：

| 类型 | 作用 | 底层 |
|------|------|------|
| `Signal<T>` | 基础响应式值，读订阅、写通知 | `SignalData<T>` |
| `Memo<T>` | 派生值，依赖变化自动重算 | `SignalData<T>` + 自动重算 |
| `ReadSignal<T>` | 只读信号（boxed 类型） | `SignalData<T>` |
| `CopyValue<T>` | 像 Signal 但不追踪读依赖 | `SignalData<T>` + 跳过订阅 |
| `Global<T>` | 全局信号，跨组件共享 | `SignalData<T>` + 全局存储 |
| `MappedSignal<O>` | 映射信号，从其他信号派生引用 | 引用父信号 |
| `Resource<T>` | 异步数据获取 | 内部用 `Signal<Option<T>>` |
| `Effect` | 依赖追踪副作用 | `ReactiveContext` + spawn |

### use_hook + Cell = Dioxus 版 useRef

`use_hook` + `std::cell::Cell` 是 Dioxus 0.7 中替代已移除的 `use_ref` 的方案，适合"需要记住值但不触发重渲染"的场景：

```rust
use std::cell::Cell;

// 记住上一次的缩放值，但不触发重渲染
let prev_scale = use_hook(|| Cell::new(0.0_f64));
use_effect(move || {
    let s = scale();
    let prev = prev_scale.get();  // 读取 Cell 不触发依赖
    if prev != 0.0 && (s - prev).abs() > f64::EPSILON {
        // 缩放值变化了
    }
    prev_scale.set(s);  // 写入 Cell 不触发 effect 重跑
});
```

**与 `use_signal` 的关键区别**：`use_signal` 在 effect 中读取会建立依赖，写入会触发 effect 重跑，可能导致无限循环。`use_hook` + `Cell` 完全绕开响应式系统，不会触发任何重渲染。

## Vue 钩子对照参考

从 Vue 迁移到 Dioxus 时，以下对照表帮助快速理解 Dioxus hooks 的语义。

### use_effect ≈ watchEffect

`use_effect` 最接近 Vue 3 的 `watchEffect`：自动追踪依赖、首次渲染后执行、依赖变化时重新执行、组件卸载时不执行。

**Dioxus**:
```rust
let count = use_signal(|| 0);
use_effect(move || {
    println!("count 变了: {}", count());
});
```

**Vue**:
```typescript
const count = ref(0);
watchEffect(() => {
    console.log('count 变了:', count.value);
});
```

| 特性 | Dioxus `use_effect` | Vue `watchEffect` |
|------|:---:|:---:|
| 自动追踪依赖 | ✅ | ✅ |
| 首次渲染后执行 | ✅ | ✅ |
| 依赖变化时重新执行 | ✅ | ✅ |
| 组件卸载时不执行 | ✅ | ✅ |
| 执行时机 | DOM 更新后（effect 优先级最低） | DOM 更新后（flush: 'post'） |
| 内置清理回调 | ❌ 需用 `use_drop` | ✅ `onCleanup(() => ...)` |
| 条件性依赖 | ✅ 每次执行重新建立 | ✅ 每次执行重新追踪 |

### 完整钩子对照表

| Vue | Dioxus | 说明 |
|-----|--------|------|
| `watchEffect` | `use_effect` | 自动追踪依赖的副作用 |
| `watch(source, cb)` | `use_effect` + 手动判断 | 显式指定依赖源 |
| `onMounted` | `use_effect`（首次执行即 mount 后） | Dioxus 没有独立的 mounted 钩子 |
| `onUpdated` | 无直接等价 | `onUpdated` 每次更新都跑，`use_effect` 只在依赖变化时跑 |
| `onUnmounted` | `use_drop` | 组件卸载时执行清理 |
| `computed` | `use_memo` / `use_resource` | 派生响应式状态 |
| `ref` / `reactive` | `use_signal` | 响应式状态 |
| `provide` / `inject` | `use_context` / `use_provide_context` | 跨组件依赖注入 |
| `onMounted + async` | `use_resource` / `use_future` | 异步数据获取 |
| `watch + async` | `use_effect` + `spawn` | 响应变化执行异步副作用 |

### 关键差异

1. **`use_effect` ≈ `watchEffect` + `onMounted` 合体**：首次执行在 mount 后，后续依赖变化时重新执行
2. **清理逻辑分离**：Vue 的 `onCleanup` 内置于 `watchEffect`，Dioxus 需要单独用 `use_drop`
3. **`use_resource` 无 Vue 等价物**：Vue 中异步数据获取通常用 `onMounted + ref` 手动管理，Dioxus 的 `use_resource` 提供了自动依赖追踪 + 值缓存 + 状态管理的一体化方案
4. **`use_future` 的 early return 保护**：Vue 没有 early return 概念（template 始终完整渲染），Dioxus 组件可以 early return，`use_future` 通过 `use_hook_did_run` 自动暂停未执行的 future

## Server Function

### 宏展开机制

`#[post]`/`#[get]` 宏将函数体分裂为两个分支：

```rust
pub async fn echo(input: String) -> Result<String, ServerFnError> {
    // 客户端：生成 HTTP 请求桩
    #[cfg(not(feature = "server"))]
    { return ClientRequest::new(POST, "/api/echo", ...).fetch_client(...).await; }

    // 服务端：保留原函数体 + inventory::submit! 注册 axum handler
    #[cfg(feature = "server")]
    {
        inventory::submit! { ServerFunction::new(POST, "/api/echo", handler) }
        return echo(input).await;
    }
}
```

- 原函数体在 `#[cfg(feature = "server")]` 内，前端不编译，可自由使用 server-only 依赖
- 函数体内**不需要**手动加 `#[cfg]`，宏已处理。但模块级 `use`/`struct` 等不受宏保护
- `feature = "server"` 检查的是**当前 crate**，不是依赖的 feature

### 禁止 `#[server]`，用 `#[get]`/`#[post]`

`#[server]` 生成不可读 hash 路径。始终用显式路径：

```rust
#[get("/api/posts?category")]  async fn get_posts(category: String) -> Result<Vec<Post>> { ... }
#[post("/api/posts/create")]   async fn create_post(req: CreatePostRequest) -> Result<Post> { ... }
#[get("/api/user/{id}")]       async fn get_user(id: u32) -> Result<User> { ... }
#[get("/api/search?q&limit")]  async fn search(q: String, limit: Option<u32>) -> Result<Vec<Item>> { ... }
```

## asset! 宏完整参考（源码确证，manganis + dioxus 0.7.10）

### 基本语法

```rust
// 无选项（默认加 hash 后缀）
asset!("/assets/style.css")

// 带选项
asset!("/assets/style.css", AssetOptions::css().with_minify(false))
```

### 路径规则

| 路径格式 | 解析方式 | 示例 |
|---------|---------|------|
| `/path` | 相对于 `CARGO_MANIFEST_DIR` | `asset!("/ts/dist/mt-note-ts.css")` |
| `./path` | 相对于当前 `.rs` 文件（需 Rust 1.88+） | `asset!("./style.css")` |

**安全限制**（源码 `manganis-macro/src/asset.rs`）：路径必须在 `CARGO_MANIFEST_DIR` 下，不能逃逸到 crate 目录外。

**禁止直接引用 node_modules**：`asset!` 禁止直接引用 `node_modules/` 中的资源。所有资源必须经过 TS 项目（Vite）筛选输出到 `dist/` 后，Rust 侧才能通过 `asset!("/ts/dist/...")` 引用。这确保了 TS 项目是资源的唯一入口，Rust 侧不绕过 TS 项目直接访问 node_modules。

### 返回值

`asset!` 返回 `manganis::Asset`，可 `.to_string()` 得到运行时路径（如 `/assets/style-dxh3a4b5c.css`），实现了 `Display`、`Into<String>`、`From<Asset> for String`。

### AssetOptions 类型一览

| 类型 | 创建方式 | 默认 hash | 专属选项 |
|------|---------|:---------:|---------|
| **通用** | `AssetOptions::builder()` | ✅ | `.with_hash_suffix(bool)` |
| **Image** | `AssetOptions::image()` | ✅ | `.with_format(ImageFormat)` / `.with_avif()` / `.with_webp()` / `.with_jpg()` / `.with_png()` / `.with_size(ImageSize)` / `.with_preload(bool)` |
| **CSS** | `AssetOptions::css()` | ✅ | `.with_minify(bool)` (默认 true) / `.with_preload(bool)` / `.with_static_head(bool)` |
| **CSS Module** | `AssetOptions::css_module()` | ✅ | `.with_minify(bool)` / `.with_preload(bool)` |
| **JS** | `AssetOptions::js()` | ✅ | `.with_minify(bool)` (默认 true) / `.with_preload(bool)` / `.with_static_head(bool)` / `.with_module(bool)` |
| **Folder** | `AssetOptions::folder()` | ❌ | 无 |

### ImageFormat 枚举

`Png` / `Jpg` / `Webp` / `Avif` / `Unknown`

### ImageSize 枚举

`Manual { width: u32, height: u32 }` / `Automatic`

### Folder 选项详解

`AssetOptions::folder()` 默认 `add_hash: false`，文件名保持不变。CLI 递归复制整个目录内容到输出目录。

```rust
// 复制整个 cmaps 目录（168 个 .bcmap 文件），文件名不被 hash
// 注意：必须引用 TS 项目 dist/ 下的资源，禁止直接引用 node_modules
asset!("/ts/dist/cmaps", AssetOptions::folder())
```

**适用场景**：运行时按原名 fetch 的资源（如 pdfjs cmaps）。第三方库内部用 `fetch(cMapUrl + name + ".bcmap")` 按原名取文件，文件名不能被 hash。

### with_hash_suffix(false) 注意事项

当禁用 hash 时，必须用 `#[used]` 确保 asset 不被优化掉：

```rust
#[used]
static ASSET: manganis::Asset = manganis::asset!(
    "/assets/style.css",
    manganis::AssetOptions::builder().with_hash_suffix(false)
);
```

### 跨 crate 资产共享（SCSS / CSS）

Manganis `asset!()` 将文件注册到 `__ASSETS__` 段，链接时自动合并所有 crate 的资产。上游 crate 通过 `pub static` 暴露常量，下游直接引用：

```rust
// crates/ui/src/lib.rs — 上游
#[used]
pub static STYLES: dioxus::prelude::Asset =
    dioxus::prelude::asset!("/assets/styles/bundle-ui.scss");
```

```rust
// crates/web/src/app.rs — 下游平台包
fn app() -> Element {
    rsx! {
        document::Stylesheet { href: ui::STYLES }         // ← 上游样式先加载
        document::Stylesheet { href: asset!("/assets/styles/bundle-web.scss") }
        Router::<Route> {}
    }
}
```

**样式表顺序**：上游 `ui::STYLES` 在前，平台 bundle 在后——后声明的 CSS 覆盖先声明的。`#[used]` 防止链接器剥离。`dioxus/fullstack` 已包含 asset feature。

### asset! 复制机制

`asset!("/assets/xxx")` 将文件注册到 manganis 资产系统，`dx serve`/`dx bundle` 时自动复制到 `target/.../public/assets/` 目录。**`public/` 目录下的文件不会被 `dx` 复制到输出目录**，导致 404。

```rust
// ✅ assets 目录 + asset! 宏 → dx 自动复制到输出目录
document::Stylesheet { href: asset!("/assets/katex-0.16.11/katex.min.css") }
document::Script { src: asset!("/assets/katex-0.16.11/katex.min.js") }

// ❌ public 目录 → dx 不复制，浏览器 404
document::Stylesheet { href: "/katex-0.16.11/katex.min.css" }
```

**规则**：需要 `dx` 自动复制的静态文件（CSS/JS/字体/图片等）放 `assets/` 目录并用 `asset!()` 引用；`public/` 仅用于 `index.html` 等需原样保留的文件。

### 禁止直接引用 node_modules（重要规则）

**`asset!` 禁止直接引用 `node_modules/` 中的任何资源。** 所有资源必须经过 TS 项目（Vite）处理或筛选后输出到 `dist/`，Rust 侧只能引用 `dist/` 下的产物。

**原因**：TS 项目是资源的唯一入口和筛选层。Rust 侧绕过 TS 项目直接引用 node_modules 会导致：
- 资源未经 TS 项目筛选，可能包含不需要的文件
- TS 项目升级依赖或调整构建配置后，Rust 侧引用可能失效
- 违反关注点分离原则

**正确做法**：

```rust
// ✅ 引用 TS 项目构建产物
asset!("/ts/dist/mt-note-ts.css")
asset!("/ts/dist/cmaps", AssetOptions::folder())
asset!("/ts/dist/standard_fonts", AssetOptions::folder())

// ❌ 禁止直接引用 node_modules
asset!("/ts/node_modules/pdfjs-dist/cmaps", AssetOptions::folder())
asset!("/ts/node_modules/katex/dist/katex.min.css")
```

**TS 项目侧处理方式**：
- JS/CSS 模块：通过 `import` 语句引入，Vite 自动打包
- 运行时按需 fetch 的资源目录（如 cmaps、standard_fonts）：通过 Vite 插件在 `closeBundle` 钩子中复制到 `dist/`
- 单个静态资源文件：通过 Vite 的 `new URL(..., import.meta.url)` 或 `?url` 后缀处理

### asset! + CSS 字体 404 问题与解决方案

**问题**：`asset!()` 处理 CSS 文件时，会给文件名加 hash 后缀（如 `katex.min-dxh20b2667b35f97d9.css`），但**不会替换 CSS 内部 `url()` 引用的路径**。CSS 中 `@font-face{src:url(fonts/xxx.woff2)}` 的相对路径，浏览器解析为 `/assets/fonts/xxx.woff2`，而 manganis 复制字体文件到 `assets/` 根目录且带 hash（如 `KaTeX_Main-Regular-dxhb9499d34a68da06c.woff2`），导致所有字体 404。

**根因**：manganis 会解析 CSS 中的 `url()` 并复制引用的资源文件，但复制后的文件名带 hash 且不保持目录结构，也不替换 CSS 中的路径——路径不匹配导致 404。

**解决方案**（以 KaTeX 为例）：

1. **从 CSS 中删除所有 `@font-face` 声明**（路径不可用）
2. **在 Rust 代码中用 `asset!` + JS 动态注入正确的 `@font-face`**

```rust
#[cfg(feature = "web")]
fn inject_katex_fonts() {
    if let Some(win) = window()
        && let Some(doc) = win.document()
        && doc.query_selector("#katex-fonts-injected").unwrap_or(None).is_none()
        && let Ok(style) = doc.create_element("style")
    {
        style.set_id("katex-fonts-injected");
        let font_faces = format!(
            "@font-face{{font-family:KaTeX_AMS;src:url({ams}) format('woff2')}}\
             @font-face{{font-family:KaTeX_Main;src:url({main}) format('woff2')}}\
             @font-face{{font-family:KaTeX_Math;font-style:italic;src:url({math}) format('woff2')}}",
            ams = asset!("/assets/katex-0.16.11/fonts/KaTeX_AMS-Regular.woff2"),
            main = asset!("/assets/katex-0.16.11/fonts/KaTeX_Main-Regular.woff2"),
            math = asset!("/assets/katex-0.16.11/fonts/KaTeX_Math-Italic.woff2"),
            // ... 其他字体
        );
        style.set_text_content(Some(&font_faces));
        if let Some(head) = doc.head()
            && let Err(e) = head.append_child(&style)
        {
            log::debug!("append_child style failed: {e:?}");
        }
    }
}
```

**原理**：
- `asset!()` 在编译时返回带 hash 的正确路径（如 `/assets/KaTeX_Main-Regular-dxhb9499d34a68da06c.woff2`），同时 manganis 将文件复制到输出目录
- JS 动态创建 `<style>` 注入 `@font-face`，使用 `asset!` 生成的正确路径，覆盖 CSS 中无效的声明
- `#katex-fonts-injected` 标记确保只注入一次
- 只声明 `.woff2` 格式（现代浏览器均支持），无需 `.woff`/`.ttf` 回退

**通用规则**：当 CSS 中 `url()` 引用的资源需要 manganis 处理时，不能依赖 CSS 内的相对路径。必须用 `asset!()` 引用资源文件，再通过 JS 注入或 `document::Stylesheet`/`document::Link` 等 Dioxus 组件引用。

### asset! hash 机制与第三方库运行时 fetch

**核心区别**：`asset!` 的 hash 后缀对"由 Rust 代码引用"的资源无影响，但对"由第三方库运行时 fetch"的资源会导致 404。

| 资源 | 谁引用 | hash 有问题？ | 解决方案 |
|------|--------|:---:|------|
| CSS 文件 | Rust `asset!()` → 传给 `document::Stylesheet` | ❌ | 默认 hash 即可 |
| 字体文件 | Rust `asset!()` → 注入 `@font-face` | ❌ | 默认 hash 即可 |
| JS/MJS 文件 | Rust `asset!()` → 传给 `<script src>` | ❌ | 默认 hash 即可 |
| .bcmap 文件 | **pdfjs 内部** `fetch(cMapUrl + name)` | ✅ | `AssetOptions::folder()` 或 `.with_hash_suffix(false)` |

**原因**：`asset!` 返回带 hash 的路径给 Rust 代码使用，Rust 代码用这个路径引用资源，所以 hash 无影响。但第三方库（如 pdfjs）在运行时自己拼接 URL fetch 文件，不知道 hash 后缀，按原名 fetch 就找不到文件。

## 流式传输

### ServerEvents\<T\>（推荐 Agent 聊天）

```rust
#[derive(serde::Serialize, serde::Deserialize)]
struct ChatChunk { text: String, done: bool }

// 服务端
#[get("/api/chat/stream?msg")]
async fn chat_stream(msg: String) -> Result<ServerEvents<ChatChunk>> {
    Ok(ServerEvents::new(|mut tx| async move {
        tx.send(ChatChunk { text: "Hello".into(), done: false }).await.unwrap();
        tx.send(ChatChunk { text: " world".into(), done: true }).await.unwrap();
    }))
}

// 客户端（与 Streaming 相同：ServerEvents 也实现 Stream trait）
let mut stream = chat_stream(msg()).await?;
while let Some(Ok(chunk)) = stream.next().await { /* ... */ }
```

`ServerEvents::new` 需要 `dioxus` 的 `server` feature（见"推荐 Feature 配置"）。

### 为什么 Agent 聊天用 ServerEvents 而不是 Streaming

`Streaming<T, E>` 底层是 **WebSocket 二进制帧**跑在 HTTP 流上：每个 chunk 编码为带长度前缀的帧。**经过网络（非 localhost）时**，TCP 分片会导致帧边界丢失 → 客户端解码失败。本地回环通常不分片，所以桌面开发时正常，一旦通过 IP 从其他设备访问就暴露问题。

| 对比 | `Streaming<T, E>` | `ServerEvents<T>` |
|------|-----|-----|
| 协议 | WS 二进制帧（长度前缀） | SSE 文本（`\n\n` 分隔） |
| 网络 TCP 分片 | 帧边界丢失，解码失败 | 拼接后按 `\n\n` 找边界，天然抗分片 |
| 跨设备访问 | ❌ 易出 `Decoding` 错误 | ✅ 稳定 |

**结论**：Agent 聊天用 `ServerEvents<T>`。`Streaming<T, E>` 仅在不需要抗分片或需要 CBOR/Postcard 编码时使用。

### Streaming\<T, E\>（备选）

```rust
#[get("/api/chat/stream?msg")]
async fn chat_stream(msg: String) -> Result<Streaming<ChatChunk, JsonEncoding>> {
    let (tx, rx) = futures::channel::mpsc::unbounded();
    tokio::spawn(async move {
        tx.unbounded_send(ChatChunk { text: "Hello".into(), done: false }).unwrap();
    });
    Ok(Streaming::new(rx))
}
```

## Chrome DevTools MCP 远程调试

Dioxus Desktop/Mobile 均使用 WebView（Chromium 内核），支持 CDP（Chrome DevTools Protocol），可通过 Chrome DevTools MCP 进行远程调试。

### Desktop（Windows）— 通过浏览器启动参数暴露 CDP

WebView2 支持 `--remote-debugging-port` 参数，直接在 TCP 端口暴露 CDP。**仅在 debug 构建中启用**，生产环境不应暴露调试端口：

```rust
fn main() {
    #[cfg(feature = "desktop")]
    {
        use dioxus::desktop::Config;
        let cfg = if cfg!(debug_assertions) {
            Config::new().with_windows_browser_args("--remote-debugging-port=9223")
        } else {
            Config::new()
        };
        dioxus::LaunchBuilder::desktop().with_cfg(cfg).launch(app);
    }

    #[cfg(not(feature = "desktop"))]
    dioxus::launch(app);
}
```

- `Config::with_windows_browser_args` 传递 WebView2 启动参数（仅 Windows）
- `#[cfg(debug_assertions)]` 确保仅在 debug 构建时启用调试端口，release 构建不暴露
- MCP 用 connect 模式连接 `localhost:9223`（与上方示例端口一致），无需额外步骤。注意：cdp-local 现为 launch 模式，连 webview 需单独的 connect 配置（见 cdp skill「连接已有 CDP 端点」）
- WebView2 使用应用私有 profile，不受 Chromium 136+ 默认 profile 禁用调试端口的影响，`--remote-debugging-port` 正常生效
- debug 模式下 Dioxus 自动调用 `webview.with_devtools(true)`（源码 `webview.rs:458`），启用右键菜单 DevTools

### Mobile（Android）— 通过 ADB 端口转发暴露 CDP

Android WebView 的调试机制与 Desktop 不同：

| | Desktop (Windows) | Mobile (Android) |
|---|---|---|
| WebView 引擎 | WebView2 (Edge/Chromium) | Android WebView (Chromium) |
| CDP 暴露方式 | `--remote-debugging-port` → TCP 端口 | `setWebContentsDebuggingEnabled(true)` → Unix socket |
| 需要代码配置 | 是（传浏览器参数） | 否（debug 模式自动启用） |
| MCP 连接前 | 无 | 需 `adb forward` 把 Unix socket 映射为 TCP |

**调试已自动启用（仅 debug 构建）**，三层保障：

1. **Kotlin 层**：`MainActivity.kt` 中 `WebView.setWebContentsDebuggingEnabled(BuildConfig.DEBUG)` — 仅 debug 构建启用
2. **Dioxus 层**：debug 模式下自动调用 `webview.with_devtools(true)`（`webview.rs:458`）
3. **Wry 层**：内部通过 JNI 调用 `setWebContentsDebuggingEnabled`（`main_pipe.rs:166`，条件 `#[cfg(any(debug_assertions, feature = "devtools"))]`）

**生产构建中三层均不启用**，不存在安全隐患。

**MCP 连接步骤**：

```bash
# 1. 发现 WebView 的 devtools socket
adb shell "cat /proc/net/unix | grep devtools"
# 输出示例：
#   @browser_webview_devtools_remote_17240    ← 系统浏览器，忽略
#   @webview_devtools_remote_18616            ← 应用 WebView，用这个
# 注：选 webview_devtools_remote_*（非 browser_ 前缀）。若有多个应用 socket，通常选最新（PID 最大）

# 2. ADB 端口转发（将 Unix socket 映射到主机 TCP 端口 9228）
adb forward tcp:9228 localabstract:webview_devtools_remote_18616

# 3. 验证 CDP 端点
curl http://localhost:9228/json
# 返回 JSON（含 webSocketDebuggerUrl）即表示 CDP 可用
```

之后 **cdp-remote MCP** 连接 `localhost:9228`。`take_snapshot`（1.8.0 起需显式传 `pageId`）、`evaluate_script`、`list_pages` 等均实测可用。

**⚠️ MCP 分工**：两个 MCP 对应不同连接方式，互不冲突：
- **cdp-remote MCP**（connect 模式，`--browser-url=http://127.0.0.1:9228`）：远程 CDP 服务端，经 adb forward（及 cdp-proxy）连接
- **cdp-local MCP**（**launch 模式**）：本机浏览器自动化 —— 自启/自闭 Edge、持久 profile 共享登录态；连接本机 webview 等已有 CDP 端点时另配 connect 条目

**注意**：`chrome://inspect` / `edge://inspect` 只是 CDP 的 UI 客户端，MCP 无法操作它。但 Android WebView 的 Unix socket 本身就是 CDP 协议，通过 `adb forward` 映射后 MCP 可直接连接，不需要 inspect 页面作为中间人。

### iOS — Safari Web Inspector

iOS WebView 通过 Safari Web Inspector 暴露调试接口，不支持 CDP。在 Mac 上通过 Safari > Develop > [设备] > [WebView] 打开 DevTools。Chrome DevTools MCP 无法连接 iOS WebView。

## Asset Handler 本地文件访问

Dioxus Mobile 的 WebView 基础 URL 是 `https://dioxus.index.html/`（虚拟域名），所有请求被 wry 的 `RustWebViewClient.shouldInterceptRequest` 拦截并交由 Rust 侧 `desktop_handler` 处理。`file:///` URL 因跨域和安全策略无法被 WebView 直接加载，需通过 asset handler 注册自定义协议来实现本地文件访问。

### 原理

```
WebView 请求 /local-file/sdcard/Document/a/img.png
  → wry shouldInterceptRequest 拦截
  → 匹配 "local-file" handler
  → Rust 读取 /sdcard/Document/a/img.png
  → 返回 HTTP Response（含 Content-Type）
```

关键点：
- `https://dioxus.index.html/` 是同源 URL，不存在 CORS 问题
- `/local-file/` 前缀请求会被 dioxus asset handler 路由到注册的处理器
- 处理器接收 `Request<Vec<u8>>`，通过 `responder.respond(Response<Vec<u8>>)` 返回文件内容

### 注册 Asset Handler

```rust
fn use_local_file_asset_handler() {
    dioxus::mobile::use_asset_handler("local-file", |request, responder| {
        let path_str = request.uri().path();
        let relative = path_str.strip_prefix("/local-file/").unwrap_or(path_str);
        let file_path = format!("/{relative}");

        match std::fs::read(&file_path) {
            Ok(content) => {
                let mime = mime_type_from_path(&file_path);
                let response = dioxus::mobile::wry::http::Response::builder()
                    .header("Content-Type", mime)
                    .body(content);
                match response {
                    Ok(r) => responder.respond(r),
                    Err(e) => {
                        tracing::error!("local-file handler: response build failed: {e}");
                        responder.respond(not_found_response());
                    }
                }
            }
            Err(e) => {
                tracing::debug!("local-file handler: file not found: {e}");
                responder.respond(not_found_response());
            }
        }
    });
}
```

### 图片 URL 转换

Markdown 渲染时，将相对路径图片转为 `/local-file/` 前缀 URL：

```rust
fn resolve_image_url(url: &str, base_dir: &Path) -> Option<String> {
    if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("data:") {
        return None;
    }

    let resolved = if url.starts_with('/') {
        PathBuf::from(url)
    } else {
        base_dir.join(url)
    };

    Some(format!(
        "/local-file/{}",
        resolved.to_string_lossy().trim_start_matches('/')
    ))
}
```

**注意**：`trim_start_matches('/')` 移除绝对路径开头的 `/`，避免生成 `/local-file//sdcard/...` 双斜杠路径。

### MIME 类型推断

```rust
fn mime_type_from_path(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("");
    match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "pdf" => "application/pdf",
        "md" => "text/markdown",
        "txt" => "text/plain",
        "html" => "text/html",
        "css" => "text/css",
        "js" => "application/javascript",
        "json" => "application/json",
        _ => "application/octet-stream",
    }
}
```

## Native 平台非冒泡事件不触发（scroll/focus/blur 等）

**Desktop/Mobile 上 `onscroll`、`onfocus`、`onblur` 等 Dioxus 事件处理器不会触发。** 这是 Dioxus native interpreter 的 bug（`native.ts` 硬编码 `bubbles=true`），导致所有非冒泡事件在 JS→Rust 传递时冒泡属性被篡改，Rust 侧走错分发路径。

- **根因分析、绕行方案** → 详见 [native-non-bubbling-events.md](native-non-bubbling-events.md)
- **缺陷记录** → 详见 `mt-note-dioxus-bugs` skill 缺陷 4

### 受影响的完整事件列表

`core-types/bubbles.rs` 中 `bubbles=false` 的所有事件均不触发：

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

**冒泡事件（onclick、oninput、onchange、onkeydown 等）不受影响**，可正常使用 Dioxus 事件系统。

### 正确做法

不要依赖 Dioxus 的 `onscroll`/`onfocus`/`onblur` 等，改用 **JS 侧原生 `addEventListener` + `js-bridge-bindgen`** 通信绕行。

## Native JS/Rust IPC 交互与性能

Dioxus Native（Desktop/Mobile）的 JS↔Rust 通信与 Web (wasm) 完全不同，有显著 IPC 开销：

- **JS→Rust（事件）**：同步 XHR POST `/__events`，事件数据 base64 编码放 header，**阻塞 JS 主线程**等 Rust 处理完返回
- **Rust→JS（DOM 更新）**：WebSocket 二进制 + `requestAnimationFrame` 批处理，设计高效

**高频事件（scroll/mousemove/wheel/input）禁止使用 Dioxus 事件系统**，同步 XHR 每帧阻塞会导致严重卡顿，改用 JS 侧原生监听 + `js-bridge-bindgen` 绕行。低频事件（click/change/submit）可安全使用。

- **完整架构图、源码追踪、序列化细节、性能优化建议** → 详见 [native-js-rust-ipc.md](native-js-rust-ipc.md)

## CustomEvent + Promise JS→Rust 通信

Dioxus Mobile（native WebView）中 JS 与 Rust 的通信机制与 Web (wasm) 不同：native 上没有 `web_sys`，JS→Rust 通信必须通过 `document::eval` + DOM 事件实现。**禁止使用定时轮询**（`setInterval`/`sleep` 循环），必须采用事件驱动模式。

### 通信架构

```
JS 侧                          Rust 侧
─────────                      ────────
用户点击链接
  → e.preventDefault()
  → dispatchEvent(CustomEvent)
                                document::eval(Promise)
                                  → await 等待事件
                                  → 解析 JSON
                                  → 处理业务逻辑
                                  → 循环回到 await
```

### JS 侧：派发 CustomEvent

```javascript
el.addEventListener('click', function(e) {
    var localLink = e.target.closest('a:not([href^="#"])');
    if (localLink) {
        var href = localLink.getAttribute('href');
        if (href && !href.startsWith('http://') && !href.startsWith('https://')) {
            e.preventDefault();
            e.stopPropagation();
            try { href = decodeURIComponent(href); } catch(ex) {}
            var base = el.dataset.mdBasePath || '';
            try { base = decodeURIComponent(base); } catch(ex) {}
            window.dispatchEvent(new CustomEvent('link-click', {
                detail: { href: href, base: base }
            }));
        }
    }
});
```

关键点：
- `e.preventDefault()` + `e.stopPropagation()` 阻止 WebView 默认导航行为
- `decodeURIComponent` 解码 URL 编码的中文路径
- `dataset.mdBasePath` 存储当前 Markdown 文件的目录路径，用于解析相对链接

### Rust 侧：Promise 等待事件

```rust
use_effect(move || {
    let nav = nav;
    spawn(async move {
        loop {
            let result = document::eval(
                "return (function(){\
                 if(!window.__lcq){\
                 window.__lcq=[];\
                 window.addEventListener('link-click',function(e){\
                 var d=e.detail||{};\
                 var p=JSON.stringify({href:d.href||'',base:d.base||''});\
                 if(window.__lcr){window.__lcr(p);window.__lcr=null;}\
                 else{window.__lcq.push(p);}\
                 });\
                 }\
                 return new Promise(function(resolve){\
                 if(window.__lcq.length>0){resolve(window.__lcq.shift());}\
                 else{window.__lcr=resolve;}\
                 });\
                 })()",
            )
            .join::<Option<String>>()
            .await;

            match result {
                Ok(Some(json)) => {
                    let parsed: serde_json::Value = serde_json::from_str(&json)?;
                    let href = parsed["href"].as_str().unwrap_or("");
                    let base = parsed["base"].as_str().unwrap_or("");
                    if !href.is_empty() {
                        let resolved_path = resolve_path(href, base);
                        if resolved_path.exists() {
                            nav.push(Route::Editor { path: encode(&resolved_path) });
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => tracing::error!("link-click eval error: {e:?}"),
            }
        }
    });
});
```

### Promise 队列机制详解

JS 侧的 `window.__lcq`（队列）和 `window.__lcr`（回调）实现了生产者-消费者模式，解决事件时序竞争问题：

```
场景 1：事件先于 Promise 到达
  CustomEvent 触发 → __lcq.push(data)
  Promise 创建 → __lcq 有数据 → 立即 resolve(__lcq.shift())

场景 2：Promise 先于事件创建
  Promise 创建 → __lcq 为空 → __lcr = resolve（注册回调）
  CustomEvent 触发 → __lcr 存在 → __lcr(data)，__lcr = null
```

**为什么不用 `addEventListener` 直接回调？** Dioxus 的 `document::eval` 返回的是一次性 Promise，无法注册持久的 JS→Rust 回调。每次 `await` 完成后需要重新 `eval` 创建新 Promise，循环等待下一个事件。

**为什么不用轮询？** 轮询（`setInterval` + Rust 侧定时读取）浪费 CPU、引入延迟、且违反事件驱动范式。Promise 模式是零开销的——没有事件时 Rust 侧的 Future 被 suspend，不消耗任何资源。

### 通用模式提取

此模式适用于所有 JS→Rust 单向通信场景：

1. **JS 侧**：`window.dispatchEvent(new CustomEvent('event-name', { detail: data }))`
2. **Rust 侧**：`document::eval(Promise)` → `await` → 处理 → 循环

扩展为多事件类型时，可在 `detail` 中添加 `type` 字段，或在 JS 侧注册多个事件名，Rust 侧用多个 `spawn` 循环分别监听。

## js-bridge-bindgen：TS → Rust 类型安全绑定

`js-bridge-bindgen` 是项目的代码生成工具，从 TypeScript 源码自动生成类型安全的 Rust 绑定代码。**它是 `CustomEvent + Promise` 模式的现代化替代方案**——不再需要手写内联 JS 字符串、手动 JSON 序列化/反序列化、手动管理 Promise 队列，而是通过 TS 接口定义 + 代码生成，自动获得类型安全的 Rust API。

### 架构总览

```
┌─────────────────────────────────────────────────────────────────┐
│  TS 项目 (crates/mobile/ts/)                                     │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐                       │
│  │editor.ts │  │scroll.ts │  │...其他   │  ← 纯 TS 模块，导出函数  │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘                       │
│       └──────────────┼─────────────┘                             │
│                      ▼                                           │
│              index.ts（入口）                                     │
│              - 导入所有模块函数                                    │
│              - 定义 export interface（API 类型）                   │
│              - export function createXxx(): Xxx（工厂函数）        │
│              - declare global { Window }（全局注册）               │
│              - window.__mt_note_mobile = { ... }（运行时赋值）     │
│                      │                                           │
│              pnpm build ← build.rs 调用                           │
│                      ▼                                           │
│              dist/mt-note-mobile-ts.js（JS bundle）               │
└─────────────────────────────────────────────────────────────────┘
                       │
                       ▼ build.rs 阶段
┌─────────────────────────────────────────────────────────────────┐
│  js-bridge-bindgen::generate()                                   │
│  1. oxc_parser 解析 index.ts → AST                               │
│  2. 提取 factory_functions、interfaces、global_decl              │
│  3. 校验工厂模式（validate）                                      │
│  4. 生成 OUT_DIR/js_api.rs                                       │
└─────────────────────────────────────────────────────────────────┘
                       │
                       ▼ include! 嵌入
┌─────────────────────────────────────────────────────────────────┐
│  Rust 侧 (crates/mobile/src/frontend/js_bridge.rs)               │
│  - struct DioxusBridge; impl JsBridge { ... }                    │
│  - pub mod js_api { include!(concat!(env!("OUT_DIR"), "/js_api.rs")); } │
│                                                                  │
│  调用方:                                                         │
│  js_api::scroll::create::<DioxusBridge>();          // 初始化    │
│  js_api::scroll::set_home_scroll_top::<DioxusBridge>(42.0);     │
└─────────────────────────────────────────────────────────────────┘
```

### 工厂模式（Factory Pattern）

`js-bridge-bindgen` 要求 TS 源码遵循**工厂模式**。每个 JS 模块通过一个工厂函数创建实例，实例存储在 `window.__xxx` 变量中，Rust 侧通过生成的代码调用实例方法。

**TS 入口文件（index.ts）必须包含以下四个部分**：

#### 1. 导出接口（export interface）—— 定义 API 类型

```ts
export interface Scroll {
  setHomeScrollTop: (scrollTop: number) => void;
}
```

- 接口名 = 工厂函数返回类型名
- 方法签名 = Rust 侧可调用的 API
- 返回类型必须是可序列化的：`void`、`string`、`number`、`boolean`、`Promise<T>`
- 嵌套对象属性会生成 `_` 分隔的 Rust 函数名（如 `render.load` → `render_load`）

#### 2. 工厂函数（export function createXxx）—— 创建实例

```ts
export function createScroll(): Scroll {
  return {
    setHomeScrollTop,
  };
}
```

- 命名规则：`create` + 接口名（如 `createScroll` → 接口 `Scroll`）
- 返回一个实现接口的对象字面量
- 函数体直接引用子模块的导出函数

#### 3. 全局声明（declare global）—— 注册到 Window

```ts
declare global {
  interface Window {
    __mt_note_mobile: {
      createScroll: () => Scroll;
      // ... 其他工厂函数
    };
  }
}
```

- `__mt_note_mobile` 是全局工厂对象名（由 `Config.global_factory_name` 指定）
- 每个工厂函数在此声明为工厂对象的方法

#### 4. 运行时赋值（window.__mt_note_mobile = ...）—— 挂载工厂

```ts
window.__mt_note_mobile = {
  createScroll,
  // ... 其他工厂函数
};
```

### 子模块（纯 TS 文件）

子模块如 `scroll.ts` 只需导出纯函数，不涉及工厂模式：

```ts
// scroll.ts —— 纯 TS 模块，导出函数即可
export function setHomeScrollTop(scrollTop: number): void {
  const el = document.getElementById("home_view");
  if (el) {
    el.scrollTop = scrollTop;
  }
}
```

### build.rs 集成

```rust
fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("...");
    let out_dir = std::env::var("OUT_DIR").expect("...");

    let ts_dir = format!("{manifest_dir}/ts");
    let ts_entry = format!("{ts_dir}/src/index.ts");

    if std::path::Path::new(&ts_entry).exists() {
        // 1. 安装依赖（如需要）
        if !std::path::Path::new(&format!("{ts_dir}/node_modules")).exists() {
            pnpm_run("install", &ts_dir);
        }
        // 2. 增量编译 TS → JS bundle
        if need_rebuild(&ts_dir, &ts_dist) {
            pnpm_run("build", &ts_dir);
        }
        // 3. 生成 Rust 绑定代码
        js_bridge_bindgen::generate(&js_bridge_bindgen::Config {
            entry: ts_path.to_path_buf(),
            out_dir: std::path::PathBuf::from(&out_dir),
            global_factory_name: "__mt_note_mobile".into(),
            ..Default::default()
        }).expect("js-bridge-bindgen codegen failed");
    }
}
```

### 生成的 Rust 代码结构

对于 `export function createScroll(): Scroll`，生成的代码：

```rust
pub mod scroll {
    pub fn create<B: super::JsBridge>() {
        B::ensure_injected();
        B::eval_js(&format!(
            "if(!{g}.__scroll){g}.__scroll={g}.__mt_note_mobile.createScroll()}",
            g = B::GLOBAL_OBJECT
        ));
    }

    pub fn set_home_scroll_top<B: super::JsBridge>(scroll_top: f64) {
        B::ensure_injected();
        B::eval_js(&format!(
            "if({g}.__scroll){g}.__scroll.setHomeScrollTop({scroll_top})",
            g = B::GLOBAL_OBJECT, scroll_top = scroll_top
        ));
    }

    pub fn destroy<B: super::JsBridge>() {
        // 清理实例
    }
}
```

关键细节：
- **实例变量名**：`createScroll` → `__scroll`（去掉 `create` 前缀，首字母小写，加 `__` 前缀）
- **create 幂等**：`if(!{g}.__scroll)` 保证只创建一次
- **方法调用守卫**：`if({g}.__scroll)` 确保实例存在才调用
- **字符串参数**：自动对 `'` 做转义（`replace("'", "\\'")`）
- **结构体参数**：自动 `serde_json::to_string` + `JSON.parse`
- **异步返回**：`Promise<T>` 返回类型的方法生成 `async fn`，使用 `eval_async`

### JsBridge trait 实现

```rust
use js_bridge_bindgen::JsBridge;

pub struct DioxusBridge;

impl JsBridge for DioxusBridge {
    const GLOBAL_OBJECT: &str = "window";

    fn eval_js(js: &str) {
        dioxus::document::eval(js);
    }

    async fn eval_async(js: &str) -> serde_json::Value {
        let wrapped = format!("return (async()=>{{{js}}})()");
        dioxus::document::eval(&wrapped)
            .join::<serde_json::Value>()
            .await
            .unwrap_or(serde_json::Value::Null)
    }

    fn ensure_injected() {
        // 确保 JS bundle 已注入运行时（幂等）
        if !BUNDLE_INJECTED.load(Ordering::Relaxed) {
            dioxus::document::eval(&ENSURE_BUNDLE_JS);
            BUNDLE_INJECTED.store(true, Ordering::Relaxed);
        }
    }
}

// 嵌入生成的代码
pub mod js_api {
    include!(concat!(env!("OUT_DIR"), "/js_api.rs"));
}
```

### Rust 侧调用模式

```rust
use crate::frontend::js_bridge::{js_api, DioxusBridge};

// 1. 应用初始化时创建所有 JS 模块实例（幂等，可多次调用）
use_effect(move || {
    js_api::scroll::create::<DioxusBridge>();
    js_api::editor::create::<DioxusBridge>();
    // ... 其他模块
});

// 2. 业务代码中调用方法
js_api::scroll::set_home_scroll_top::<DioxusBridge>(target);
```

### 与 CustomEvent + Promise 模式对比

| 特性 | CustomEvent + Promise | js-bridge-bindgen |
|------|----------------------|-------------------|
| JS 代码位置 | 内联 Rust 字符串 | 独立 `.ts` 文件 |
| 类型安全 | ❌ 手动 JSON 解析 | ✅ 自动生成 Rust 类型 |
| 语法高亮 | ❌ Rust 字符串内无高亮 | ✅ VS Code 完整 TS 支持 |
| 参数传递 | 手动序列化 | 自动 `serde_json` + 转义 |
| 返回值处理 | 手动 `serde_json::from_str` | 自动反序列化 |
| 错误检测 | 运行时才发现 | 编译时校验 |
| 代码复用 | 复制粘贴 | 子模块导出 |
| 校验 | 无 | 8 条工厂模式校验规则 |

### 校验规则

`js-bridge-bindgen` 在 build.rs 阶段对 TS 源码进行 8 条校验：

| 规则 | 说明 |
|------|------|
| E001 | 必须有 `declare global` 块 |
| E003 | `Window` 中必须存在工厂对象 |
| E004 | 工厂对象属性必须是函数类型 |
| E005 | 工厂方法必须有对应的 `export function` |
| E006 | 工厂函数返回类型必须有对应的 `export interface` |
| E007 | 实例变量类型必须与工厂函数返回类型一致 |
| E008 | 所有方法必须有显式返回类型标注 |
| E009 | 返回类型必须是可序列化的 |

### TS 类型到 Rust 类型映射

| TS 类型 | Rust 类型 |
|---------|----------|
| `void` | `()` |
| `string` | `String` |
| `number` | `f64` |
| `boolean` | `bool` |
| `Promise<T>` | `T`（async fn） |
| `Promise<void>` | `()`（async fn） |
| `string \| Promise<string>` | `String`（async fn） |
| `T \| null` | `Option<T>` |
| `interface { ... }` | `struct`（derive Serialize/Deserialize） |
| `export type Xxx` | 类型别名引用 |

### 新增 JS 模块的完整步骤

1. **创建子模块**：`ts/src/new_module.ts`，导出纯函数
2. **更新 index.ts**：
   - 添加 `import { ... } from "./new_module"`
   - 添加 `export interface NewModule { ... }`
   - 添加 `export function createNewModule(): NewModule { ... }`
   - 在 `declare global` 中注册工厂方法
   - 在 `window.__mt_note_mobile = { ... }` 中添加字段
3. **在 app.rs 初始化**：`js_api::new_module::create::<DioxusBridge>();`
4. **在业务代码中调用**：`js_api::new_module::method::<DioxusBridge>(args);`

## 打包分发（dx bundle）

### 基本用法

```bash
dx bundle --desktop -r --package-types nsis    # Windows NSIS 安装器
dx bundle --desktop -r --package-types msi      # Windows MSI 安装包
dx bundle --android -r --package-types apk      # Android APK
dx bundle --android -r --package-types aab      # Android AAB
```

- **默认是 debug 模式**，发布必须加 `-r`（`--release`）
- 不指定 `--package-types` 时，Windows 默认 `nsis`，Android 默认 `aab`

### ⚠️ Android Release 打包

dioxus-cli 0.7.10 的 `assemble_android` 有隐藏条件：**必须 Dioxus.toml 存在 `[bundle.android]` 签名配置，`dx bundle -r` 才会调用 Gradle 的 `assembleRelease`**，否则即使传了 `-r` 也静默降级为 `assembleDebug`。

`[bundle.android]` 四个字段全部必填（`jks_file`/`jks_password`/`key_alias`/`key_password`），缺一不可。

如果不想将真实密钥密码写入 Dioxus.toml，可用**临时 JKS + apksigner 覆盖**方案：

```bash
# 1. 生成临时 JKS（一次性，密码随意）
keytool -genkey -v -keystore dioxus-build.jks -alias temp \
  -keyalg RSA -keysize 2048 -validity 10000 \
  -storepass temp123 -keypass temp123 \
  -dname "CN=Build"

# 2. Dioxus.toml 追加
# [bundle.android]
# jks_file = "crates/mobile/dioxus-build.jks"
# jks_password = "temp123"
# key_alias = "temp"
# key_password = "temp123"

# 3. .gitignore 忽略 JKS 文件
# **/dioxus-build.jks

# 4. 构建后用正式 keystore 覆盖签名
dx bundle -p my-app --android -r --package-types apk
apksigner sign --ks <你的-release-keystore> --ks-key-alias <key-alias> \
  --ks-pass pass:<real_pwd> --key-pass pass:<real_pwd> \
  --out signed.apk app-release.apk
```

### 支持的包类型

| 参数值 | 说明 | 适用平台 |
|--------|------|----------|
| `nsis` | Windows NSIS 安装器 (.exe) | Windows |
| `msi` | Windows MSI 安装包 | Windows |
| `dmg` | macOS DMG 镜像 | macOS |
| `macos` | macOS .app 包 | macOS |
| `deb` | Linux Debian 包 | Linux |
| `rpm` | Linux RPM 包 | Linux |
| `appimage` | Linux AppImage | Linux |
| `apk` | Android APK | Android |
| `aab` | Android AAB | Android |
| `ipa` | iOS IPA | iOS |
| `ios` | iOS .app 包 | iOS |
| `updater` | 增量更新包 | 通用 |

### NSIS vs MSI（Windows）

| 特性 | NSIS | MSI |
|------|------|-----|
| 格式 | `.exe` 安装器 | `.msi` Windows Installer 包 |
| 技术 | Nullsoft 脚本安装系统 | Windows Installer（微软原生） |
| 界面 | 自定义 UI，灵活美观 | 标准 Windows 安装向导，较朴素 |
| 默认安装路径 | `C:\Users\用户\AppData\Local\应用名\` | `C:\Program Files\应用名\` |
| 需要管理员 | ❌ 否（默认 CurrentUser） | ✅ 是（硬编码 perMachine） |
| 企业 GPO 部署 | ❌ 不支持 | ✅ 支持 |
| WebView2 安装 | ✅ 内嵌（默认 OfflineInstaller） | ❌ 不包含 |
| 体积 | 大（含 WebView2 运行时） | 小（仅应用本身） |
| 可配置安装模式 | ✅ `install_mode` | ❌ 需自定义 WiX 模板 |

### NSIS 安装模式配置

```toml
[windows.nsis]
install_mode = "CurrentUser"   # 默认，装到用户目录，无需管理员
# install_mode = "PerMachine"  # 装到 Program Files，需要管理员
# install_mode = "Both"        # 安装时让用户选择
```

### WebView2 安装模式（仅影响 NSIS，MSI 不受影响）

NSIS 默认嵌入 WebView2 完整离线安装器（~160MB），是体积大的主要原因。`webview_install_mode` **只对 NSIS 生效**——MSI 的 WiX 模板中没有 WebView2 安装逻辑，完全忽略此配置。

**⚠️ 配置位置**：必须在 `[bundle.windows]` 下，不是 `[windows]`。Dioxus CLI 存在两个配置层级：`[windows]`（`DioxusConfig.windows`）和 `[bundle.windows]`（`BundleConfig.windows`），bundler 只读取后者。

在 `Dioxus.toml` 中配置：

```toml
[bundle.windows]
# 内嵌完整 WebView2 运行时，离线可用（默认，~170MB）
webview_install_mode = "OfflineInstaller"

# 安装时在线下载 WebView2，需联网（~7MB）
webview_install_mode = "DownloadBootstrapper"

# 内嵌引导程序，下载安装 WebView2（~8MB）
webview_install_mode = "EmbedBootstrapper"

# 跳过，假设用户已装 WebView2（~6MB）
webview_install_mode = "Skip"

# 指定固定 WebView2 运行时路径
webview_install_mode = { FixedRuntime = "C:\\path\\to\\runtime" }
```

**注意**：Dioxus 的 TOML 格式与 Tauri 不同——Tauri 用 `{ type = "Skip" }`，Dioxus 直接用字符串变体名 `"Skip"`。`FixedRuntime` 是唯一需要内联表的变体，格式为 `{ FixedRuntime = "路径" }`。

| 模式 | 安装器体积 | 联网要求 | 适用场景 |
|------|-----------|---------|---------|
| `OfflineInstaller`（默认） | ~170MB | 不需要 | 内网/离线环境 |
| `DownloadBootstrapper` | ~7MB | 需要 | 一般桌面应用 |
| `EmbedBootstrapper` | ~8MB | 需要 | 需要引导程序 |
| `Skip` | ~6MB | 不需要 | Win10/11 已自带 WebView2 |
| `FixedRuntime` | 取决于 runtime | 不需要 | 企业指定运行时版本 |

**MSI 不包含 WebView2 安装逻辑**（WiX 模板未实现），仅打包应用本身（~5.8MB），安装后依赖系统已装的 WebView2。

### MSI per-user 安装

MSI 默认 `InstallScope="perMachine"`（硬编码在 WiX 模板中），Dioxus 未提供配置项。如需 per-user 安装，需自定义 WiX 模板：

```toml
[windows.wix]
template = "custom-wix-template.wxs"
```

在自定义模板中将 `InstallScope="perMachine"` 改为 `InstallScope="perUser"`，`ProgramFiles64Folder` 改为 `LocalAppDataFolder`。**建议直接用 NSIS**，天然支持普通用户安装。

### 交叉编译 server（fullstack）

`--target` 必须配合 `--server` 才生效，但 `--server` **只编译 server 二进制，不编译 client 的 wasm/js**（public 目录缺少 wasm，前端事件系统无法启动）。需分两步：

```bash
# 1. 交叉编译 server（仅 server，不含 client wasm）
dx bundle -p agent-cli-web --target aarch64-linux-android -r --server

# 2. 编译完整 public（含 wasm/js）—— 不用 --target 和 --server
dx bundle -p agent-cli-web -r
```

部署时将第 2 步生成的 `public/` 目录复制到第 1 步 server 同级目录。