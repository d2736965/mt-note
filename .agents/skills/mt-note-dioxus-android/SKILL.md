---
name: "mt-note-dioxus-android"
description: "Dioxus Android 开发技术参考（Dioxus.toml 配置、权限注入、JNI 桥接、软键盘遮挡修复）。当用户涉及 Dioxus Android 构建/配置/权限/JNI/原生桥接/键盘遮挡输入框时调用。（mt-note 项目自带版本）"
---

# Dioxus Android 开发技术参考

基于 Dioxus 0.7.10 CLI 源码分析，涵盖 Dioxus.toml 配置体系、权限注入机制、构建流水线和 JNI 原生桥接方法。

---

## 一、Dioxus.toml Android 配置体系

### 1.1 三个权限配置节的区别（核心易混淆点）

| 配置节                   | 机制                     | 支持的权限                                 | 模板渲染位置            |
| ------------------------ | ------------------------ | ------------------------------------------ | ----------------------- |
| `[permissions]`          | 跨平台预定义映射         | 仅 `location`/`camera`/`photos` 等有限集合 | `{{#each permissions}}` |
| `[android.permissions]`  | Android 专有，任意权限名 | 任意 `android.permission.XXX`              | `{{#each permissions}}` |
| `[android.raw.manifest]` | 原始 XML 注入            | 任意 XML                                   | `{{{ raw_manifest }}}`  |

**关键**：`[permissions]` 不支持 `MANAGE_EXTERNAL_STORAGE` 等 Android 特有权限，因为它只识别跨平台预定义字段。需要 Android 特有权限时，用 `[android.permissions]`（结构化）或 `[android.raw.manifest]`（原始 XML）。

模板中三者的渲染方式（CLI 内置 AndroidManifest.xml.hbs）：

```xml
<!-- [permissions] + [android.permissions] 都走这里 -->
{{#each permissions}}
<uses-permission android:name="{{ this }}" />
{{/each}}

<!-- [android.raw.manifest] 直接注入原始 XML（三花括号不转义） -->
{{{ raw_manifest }}}
```

### 1.2 `[permissions]` 跨平台预定义字段

| 字段       | TOML 键名             | 映射到 Android                                     |
| ---------- | --------------------- | -------------------------------------------------- |
| 位置       | `location`            | `ACCESS_FINE_LOCATION` / `ACCESS_COARSE_LOCATION`  |
| 相机       | `camera`              | `CAMERA`                                           |
| 麦克风     | `microphone`          | `RECORD_AUDIO`                                     |
| 照片       | `photos`              | `READ_MEDIA_IMAGES` / `WRITE_EXTERNAL_STORAGE`     |
| 蓝牙       | `bluetooth`           | `BLUETOOTH_CONNECT` / `BLUETOOTH_SCAN`             |
| 通知       | `notifications`       | `POST_NOTIFICATIONS`                               |
| 后台位置   | `background-location` | `ACCESS_BACKGROUND_LOCATION`                       |
| 通讯录     | `contacts`            | `READ_CONTACTS` / `WRITE_CONTACTS`                 |
| 日历       | `calendar`            | `READ_CALENDAR` / `WRITE_CALENDAR`                 |
| 生物识别   | `biometrics`          | `USE_BIOMETRIC`                                    |
| NFC        | `nfc`                 | `NFC`                                              |
| 运动       | `motion`              | `ACTIVITY_RECOGNITION`                             |
| 健康       | `health`              | 多个 `BODY_SENSORS` / `HIGH_SAMPLING_RATE_SENSORS` |
| 语音       | `speech`              | `RECORD_AUDIO`                                     |
| 媒体库     | `media-library`       | `READ_MEDIA_VISUAL_USER_SELECTED`                  |
| 附近 Wi-Fi | `nearby-wifi`         | `NEARBY_WIFI_DEVICES`                              |

`photos` 的 `access` 字段：`"read"` → `READ_MEDIA_IMAGES`，`"write"` → `WRITE_EXTERNAL_STORAGE`，`"read-write"` → 两者都加。

配置示例：

```toml
[permissions]
location = { precision = "fine", description = "定位" }
camera = { description = "拍照" }
photos = { access = "read-write", description = "相册访问" }
```

### 1.3 `[android.permissions]` — Android 专有权限（推荐）

