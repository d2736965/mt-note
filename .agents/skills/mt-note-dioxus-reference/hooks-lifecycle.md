# Dioxus 0.7 Hooks 生命周期（源码追踪）

基于 Dioxus 0.7.9 源码（`~/pkgs/downloads/dioxus/packages/`），追踪 hooks 在组件渲染生命周期中的精确位置。

## 1. Scope 是什么

**Scope = 组件实例的运行时容器**。每次你在 RSX 里写 `<Home />`，Dioxus 在内部 slab 里分配一个 Scope。

### 两个结构体

**`Scope`**（`scope_context.rs:45`）—— 轻量元数据，存 hooks 列表、状态、回调：

```rust
pub(crate) struct Scope {
    pub(crate) name: &'static str,                           // 组件名
    pub(crate) id: ScopeId,                                  // 唯一 ID
    pub(crate) parent_id: Option<ScopeId>,                   // 父组件
    pub(crate) height: u32,                                  // 树深度
    pub(crate) render_count: Cell<usize>,                    // 渲染次数

    // 注意顺序：hooks 必须在 shared_contexts 之前 drop（hooks drop 时可能访问 context）
    pub(crate) hooks: RefCell<Vec<Box<dyn Any>>>,            // 所有 use_xxx 的状态
    pub(crate) hook_index: Cell<usize>,                      // 当前 hooks 游标
    pub(crate) shared_contexts: RefCell<Vec<Box<dyn Any>>>,  // use_provide_context 提供
    pub(crate) spawned_tasks: RefCell<FxHashSet<Task>>,     // spawn 的 Future
    pub(crate) before_render: RefCell<Vec<Box<dyn FnMut()>>>, // use_before_render
    pub(crate) after_render: RefCell<Vec<Box<dyn FnMut()>>>,  // use_after_render
    pub(crate) status: RefCell<ScopeStatus>,                 // Mounted / Unmounted
}
```

**`ScopeState`**（`scopes.rs:73`）—— 持有 props、reactive_context、last_rendered_node：

```rust
pub struct ScopeState {
    pub(crate) runtime: Rc<Runtime>,              // 共享的 runtime
    pub(crate) context_id: ScopeId,               // 自身 ID
    pub(crate) last_rendered_node: Option<...>,   // 上次渲染的 VNode
    pub(crate) props: BoxedAnyProps,              // 组件 props
    pub(crate) reactive_context: ReactiveContext, // 信号订阅/脏标记
}
```

### ScopeId 预设值（`scopes.rs:47-61`）

```rust
ScopeId(0) = ROOT                    // 根包装器
ScopeId(1) = ROOT_SUSPENSE_BOUNDARY  // 默认 Suspense 边界
ScopeId(2) = ROOT_ERROR_BOUNDARY     // 默认 Error 边界
ScopeId(3) = APP                     // 用户的 app() 组件
```

### 组件树示例

```
ScopeId(3)  <App />
  ├─ ScopeId(12) <Layout />
  │    ├─ ScopeId(31) <TopAppBar />
  │    ├─ ScopeId(27) <Home />      ← 你的组件
  │    │    ├─ ScopeId(55) <Chart />
  │    │    └─ ScopeId(56) <Sidebar />
  │    └─ ScopeId(32) <BottomBar />
  └─ ScopeId(13) <Footer />
```

---

## 2. `run_scope` — 组件渲染完整流程

源码：`scope_arena.rs:47-105`

```
run_scope(scope_id):
  │
  ├─ 1. hook_index.set(0)                    // 重置 hooks 游标
  │
  ├─ 2. before_render hooks 执行              // line 59-61
  │     for pre_run in before_render { pre_run(); }
  │
  ├─ 3. ★ props.render()                     // line 68 — 组件函数体
  │     │                                     // 在 reactive_context.reset_and_run_in 内
  │     ├─ use_signal → use_hook(|| CopyValue::new(...))
  │     ├─ use_effect(callback)
  │     │   ├─ use_hook(|| {
  │     │   │   ├─ ReactiveContext::new_with_origin()
  │     │   │   ├─ queue_effect_for_next_render()
  │     │   │   │   └→ queue_effect(|| { rc.reset_and_run_in(|| callback(())); })
  │     │   │   │       └→ Runtime::queue_effect(id, f)
  │     │   │   │           ├─ scope.Mounted?  → pending_effects.insert()
  │     │   │   │           └─ scope.Unmounted? → effects_queued.push()
  │     │   │   └─ spawn(async { changed.next().await; queue_effect_for_next_render(); })
  │     │   })
  │     ├─ use_memo, use_resource, use_context, ...
  │     └─ rsx! { ... } → 返回 Element(VNode tree)
  │
  ├─ 4. after_render hooks 执行               // line 96-98
  │     for post_run in after_render { post_run(); }
  │
  └─ 5. dirty_scopes.remove(scope)            // line 101-102 — 标记为干净
```

