# ndk-context 背景知识

## ndk-context 是什么

`ndk-context` 是 Android Rust 生态中的**基础设施层 crate**，提供一个全局指针仓库——在 Android 启动时把 JVM 指针和 Activity 指针存到一个 `static mut` 变量里，让任何 Rust 代码都能随时取用。

## 完整源码结构

整个 crate 只有一个 [lib.rs](https://docs.rs/ndk-context/latest/src/ndk_context/lib.rs.html)，核心约 100 行：

```rust
static mut ANDROID_CONTEXT: Option<AndroidContext> = None;

#[derive(Clone, Copy, Debug)]
pub struct AndroidContext {
    java_vm: *mut c_void,          // JavaVM 指针
    context_jobject: *mut c_void,  // Activity jobject 指针
}

// 读：任何 Rust 代码随时调用
pub fn android_context() -> AndroidContext {
    unsafe { ANDROID_CONTEXT.expect("android context was not initialized") }
}

// 写：启动时由框架调用一次
pub unsafe fn initialize_android_context(java_vm: *mut c_void, context_jobject: *mut c_void) {
    let previous = ANDROID_CONTEXT.replace(AndroidContext { java_vm, context_jobject });
    assert!(previous.is_none());  // 只能写一次
}
```

## 指针来源：ANativeActivity 结构体

Android NDK 定义了 C 结构体 `ANativeActivity`（在 `<android/native_activity.h>` 中）：

```c
typedef struct ANativeActivity {
    struct ANativeActivityCallbacks* callbacks;
    JavaVM* vm;           // ← 进程的 JavaVM 指针（每个 Android 进程只有一个 JVM）
    JNIEnv* env;          // ← 主线程的 JNI 环境
    jobject clazz;        // ← NativeActivity 实例的 JNI 引用（这就是 "context"）
    const char* internalDataPath;
    const char* externalDataPath;
    int32_t sdkVersion;
    // ...
} ANativeActivity;
```

- `vm`：Android 每个进程只有一个 JVM 实例，所有 Java/Kotlin 代码都运行在其中。进程级全局唯一。
- `clazz`：`NativeActivity` 的 Java 对象引用，同时也是一个 `android.content.Context`（因为 `Activity` 继承自 `Context`）。

## 初始化链路（Dioxus 项目实际路径）

Dioxus 项目的依赖链是 `dioxus` → `dioxus-desktop` → `wry` → `tao`，**实际写入 `ndk-context` 的是 `tao`**，而非 `android-activity`。

```
┌─ Kotlin 端 ────────────────────────────────────────────────────┐
│ NativeActivity.onCreate()                                      │
│   → System.loadLibrary("main")                                 │
│   → 调用 Rust 的 Java_xxx_create(activity: JObject)            │
└────────────────────────────────────────────────────────────────┘
                    ↓
┌─ tao::ndk_glue::create() ─────────────────────────────────────┐
│ 1. env.get_java_vm()          → 从 JNI 参数中提取 JavaVM       │
│ 2. env.new_global_ref(jobject) → 从 JNI 参数中提取 Activity    │
│ 3. ndk_context::initialize_android_context(                    │
│      vm.get_java_vm_pointer(),   ← JVM 指针                   │
│      activity.as_obj().as_raw()  ← Activity jobject 指针       │
│    )                                                           │
│ 4. 启动 main 线程                                              │
└────────────────────────────────────────────────────────────────┘
                    ↓
┌─ 业务 Rust 代码 ──────────────────────────────────────────────┐
│ ndk_context::android_context()  → 随时可用                     │
│   .vm()       → JavaVM 指针                                    │
│   .context()  → Activity jobject 指针                          │
└────────────────────────────────────────────────────────────────┘
```

关键源码位于 `tao::platform_impl::android::ndk_glue::create()`：

```rust
pub unsafe fn create(
  mut env: JNIEnv,     // JNI 框架自动提供，内含 JVM 引用
  _jclass: JClass,
  jobject: JObject,    // Kotlin 端传入的 Activity 对象
  ...
) {
  let vm = env.get_java_vm().unwrap();
  let activity = env.new_global_ref(jobject).unwrap();
  ndk_context::initialize_android_context(
    vm.get_java_vm_pointer() as *mut _,       // JVM 指针：来自 JNI 环境
    activity.as_obj().as_raw() as *mut _,     // Activity 指针：来自 Kotlin 参数
  );
  // ... 启动 main 线程
}
```

指针的最终来源是 Android 系统（创建 JVM 和 Activity 实例），`ndk-context` 的全局变量只是中转存储。

## 为什么设计成独立的 crate

- **库的解耦**：任何 Rust crate（不管用 `android-activity`、`ndk-glue`、`tao`、还是自定义方案）都能初始化它
- **共享入口**：多个库（如 `cpal`、`webbrowser`、`hickory-resolver` 等）都需要 JVM 指针来调用 JNI，它们都依赖 `ndk-context` 而不是具体的框架
- 类似于 C 世界里的 `android_native_app_glue`

## 生态标准地位

`ndk-context` 的 API 已**冻结为 1.0**，文档明确声明提供 **stable api**。它定义了一个极简协议：一个全局变量 + 两个函数（`initialize_android_context` 写、`android_context` 读），任何框架只要调用写入，任何库只要调用读取，就自动对接。

### 写入者（初始化者）

| 库                     | 说明                                                        |
| ---------------------- | ----------------------------------------------------------- |
| **ndk-glue**           | rust-mobile 组织的原始方案，`ANativeActivity_onCreate` 入口 |
| **android-activity**   | rust-mobile 组织的新方案，替代 ndk-glue                     |
| **tao**                | Tauri 的窗口库，Dioxus 项目实际走的路径                     |
| **robius-android-env** | Robius 框架的适配层                                         |

### 读取者（消费者）

直接依赖 `ndk-context` 的库有 55 个，典型代表：

| 库                   | 用途            | 需要的指针                       |
| -------------------- | --------------- | -------------------------------- |
| **webbrowser**       | 打开系统浏览器  | `context()` 调 `startActivity`   |
| **cpal**             | 音频播放/录制   | `vm()` 调 AudioTrack/AudioRecord |
| **hickory-resolver** | DNS 解析        | `vm()` 访问网络 API              |
| **app_dirs2**        | 获取应用目录    | `context()` 调 `getFilesDir`     |
| **manganis**         | Dioxus 资源管理 | `context()` 访问 Activity        |
| **oboe**             | 低延迟音频      | `vm()` 调 JNI                    |
| **openxr**           | VR/AR           | `vm()` 调 JNI                    |
| **tts**              | 文字转语音      | `vm()` 调 TTS API                |

### 架构

```
┌─ 写入者（框架层）──────────────────────────────┐
│  ndk-glue / android-activity / tao / robius    │
│  调用 initialize_android_context(vm, context)   │
└────────────────────────────────────────────────┘
                      ↓ 写入
┌─ 标准（协议层）────────────────────────────────┐
│  ndk-context  ← API 冻结 1.0，55 个库依赖       │
│  static mut ANDROID_CONTEXT: Option<...>        │
└────────────────────────────────────────────────┘
                      ↓ 读取
┌─ 读取者（业务库）──────────────────────────────┐
│  webbrowser / cpal / hickory / app_dirs2 / ...  │
│  调用 android_context() 获取指针                 │
└────────────────────────────────────────────────┘
```

核心价值：**解耦**。业务库不需要知道自己跑在 `ndk-glue`、`android-activity` 还是 `tao` 上，只要有人初始化了 `ndk-context`，它们就能工作。

## 各层职责

| 层级                                    | 角色                          | 谁提供指针                                                  |
| --------------------------------------- | ----------------------------- | ----------------------------------------------------------- |
| Android 系统                            | 创建 JVM、创建 NativeActivity | `ANativeActivity->vm` 和 `->clazz`                          |
| `ndk-glue` / `android-activity` / `tao` | 框架层，接收系统指针          | 调用 `initialize_android_context(vm, clazz)`                |
| `ndk-context`                           | 全局指针仓库（协议标准）      | `static mut` 存储两个指针，提供 `android_context()` 读取    |
| `jni` crate                             | JNI 绑定                      | 用 `JavaVM::from_raw(ctx.vm().cast())` 把裸指针转为安全接口 |
| 业务代码                                | 应用逻辑                      | `ndk_context::android_context()` 随时取用                   |

本质上就是三步接力：Android 系统创建指针 → 框架层存入全局变量 → 业务代码读取使用。

## ndk-context 与 JNI 函数定义的关系

`ndk-context` + `jni` crate 可以直接从 Rust 调用 Kotlin/Java 方法，但**不能替代 Kotlin→Rust 方向的 JNI 函数定义**。

### Rust→Kotlin：不需要 `Java_xxx` 函数定义

`ndk-context` 提供 JVM 指针，`jni` crate 提供操作接口，两者配合即可直接调用任何 Kotlin/Java 方法，**无需定义任何 `Java_xxx` 函数**。

**推荐方式：`ctx.context()` + instance method**。通过 `ctx.context()` 拿到 Activity 实例的 jobject，直接调实例方法，**不需要 `FindClass`**。

为什么不用 `find_class`：

1. **ClassLoader 限制**：NativeActivity 使用系统 ClassLoader，无法加载应用 APK 中的自定义 Kotlin 类，`find_class("dev/dioxus/main/MainActivity")` 运行时会抛出 `ClassNotFoundException`
2. **时序风险**：`.so` 在 `JNI_OnLoad` 或 `NativeActivity.onCreate` 之前就可能被加载，此时 VM/ClassLoader 尚未就绪，调用 `find_class` 会崩溃

为什么 `ctx.context()` 更可靠：

- `ndk_context` 的全局 activity 引用由框架层在 `NativeActivity.onCreate` 时写入，此时 Java VM 和 Activity 实例均已创建完毕
- 调用 `android_context()` 时若未初始化会直接 panic（fail-fast），不存在半初始化状态的隐式崩溃
- 拿到的 jobject 就是 Kotlin Activity 的 `this`，直接调实例方法，跳过了 ClassLoader 查找这整个不稳定环节

```rust
// 实例方法调用（jni 0.22 API，无需 FindClass）
use jni::{jni_sig, jni_str, objects::JObject, JValueOwned};

let ctx = ndk_context::android_context();
let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) };
let Ok(height) = vm.attach_current_thread(|env| -> Result<f32, jni::errors::Error> {
    let activity = unsafe { JObject::from_raw(env, ctx.context().cast()) };
    let result = env.call_method(
        &activity,
        jni_str!("getStatusBarHeight"),
        jni_sig!("()F"),
        &[],
    )?;
    Ok(match result {
        JValueOwned::Float(h) => h,
        _ => 0.0,
    })
}) else {
    return;
};
```

对于 Android 系统类（如 `android/os/Environment`），仍然可以用 `find_class` 调用静态方法——系统类由系统 ClassLoader 加载，不受上述限制。

### Kotlin→Rust：必须定义 `Java_xxx` 函数

Kotlin 通过 `System.loadLibrary("main")` 加载 `.so`，然后通过符号名 `Java_dev_dioxus_main_RustBridge_sendEventToRust` 找到 Rust 函数。这是 JNI 规范的硬性机制，`ndk-context` 无法替代——它只提供 JVM 指针，不能反过来让 Kotlin 发现 Rust 函数。

```rust
// Rust 端：必须定义，Kotlin 才能调进来
#[no_mangle]
pub extern "system" fn Java_dev_dioxus_main_RustBridge_someMethod(
    mut env: jni::JNIEnv,
    _class: jni::objects::JClass,
) -> jni::sys::jboolean { ... }
```

```kotlin
// Kotlin 端：必须声明 external
external fun someMethod(): Boolean
```

### 总结

| 方向            | 是否需要 `Java_xxx` 定义 | 原因                                                 |
| --------------- | ------------------------ | ---------------------------------------------------- |
| **Rust→Kotlin** | ❌ 不需要                | `ndk-context` 提供 JVM，`jni` crate 直接找类调方法   |
| **Kotlin→Rust** | ✅ 必须                  | Kotlin 通过符号名找 Rust 函数，是 JNI 规范的硬性机制 |

## `release_android_context()` — 清除 API

文档只讲了读写，**还有第三个 API：销毁**。

```rust
/// 移除 AndroidContext。Activity 销毁时由框架调用。
///
/// # Safety
/// 必须在 initialize_android_context() 之后、Activity 销毁时调用。
pub unsafe fn release_android_context() {
    let previous = ANDROID_CONTEXT.take();  // Option::take() → 置 None
    assert!(previous.is_some());            // 必须之前已初始化
}
```

**调用之后 `ANDROID_CONTEXT` 变回 `None`**，此后任何 `android_context()` 调用都会 panic（`"android context was not initialized"`）。

## 销毁链路：谁在什么时候调用 `release_android_context()`

唯一调用者是 **tao**（`ndk_glue.rs:320-324`）：

```rust
// tao-0.34.8/src/platform_impl/android/ndk_glue.rs:320-324
pub unsafe fn destroy(_: JNIEnv, _: JClass, _: JObject) {
    wake(Event::Destroy);
    ndk_context::release_android_context();  // ← 全局置 None
    WINDOW_MANAGER.take();
}
```

这个 `destroy()` 是 JNI callback，由 Kotlin 端的 Activity 生命周期触发：

```
Android 系统
  → Activity.onDestroy()
    → WryActivity.kt:94  override fun onDestroy()
      → super.onDestroy()              // ① Android 框架清理
      → destroy()                      // ② JNI → tao::destroy() → release_android_context()
      → onActivityDestroy()            // ③ JNI → dioxus-desktop 资源回收
```

关键顺序：**`super.onDestroy()` 在前，`destroy()` 在后**。所以 `release_android_context()` 执行时，Android 框架的 `Activity.onDestroy()` 已经完成。

对应的，每次 Activity 重建时会重新走 `onCreate()` → `create()` → `initialize_android_context()`：

```
WryActivity.onCreate()
  → create(this)                       // JNI → tao ndk_glue::create()
      → initialize_android_context()   // ← ANDROID_CONTEXT 重新就绪
      → wry::android_setup()           // WebChromeClient + MainPipe 注册
      → thread::spawn { main() }       // 启动 Rust 主逻辑
```

**就绪**：`initialize_android_context` 在 setup/main 之前，Dioxus 组件首次渲染时必定可用。
**销毁**：`release_android_context` 在 `destroy()` 中，此后 `android_context()` 必定 panic。

## panic 窗口与 Dioxus 避坑指南

### panic 窗口

```
release_android_context()  ← ANDROID_CONTEXT = None
    ↓
Dioxus runtime 仍在做清理渲染
    ↓
组件函数体再次执行（同步，持有 android_runtime_lock）
    ↓
AndroidBackend::new() → android_context()  ← PANIC
    ↓
panic 毒化 android_runtime_lock → 之后所有渲染静默失败（UI 卡死）
```

### 为什么组件函数体在销毁时还会执行

Dioxus 的 `run_scope`（`scope_arena.rs:47`）在每次 VDOM 协调时调用 `props.render()`——包括父组件 diff 触发子组件移除时的最后一次清理渲染。而 `use_effect` 回调在 scope 销毁时**明确不执行**（`runtime.rs:192-194` 直接移除 `pending_effects`，不运行）。

### 避坑规则

| 做法 | 安全性 |
|------|:---:|
| 在组件**函数体**中调用 `android_context()` | ❌ 销毁时 panic |
| 在 `use_effect` 回调中调用 `android_context()` | ✅ 销毁时不执行 |
| 在事件处理器（onclick 等）中调用 `android_context()` | ✅ 但 Activity 销毁后事件不可能触发 |
| 在 `use_resource` 异步块中调用 `android_context()` | ⚠️ 可能执行中遇到 destroy，需判断 |

**结论**：所有 `android_context()` 调用应放在 `use_effect` 回调或事件处理器中，**禁止**放在组件函数体中同步调用。