```toml
[android.permissions]
"android.permission.MANAGE_EXTERNAL_STORAGE" = { description = "文件管理" }
"android.permission.READ_EXTERNAL_STORAGE" = { description = "读取文件" }
"android.permission.WRITE_EXTERNAL_STORAGE" = { description = "写入文件" }
"android.permission.FOREGROUND_SERVICE" = { description = "前台服务" }
```

### 1.4 `[android.raw]` — 原始 XML 注入

```toml
[android.raw]
manifest = """
<uses-permission android:name="android.permission.NFC" />
"""
```

- `manifest`（✅ 生效）：注入到 `<manifest>` 元素内、`<application>` **之前**。注意：只能放顶层元素（`<uses-permission>`/`<uses-feature>` 等），放 `<activity>`/`<service>` 等 application 子元素**非法**。
- ~~`application_attrs`~~（❌ 不生效）：config 有字段但 hbs 模板无对应注入点。
- ~~`application`~~（❌ 不生效）：config 有字段但 hbs 模板无对应注入点。

**⚠️ 如需在 `<application>` 内注入内容**（intent-filter、activity-alias、service 等），hbs 模板无注入点。须用 `[application] android_manifest` 整体替换 manifest（见 §1.6）。

### 1.5 完整 `[android]` 配置参考

```toml
[android]
identifier = "com.example.myapp"     # 覆盖 bundle.identifier
min_sdk = 24                         # 最小 SDK 版本（默认 24）
target_sdk = 34                      # 目标 SDK 版本（默认 34）
compile_sdk = 34                     # 编译 SDK 版本（默认 34）
lib_name = "main"                    # 原生库名（不含 lib 前缀和 .so 后缀）

features = [
    "android.hardware.location.gps",  # 硬件特性声明
]

gradle_dependencies = [
    "androidx.core:core-ktx:1.12.0",  # 额外 Gradle 依赖
]

gradle_plugins = [
    "kotlin-android",  # 额外 Gradle 插件
]

[android.permissions]
"android.permission.CAMERA" = { description = "相机" }

[android.application]
uses_cleartext_traffic = true  # 允许 HTTP 明文流量
theme = "@style/MyTheme"       # 自定义主题
supports_rtl = true            # RTL 布局支持
large_heap = false             # 大堆内存

[android.raw]
manifest = """..."""           # ✅ 注入 manifest 顶层（application 之前）
# application_attrs = """..."""  # ❌ 不生效（hbs 无注入点）
# application = """..."""        # ❌ 不生效（hbs 无注入点）

[android.signing]
jks_file = "release.keystore"
jks_password = "changeit"
key_alias = "my-key"
key_password = "changeit"
```

### 1.6 其他相关配置节

```toml
[application]
android_manifest = "android/AndroidManifest.xml"    # 自定义 manifest（整体替换，跳过 hbs，推荐）
android_main_activity = "android/MainActivity.kt"   # 自定义 Kotlin Activity 路径

[deep_links]
schemes = ["myapp", "com.example.myapp"]  # URL scheme
hosts = ["example.com"]                     # Universal/App Link 主机

[background]
location = true   # 后台位置
audio = true      # 后台音频
fetch = true      # 后台获取
```

---

## 二、Android 构建流水线

### 2.1 目录结构

Dioxus CLI (`dx serve --platform android`) 生成标准 Gradle 项目：

```
target/dx/<app>/debug/android/
├── app/
│   ├── src/main/
│   │   ├── AndroidManifest.xml       ← 由 .hbs 模板生成
│   │   ├── kotlin/                   ← 用户自定义 Kotlin
│   │   ├── jniLibs/                  ← 编译产物 .so
│   │   ├── assets/                   ← Dioxus 资源
│   │   └── res/                      ← Android 资源
│   ├── build.gradle.kts              ← 由 .hbs 模板生成
│   └── proguard-rules.pro
├── buildSrc/                         ← BuildTask.kt（编译 Rust .so）
├── build.gradle.kts
├── gradle.properties
├── gradlew / gradlew.bat
└── settings.gradle
```

### 2.2 AndroidManifest.xml 生成逻辑