**关键点**：步骤 3 的 `props.render()` 就是你的组件函数体。每次重渲染都会走到这里。

---

## 3. 各 Hook 在生命周期中的执行时机

| Hook | 注册位置 | 回调首次执行 | 回调重执行条件 | Scope 销毁时 |
|------|---------|-------------|---------------|:---:|
| `use_signal` | 🅐 函数体 (`use_hook`) | —（无回调） | — | drop |
| `use_effect` | 🅐 函数体 (`use_hook`) | **poll_tasks 阶段** | `ReactiveContext::changed` 触发 | ❌ 不执行 |
| `use_memo` | 🅐 函数体 (`use_hook`) | 🅐 函数体（立即） | 依赖信号变化 → mark_dirty | drop |
| `use_resource` 外部闭包 | 🅐 函数体 | 🅐 函数体（立即返回） | 🅐 每次 rerun | drop |
| `use_resource` 异步块 | 🅐 函数体 (`spawn`) | poll_tasks(task) | `changed` 触发重新 spawn | 随 task 取消 |
| `use_future` | 🅐 函数体 (`use_hook`) | poll_tasks(task) | 新 future 时重新 spawn | 随 task 取消 |
| `use_before_render` | 🅐 函数体 | 下次 🅐 之**前** | 每次 🅐 | ❌ |
| `use_after_render` | 🅐 函数体 | 本次 🅐 之**后** | 每次 🅐 | ❌ |
| `use_drop` | 🅐 函数体 | — | — | ✅ Drop 触发 |
| `use_callback` | 🅐 函数体 | —（返回闭包） | — | drop |
| 事件处理器 (onclick) | 🅐 函数体 | 用户交互触发 | 用户交互触发 | ❌ |
| **组件函数体** | — | 🅐 首次渲染 | 🅐 每次 rerun | ✅ 最后清理渲染 |

---

## 4. 源码详解：`use_effect` 的完整链路

### 4.1 注册阶段（`hooks/src/use_effect.rs:11-50`）

```rust
pub fn use_effect(mut callback: impl FnMut() + 'static) -> Effect {
    let callback = use_callback(move |_| callback());   // 包装为 FnMut(())
    let location = std::panic::Location::caller();

    use_hook(|| {
        // 创建独立的 ReactiveContext 用于追踪 effect 内部的信号读取
        let (rc, mut changed) = ReactiveContext::new_with_origin(location);

        let effect_queued = Rc::new(Cell::new(false));

        // 去重：多次触发只排队一次
        let queue_effect_for_next_render = move || {
            if effect_queued.get() { return; }
            effect_queued.set(true);
            queue_effect(move || {
                rc.reset_and_run_in(|| callback(()));   // ★ 真正执行 callback
                effect_queued.set(false);
            });
        };

        queue_effect_for_next_render();                 // 首次排队
        spawn(async move {
            loop {
                let _ = changed.next().await;            // 等待依赖变化
                queue_effect_for_next_render();          // 重新排队
            }
        });

        Effect { rc }
    })
}
```

### 4.2 `queue_effect` → Runtime（`scope_context.rs:326` → `tasks.rs:214`）

```rust
// ScopeState
pub(crate) fn queue_effect(&self, f: impl FnOnce() + 'static) {
    Runtime::with(|rt| rt.queue_effect(self.id, f));
}

// Runtime
pub(crate) fn queue_effect(&self, id: ScopeId, f: impl FnOnce() + 'static) {
    let Some(scope) = self.try_get_state(id) else { return; };
    let mut status = scope.status.borrow_mut();
    match &mut *status {
        ScopeStatus::Mounted => {
            self.queue_effect_on_mounted_scope(id, Box::new(f));  // → pending_effects
        }
        ScopeStatus::Unmounted { effects_queued, .. } => {
            effects_queued.push(Box::new(f));                     // 等 mount 后 drain
        }
    }
}
```

### 4.3 `ScopeStatus` 状态机

```
创建 scope
  → ScopeStatus::Unmounted { effects_queued: vec![] }
       │
       │ run_scope() + diff_scope() 完成
       ↓
  → ScopeStatus::Mounted
       │  effects_queued 被 drain 到 pending_effects
       │
       │ ScopeState::drop()
       ↓
  → scope 被移除
```

### 4.4 Effect 回调的实际执行位置

Effect 回调**不在** `run_scope` 的任何阶段执行。它在 Scheduler 的主循环中：