1. 如果 `[application] android_manifest = "路径"` 指定了文件，**直接读取该文件作为最终 AndroidManifest.xml**（整体替换，完全跳过 hbs 模板）。用户须自行写全 permissions、NativeActivity meta-data、MAIN/LAUNCHER 等。详见 mt-note-dioxus-toml skill 的互斥关系说明。
2. 否则使用 CLI 内置的 `AndroidManifest.xml.hbs` 模板，Handlebars 渲染注入：`permissions`、`features`、`raw_manifest`、`url_schemes`、`app_link_hosts`、`uses_cleartext_traffic` 等。

**⚠️ `[android] manifest` 是死字段**：config 有此字段但 CLI 代码从不读取，配了不生效。
**⚠️ `[[android.intent_filters]]` 不生效**：config 能解析但 hbs 模板无对应渲染点，丢弃。
**推荐用 `[application] android_manifest` 整体替换**，manifest 全部能力可用，后期扩展不受限。

### 2.3 ManifestMapper 处理流程

```
[permissions] 跨平台配置 ──→ ManifestMapper::from_config() ──→ android_permissions: Vec<String>
[android.permissions] ──────→ 合并到 android_permissions ────────┘
                                                       ↓
                                              {{#each permissions}} 渲染
[android.raw.manifest] ──→ raw_manifest: String ──→ {{{ raw_manifest }}} 渲染
（注：raw.application / raw.application_attrs 有 config 字段但 hbs 无注入点，不生效）
```

---

## 三、JNI 原生桥接方法

Dioxus Android 使用 `NativeActivity` 入口，Rust 代码编译为 `.so` 由系统加载。当需要 Kotlin/Rust 双向通信时，需要自行实现 JNI 桥接。

**⚠️ ProGuard/R8 规则**：`dx bundle -r` 的 release build 启用 `isMinifyEnabled = true`，R8 会混淆 Kotlin 类中的自定义方法名。**凡是 Rust 通过 `JNIString::from("methodName")` 按名字符串调用的 Kotlin 方法，必须在 Kotlin 端加 `@Keep` 注解**，否则 release APK 运行时 JNI 查找不到 → `NoSuchMethodError`。`dx serve`（debug）不混淆，问题不暴露。Android 系统方法（`getFilesDir`、`getAbsolutePath` 等）不受影响——系统库不参与混淆。

### 3.1 自定义 Kotlin Activity

在 Dioxus.toml 中指定：

```toml
[application]
android_main_activity = "android/MainActivity.kt"
```

Kotlin Activity 必须继承 `WryActivity`（Dioxus 内置的 NativeActivity 子类）。

**⚠️ 必须包含 `typealias BuildConfig`**：CLI 模板生成的 MainActivity 包含此行，自定义时也必须保留，否则编译失败。

**⚠️ Kotlin 文件结构顺序**：`package` → `import` → `typealias` → `class`。`typealias` 放在 import 之前会导致编译错误（"imports are only allowed in the beginning of file"）。

```kotlin
package dev.dioxus.main

import android.os.Bundle
import android.view.View

typealias BuildConfig = com.example.myapp.BuildConfig

class MainActivity : WryActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
    }
}
```

其中 `com.example.myapp` 即 `[bundle].identifier` 的值，直接使用，不做任何转换。

### 3.2 Rust→Kotlin 调用

**推荐方式：instance method 调用**，通过 `ndk_context::android_context().context()` 拿到 Activity 实例，直接调实例方法。**不需要 `FindClass`**。

**为什么避免 `find_class`**：

1. **ClassLoader 问题**：NativeActivity 使用系统 ClassLoader，无法加载应用 APK 中的自定义 Kotlin 类，`find_class("dev/dioxus/main/MainActivity")` 运行时返回 `null` 或抛出 `ClassNotFoundException`
2. **时序问题**：`find_class` 需要 Java VM 已经完全初始化且 ClassLoader 就绪。而 `.so` 在 `JNI_OnLoad` 或 `NativeActivity.onCreate` 之前就可能被加载，此时 VM/ClassLoader 尚未就绪，调用 `find_class` 会崩溃

**为什么 `ctx.context()` 更省心**：

- `ndk_context` 的全局 activity 引用由 `android-activity` 框架在 `NativeActivity.onCreate` 时写入，此时 Java VM 和 Activity 实例均已创建完毕
- Rust 调用 `android_context()` 时若未初始化会直接 panic（fail-fast），不存在半初始化状态的隐式崩溃
- 拿到的 jobject 就是 Kotlin Activity 的 `this`，直接调实例方法，跳过了 ClassLoader 查找这一整个不稳定环节
- 不依赖 `@JvmStatic`，Kotlin 端定义普通实例方法即可

Kotlin 端定义**实例方法**（非 `@JvmStatic`）：

```kotlin
package dev.dioxus.main

import com.google.androidgamesdk.GameActivity

class MainActivity : GameActivity() {
    private var statusBarHeight: Float = 0f

    // 实例方法，通过 ctx.context() 拿到的 Activity 调用
    fun getStatusBarHeight(): Float = statusBarHeight

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        val resourceId = resources.getIdentifier("status_bar_height", "dimen", "android")
        if (resourceId > 0) {
            statusBarHeight = resources.getDimensionPixelSize(resourceId).toFloat()
        }
    }
}
```

Rust 端调用（jni 0.22 API，`attach_current_thread` 改为 callback 模式）：

```rust
use jni::{jni_sig, jni_str, objects::JObject, JValueOwned};

let ctx = ndk_context::android_context();
let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) };
let Ok(height) = vm.attach_current_thread(|env| -> Result<f32, jni::errors::Error> {
    // 通过 ctx.context() 获取 Activity 实例（无需 FindClass）
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

**命名规则**：`ctx.context()` 返回的 jobject 就是 NativeActivity 实例（即 Kotlin 端的 `this`），可以直接调用 Kotlin 类中定义的实例方法，方法签名使用标准 JNI 描述符。

**jni 0.22 主要 API 变化**（相比 0.21）：

| 项目 | 0.21 | 0.22 |
|------|------|------|
| 返回值枚举 | `jni::objects::JValueGen` | `jni::JValueOwned` |
| `JavaVM::from_raw` | 返回 `Result<JavaVM>` | 返回 `JavaVM` |
| `JObject::from_raw` | 单参数 `(raw)` | 双参数 `(env, raw)` |
| `attach_current_thread` | 返回 `Env` | callback: `FnOnce(&mut Env) -> Result<T, E>` |
| `call_method` 参数 | 接受 `&str` | `AsRef<JNIStr>` + `AsRef<MethodSignature>`（用 `jni_str!` / `jni_sig!` 宏） |

### 3.3 Kotlin→Rust 调用

在 Rust 端声明 `#[no_mangle] extern "system"` 函数，Kotlin 端通过 `external` 声明对应方法：

```rust
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_dev_dioxus_main_RustBridge_someMethod(
    mut env: jni::JNIEnv,
    _class: jni::objects::JClass,
    // 参数...
) -> jni::sys::jboolean {
    // 处理逻辑
    1
}
```

```kotlin
class RustBridge {
    companion object {
        init { System.loadLibrary("main") }

        @JvmStatic
        external fun someMethod(/* 参数 */): Boolean
    }
}
```

**命名规则**：JNI 函数名格式为 `Java_<包路径>_<类名>_<方法名>`，其中包路径用下划线分隔（如 `dev_dioxus_main`）。

### 3.4 tokio runtime 桥接

JNI 函数运行在 Android 主线程（无 tokio runtime）。如果需要在 JNI 函数中使用 tokio 异步操作，需保存 tokio Handle：

```rust
use std::sync::OnceLock;
use tokio::runtime::Handle;

static TOKIO_HANDLE: OnceLock<Handle> = OnceLock::new();

// 在 tokio runtime 内调用（如 Dioxus 组件初始化时）
fn save_handle() {
    let handle = Handle::current();
    TOKIO_HANDLE.get_or_init(|| handle);
}

// 在 JNI 函数中使用
fn jni_handler() {
    if let Some(handle) = TOKIO_HANDLE.get() {
        handle.block_on(async {
            // 异步操作
        });
    }
}
```

`OnceLock` 只写一次，之后永远只读，无锁竞争/死锁风险，适合 JNI 桥接层的引导问题。

### 3.5 Activity 实例获取

Kotlin 端可通过 `@Volatile` 静态引用 + 生命周期回调暴露 Activity 实例：