**`virtual_dom.rs:507-537` — `poll_tasks()`**：

```
poll_tasks():
  while dirty_tasks.not_empty() || pending_effects.not_empty():
    │
    ├─ 先跑所有 task                                    // 优先级 2
    │   while let Some(task) = pop_task():
    │     handle_task_wakeup(task)
    │     if has_dirty_scopes() → return (重新回到 scope 渲染)
    │
    └─ 再跑所有 effect                                  // 优先级 3（最低）
        while let Some(effect) = pop_effect():
          effect.run()                                   // ★ use_effect 回调在这里
          if has_dirty_scopes() → return
```

**Scheduler 优先级**（`scheduler.rs:67-77`）：

```
1. Dirty Scopes（组件重渲染）— 最高，根→叶顺序
2. Tasks（spawn 的 Future poll）— 中等
3. Effects（use_effect 回调）— 最低，DOM 更新完成后
```

---

## 5. Scope 创建与销毁

### 5.1 创建（`scope_arena.rs:11-42`）

```
VirtualDom::new_scope(props, name):
  ├─ 确定 parent_id 和 height
  ├─ 在 self.scopes slab 中分配 vacant entry → ScopeId
  ├─ 创建 Scope（runtime 元数据，status=Unmounted）
  ├─ 创建 ScopeState（props + reactive_context）
  └─ runtime.create_scope(scope_runtime)
```

### 5.2 Mount（`component.rs:58,87`）

```
run_and_diff_scope(scope):
  run_scope(scope) → new_nodes
  diff_scope(to, scope, new_nodes):
    diff old vs new VNodes
    if rendered:
      scope.mount(&runtime)        // ★ Unmounted → Mounted
        └─ effects_queued.drain() → pending_effects
  finish_render()                  // 发 EffectQueued 信号
```

### 5.3 销毁（`runtime.rs:179-206`）

```
ScopeState::drop()                           // scopes.rs:155
  └→ runtime.remove_scope(id):
       │
       ├─ 1. 取消所有 spawned_tasks
       │     for id in spawned_tasks.take():
       │       remove_task(id)
       │
       ├─ 2. 移除 pending_effects            // ★ effect 回调不执行
       │     pending_effects.remove(scope_order)
       │
       ├─ 3. 逆序 drop hooks                // hooks.take().drain().rev()
       │     ├─ use_drop 的 LifeCycle::drop 触发（ondestroy 回调）
       │     ├─ use_signal 的 CopyValue drop
       │     ├─ use_effect 的 Effect{rc} drop（callback 不执行）
       │     └─ use_memo, use_callback, ... drop
       │
       ├─ 4. 清空 shared_contexts
       │     shared_contexts.take()
       │
       └─ 5. 移除 scope
             scope_states[id].take()
```

**关键**：步骤 2 和步骤 3 是分开的——`pending_effects` 先被移除（不执行），然后 hooks 才被 drop。所以 `use_effect` 的 callback **绝对不可能**在 scope 销毁时执行。

---

## 6. 完整时序图