```kotlin
companion object {
    @Volatile
    private var currentInstance: MainActivity? = null

    @JvmStatic
    fun handleEventFromRust(/* 参数 */) {
        currentInstance?.someMethod()
    }
}

override fun onResume() {
    super.onResume()
    currentInstance = this
}

override fun onPause() {
    super.onPause()
    if (currentInstance === this) currentInstance = null
}
```

---

## 四、常见问题

### Q: 为什么 `[permissions]` 配了权限但 AndroidManifest.xml 里是空的？

`[permissions]` 只识别预定义的跨平台字段（`location`/`camera`/`photos` 等）。Android 特有权限如 `MANAGE_EXTERNAL_STORAGE` 不在其中，会被忽略。改用 `[android.permissions]` 或 `[android.raw.manifest]`。

### Q: 如何添加 Android 特有权限？

```toml
# 推荐：结构化方式
[android.permissions]
"android.permission.MANAGE_EXTERNAL_STORAGE" = { description = "文件管理" }

# 或：原始 XML 注入
[android.raw]
manifest = """
<uses-permission android:name="android.permission.MANAGE_EXTERNAL_STORAGE" />
"""
```

### Q: 如何添加自定义 intent-filter（如接收分享）？

`[[android.intent_filters]]` 配置不生效（hbs 模板无渲染点）。须用 `[application] android_manifest`
整体替换 manifest，在自定义 manifest 文件中手写 intent-filter。详见 mt-note-dioxus-toml skill 的
AndroidManifest.xml 生成逻辑和互斥关系说明。

### Q: JNI 函数名怎么确定？

格式：`Java_<包路径>_<类名>_<方法名>`，包路径中的点替换为下划线。如 `dev.dioxus.main.RustBridge.sendEvent` → `Java_dev_dioxus_main_RustBridge_sendEvent`。

### Q: 修改了 Android 相关代码后如何验证？

```bash
dx check --android --target aarch64-linux-android
```

### Q: Android 上内容被状态栏遮挡怎么办？

Dioxus Android 使用 WebView 渲染，默认系统状态栏会覆盖内容（edge-to-edge）。需要两步修复：

**1. 启用 edge-to-edge 模式**（让 WebView 报告 `safe-area-inset-*` 值）：

```kotlin
class MainActivity : WryActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
    }

    private fun enableEdgeToEdge() {
        window.setDecorFitsSystemWindows(false)
        window.decorView.systemUiVisibility =
            View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR
    }
}
```

- `setDecorFitsSystemWindows(false)`：启用 edge-to-edge，WebView 才会报告 `env(safe-area-inset-*)`
- `SYSTEM_UI_FLAG_LIGHT_STATUS_BAR`：状态栏图标/文字切换为深色（浅色背景必须设置，否则白色图标在白色背景上看不见）

**2. CSS 适配安全区域**：

```scss
.navbar {
  padding-top: calc(8px + env(safe-area-inset-top));
  position: sticky;
  top: 0;
  z-index: 100;
}
```

- `env(safe-area-inset-top)` 返回状态栏高度（未启用 edge-to-edge 时为 0）
- `position: sticky` 让导航栏固定在顶部

### Q: 状态栏图标/文字看不见（白色背景白色图标）？

在自定义 MainActivity 中设置 `SYSTEM_UI_FLAG_LIGHT_STATUS_BAR`，将状态栏图标切换为深色：

```kotlin
window.decorView.systemUiVisibility = View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR
```

此 flag 要求 `min_sdk >= 23`（API 23+），Dioxus 默认 `min_sdk = 24` 已满足。

### Q: 软键盘弹出时遮挡输入框怎么办？

Edge-to-Edge 模式下 `adjustResize` 不生效（Android 11+ 已知行为），需要三步修复：

**1. 自定义 AndroidManifest.xml**，Activity 设置 `windowSoftInputMode="adjustResize"`：

```xml
<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android">
    <application android:usesCleartextTraffic="true">
        <activity
            android:name=".MainActivity"
            android:windowSoftInputMode="adjustResize"
            android:exported="true">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
    </application>
</manifest>
```

**2. MainActivity 中监听 IME WindowInsets**，键盘弹出时给 content view 添加底部 padding：