```
┌── 首次渲染 ──────────────────────────────────────────────────────────┐
│                                                                       │
│  组件首次 RSX 中出现                                                   │
│    ↓                                                                  │
│  new_scope(props, "Home") → ScopeId(27) [Unmounted]                   │
│    ↓                                                                  │
│  run_scope(27):                                                       │
│    ├─ before_render hooks                                             │
│    ├─ props.render()    ← 函数体：注册 use_signal/use_effect/...      │
│    │   use_effect: queue_effect → Unmounted → effects_queued.push()   │
│    ├─ after_render hooks                                              │
│    └─ scope 标记为干净                                                 │
│    ↓                                                                  │
│  create_scope(to, 27, new_nodes, parent):                             │
│    ├─ VNode 挂到 DOM                                                  │
│    └─ scope.mount(&runtime) → Mounted → effects_queued → pending      │
│    ↓                                                                  │
│  finish_render() → 发 EffectQueued                                    │
│    ↓                                                                  │
│  poll_tasks():                                                        │
│    └─ pop_effect() → effect.run() → ★ use_effect 回调首次执行          │
│                                                                       │
└───────────────────────────────────────────────────────────────────────┘

┌── 重渲染（信号变化）──────────────────────────────────────────────────┐
│                                                                       │
│  signal.write(new_value)                                              │
│    ↓ 标记所有订阅该信号的 scope 为 dirty                               │
│    ↓                                                                  │
│  process_events() → mark_dirty(ScopeId(27))                           │
│    ↓                                                                  │
│  run_scope(27):                                                       │
│    ├─ before_render hooks                                             │
│    ├─ props.render()    ← 函数体重新执行                               │
│    ├─ after_render hooks                                              │
│    └─ scope 标记为干净                                                 │
│    ↓                                                                  │
│  diff_scope → diff old vs new VNodes → 生成 Mutations                  │
│    ↓                                                                  │
│  finish_render() → 发 EffectQueued                                    │
│    ↓                                                                  │
│  poll_tasks():                                                        │
│    └─ pop_effect() → effect.run() → ★ use_effect 回调重新执行          │
│                                                                       │
└───────────────────────────────────────────────────────────────────────┘

┌── 销毁（组件从 RSX 中移除）──────────────────────────────────────────┐
│                                                                       │
│  父组件 diff: 发现 <Home /> 不再出现                                   │
│    ↓                                                                  │
│  remove_component_node(to, true, 27):                                 │
│    ├─ VNode 从 DOM 移除                                               │
│    └─ drop_scope(27) → ScopeState::drop():                            │
│         └─ runtime.remove_scope(27):                                  │
│              ├─ spawned_tasks 全部取消                                 │
│              ├─ pending_effects.remove(27)   ← ★ effect 不执行        │
│              ├─ hooks.take().drain().rev():                           │
│              │   ├─ use_drop 的 Drop 触发 → ondestroy 回调             │
│              │   ├─ use_signal drop                                   │
│              │   └─ use_effect 的 Effect{rc} drop                     │
│              └─ scope_states[27].take()                               │
│                                                                       │
│  ⚠️ 注意：上面的 remove_component_node 之前的 run_scope 中，            │
│     函数体已经执行过一次了（父组件 diff 时触发子 scope 的 final rerun）。 │
│     但 use_effect 回调在之后的 remove_scope 中被跳过。                  │
│                                                                       │
└───────────────────────────────────────────────────────────────────────┘
```

---

## 7. 关键结论

| # | 结论 | 源码依据 |
|---|------|---------|
| 1 | **组件函数体每次 rerun 都执行**，包括销毁前的清理渲染 | `scope_arena.rs:68` — `props.render()` 无条件调用 |
| 2 | **`use_effect` 回调在 scope 销毁时明确不执行** | `runtime.rs:192-194` — `pending_effects.remove(scope)` 直接丢弃 |
| 3 | **Effect 优先级最低**，在所有 scope 重渲染和 task poll 之后 | `scheduler.rs:77` — "lowest priority" |
| 4 | **`use_drop` / `use_on_unmount` 在 scope 销毁时执行** | `runtime.rs:197` — hooks 逆序 drop 触发 `LifeCycle::drop` |
| 5 | **`use_before_render` / `use_after_render` 在销毁时也执行** | 它们是 run_scope 流程的一部分，每次 render 都触发 |
| 6 | **首次渲染时 scope 是 Unmounted**，effect 进入 `effects_queued`；mount 后才 drain 到 `pending_effects` | `scope_context.rs:86` + `tasks.rs:220-227` |
| 7 | **`spawn` 创建的任务受组件生命周期管理**，卸载时通过 `spawned_tasks` 全部取消 | `scope_context.rs:314` — `spawned_tasks.insert(id)` + `runtime.rs:187` — `remove_task(id)` |
| 8 | **Task 完成后自动从 `spawned_tasks` 移除**，避免卸载时 double drop | `tasks.rs:284-292` — `poll_result.is_ready()` 时 `remove(&id)` + `remove_task(id)` |
| 9 | **`use_resource` 和 `use_future` 内部都用 `spawn`**，共享同一套生命周期管理 | `use_resource.rs:50` + `use_future.rs:54` — 均调用 `dioxus_core::spawn` |
| 10 | **`spawn_forever` 注册到 ROOT scope**，组件卸载不取消 | `global_context.rs:199` — `Runtime::with_scope(ScopeId::ROOT, ...)` |

### 实践指导

- **需要在组件销毁时清理资源** → `use_drop(|| cleanup())`
- **需要在 DOM 更新后访问 DOM** → `use_effect(move || { ... })`
- **需要在每次渲染前重置状态** → `use_before_render(move || { ... })`
- **需要在函数体中访问平台 API（可能 panic）** → ❌ 移到 `use_effect`，函数体在销毁时也会执行
- **需要异步获取数据供 UI 消费** → `use_resource`（自动依赖追踪 + 值缓存）
- **需要异步任务但自己管理结果** → `use_future`（有 early return 保护）
- **需要响应状态变化执行异步副作用** → `use_effect` + `spawn`（effect 追踪依赖，spawn 执行 async）
- **需要超越组件生命周期的后台任务** → `spawn_forever`（注册到 ROOT scope）