```kotlin
import android.widget.FrameLayout
import androidx.core.view.WindowCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : WryActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        WebView.setWebContentsDebuggingEnabled(BuildConfig.DEBUG)
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        currentInstance = this
        setupKeyboardResize()
    }

    private fun enableEdgeToEdge() {
        WindowCompat.setDecorFitsSystemWindows(window, false)
        window.decorView.systemUiVisibility =
            View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR
    }

    private fun setupKeyboardResize() {
        val contentView = window.decorView.findViewById<FrameLayout>(android.R.id.content)
        ViewCompat.setOnApplyWindowInsetsListener(contentView) { view, insets ->
            val imeVisible = insets.isVisible(WindowInsetsCompat.Type.ime())
            val imeHeight = insets.getInsets(WindowInsetsCompat.Type.ime()).bottom
            if (imeVisible) {
                view.setPadding(0, 0, 0, imeHeight)
            } else {
                view.setPadding(0, 0, 0, 0)
            }
            insets
        }
    }
}
```

**3. Dioxus.toml 配置**：

```toml
[android]
manifest = "android/AndroidManifest.xml"
gradle_dependencies = ["androidx.core:core-ktx:1.12.0"]

[application]
android_main_activity = "android/MainActivity.kt"
```

**原理**：`WindowInsetsCompat` 监听键盘（IME）高度变化，键盘弹出时给 `content` FrameLayout 设置 `paddingBottom = imeHeight`，WebView 容器缩小，输入框自然上移。

**CSS 建议**：避免使用 `justify-content: center` 垂直居中含输入框的页面，改用 `justify-content: flex-end` 或 `margin-top: auto` 让内容贴底，键盘弹出时不会被推出可视区域。

### Q: 切换主题后状态栏图标颜色没变化？

最常见的原因是 **ProGuard/R8 混淆**了通过 JNI 调用的 Kotlin 方法名。

`dx serve`（debug）走 `assembleDebug`，`isMinifyEnabled = false`，方法名保留，JNI 正常。
`dx bundle -r`（release）走 `assembleRelease`，`isMinifyEnabled = true`，R8 将 `setStatusBarTheme` 混淆为短名，Rust JNI 按原名字符串查找不到 → `NoSuchMethodError`。

**修复**：MainActivity.kt 中给 JNI 可见方法加 `@Keep`：

```kotlin
import androidx.annotation.Keep

class MainActivity : WryActivity() {
    @Keep
    fun setStatusBarTheme(isDark: Boolean) { ... }
}
```

另一种较少见的情况是 effect 耦合：

`jni::set_status_bar_theme` 调用和主题持久化耦合在同一个 `use_effect` 中，且被包裹在 `if let Ok(backend) = AndroidBackend::new()` 分支内：

```rust
// ❌ 错误：状态栏更新耦合在 storage backend 的 Ok 分支
use_effect(move || {
    if let Ok(backend) = AndroidBackend::new() {
        // ... 主题持久化逻辑 ...
        jni::set_status_bar_theme(state.read().theme().is_dark());
    }
});
```

**根因**：若 `AndroidBackend::new()` 在任何一次 effect 重跑时失败，Dioxus 该轮未读到 `state` 信号，丢失对 `state` 的订阅，后续主题切换不再触发此 effect，状态栏颜色卡住。

**修复**：拆成两个独立 `use_effect`——状态栏同步只依赖 `state`，不和 storage 耦合：

```rust
// ✅ 正确：状态栏同步是独立 effect，只读 state
use_effect(move || {
    jni::set_status_bar_theme(state.read().theme().is_dark());
});

// 主题持久化是另一个 effect
let mut theme_loaded = use_signal(|| false);
use_effect(move || {
    if let Ok(backend) = AndroidBackend::new() {
        // ... 主题持久化逻辑 ...
    }
});
```

---

### 运行时崩溃/卡死速查表

当 logcat/panic_hook 出现以下症状时，按表查阅对应文档：

| 日志关键词 / 症状 | 根因 | 查阅文件与章节 |
|---|---|---|
| `android context was not initialized` | 在 `release_android_context()` 之后调用了 `android_context()` | **[ndk-context.md](ndk-context.md)** → panic 窗口与 Dioxus 避坑指南 |
| `FORTIFY: destroyed mutex` | 进程退出时 tokio worker 仍持锁 | **[ndk-context.md](ndk-context.md)** → 销毁链路（了解 destroy 时序） |
| 菜单/按钮无响应、UI 卡死 | render 期间 panic 毒化 `android_runtime_lock`，后续渲染静默失败 | **[ndk-context.md](ndk-context.md)** → panic 窗口（锁中毒理论） |
| `panic_cannot_unwind` in `handleRequest` + SIGABRT | `onPause`/`onResume` 后 wry 的 `shouldInterceptRequest` JNI 处理 panic（缺陷 1 的 panic 变体） | **[mt-note-dioxus-bugs/SKILL.md](../mt-note-dioxus-bugs/SKILL.md)** → 缺陷 1 变体 |
| `android context was not initialized` + 卡死同时出现 | 函数体中 `android_context()` 在销毁时 panic → 锁中毒 | 先看 ndk-context.md 避坑规则，再看 **[hooks-lifecycle.md](../mt-note-dioxus-reference/hooks-lifecycle.md)** → 关键结论 #1 #2 |
| 不确定 `android_context()` 该放哪里 | 函数体 vs use_effect vs 事件处理器 | **[ndk-context.md](ndk-context.md)** → 避坑规则表格 |
| 想知道 hooks 在销毁时会不会执行 | use_effect/use_drop 在 scope drop 时的行为差异 | **[hooks-lifecycle.md](../mt-note-dioxus-reference/hooks-lifecycle.md)** → 第 3 节表格 + 第 5.3 节销毁流程 |

## 五、Release 打包与签名

### 5.1 `dx bundle -r` 的隐藏条件

dioxus-cli 0.7.10 源码 `android.rs:419`：

```rust
let build_type = if self.release && self.config.bundle.android.is_some() {
    "assembleRelease"
} else {
    "assembleDebug"
};
```

**即使传了 `-r`，只要 Dioxus.toml 没有 `[bundle.android]`，就静默降级为 `assembleDebug`**。签名配置是触发 release 构建的硬开关。

### 5.2 不泄密密码的方案：临时 JKS + apksigner 覆盖

真实 keystore 密码不写入 Dioxus.toml。用一套**临时 JKS**（密码随意）过 Gradle 检查，产出签名 APK 后用 `apksigner` 覆盖为正式签名。

**1. 生成临时 JKS**（一次性）：

```bash
keytool -genkey -v -keystore dioxus-build.jks -alias temp \
  -keyalg RSA -keysize 2048 -validity 10000 \
  -storepass temp123 -keypass temp123 \
  -dname "CN=Build, OU=Dev, O=Temp, L=City, S=State, C=CN"
```

**2. Dioxus.toml 配置**：

```toml
[bundle.android]
jks_file = "crates/mobile/dioxus-build.jks"
jks_password = "temp123"
key_alias = "temp"
key_password = "temp123"
```

`jks_file` 相对路径基准是 workspace 根。JKS 文件放入 `.gitignore`：

```
**/dioxus-build.jks
```

**3. 构建 → apksigner 覆盖签名**：

```bash
dx bundle -p pub-trade-mobile --android -r --package-types apk
# 产出已签名 APK（临时 JKS），然后覆盖
python scripts/sign_apk.py -p <real_password>
```

`apksigner sign` 直接覆盖 Gradle 的临时签名，无需先解签。

### 5.3 `[bundle.android]` 字段说明

| 字段 | 说明 |
|------|------|
| `jks_file` | 相对 workspace 根的 JKS 路径 |
| `jks_password` | keystore 密码 |
| `key_alias` | 密钥别名 |
| `key_password` | 密钥密码 |

四个字段均为 `AndroidSettings` 结构体必填项，缺一不可。

---

## 六、背景参考

- [ndk-context.md](./ndk-context.md) — ndk-context 原理、生态标准地位、初始化/销毁链路、JNI 函数定义关系、**panic 窗口与 Dioxus 避坑指南**
  - **Android Kotlin → Rust** 场景：参考「ndk-context 与 JNI 函数定义的关系 → Kotlin→Rust」
  - **Rust → Android Kotlin** 场景：参考「ndk-context 与 JNI 函数定义的关系 → Rust→Kotlin」
  - **为什么 `android_context()` 在销毁时 panic**：参考「panic 窗口与 Dioxus 避坑指南」
  - **`android_context()` 应该放在哪里调用**：参考「避坑规则」表格
