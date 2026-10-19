# Mobile

Dioxus Android 平台包。编译为 `.so` 由 Android `NativeActivity` 加载，通过 WebView 渲染 UI。

## 目录结构

```
src/
├── main.rs                        # 入口（launch + Router）
├── frontend/
│   ├── mod.rs
│   ├── app.rs                     # 根布局（主题加载、样式注入、持久化同步）
│   ├── app.module.scss            # 根布局样式（含主题 CSS 变量）
│   ├── router.rs                  # 路由枚举（Home / About / Settings）
│   ├── components/
│   │   ├── mod.rs
│   │   └── appbar/                # 顶部导航栏组件
│   │       ├── mod.rs
│   │       ├── appbar.module.scss
│   │       └── components/
│   │           └── appbar_menu/   # 导航栏下拉菜单
│   ├── views/
│   │   ├── mod.rs
│   │   ├── home/
│   │   │   ├── mod.rs
│   │   │   └── home.module.scss
│   │   ├── about/
│   │   │   ├── mod.rs
│   │   │   └── about.module.scss
│   │   └── settings/
│   │       ├── mod.rs
│   │       └── settings.module.scss
│   ├── store/
│   │   ├── mod.rs
│   │   ├── appbar_state.rs        # AppBar 全局状态（菜单项、搜索）
│   │   └── storage/
│   │       ├── mod.rs
│   │       ├── android.rs         # Android 文件型持久化后端
│   │       └── jni.rs             # JNI 桥接（状态栏主题控制）
│   └── hooks/
│       ├── mod.rs
│       └── use_appbar_state.rs    # 消费 AppBar 状态
android/
│   └── MainActivity.kt            # Kotlin Activity（edge-to-edge + 状态栏主题）
assets/styles/
│   ├── variables.scss             # Material Design 3 设计 token
│   ├── theme.scss                 # 亮/暗色 CSS 自定义属性
│   └── bundle-mobile.scss         # Stylance 编译产物
build.rs                           # Stylance 构建脚本
```

## JNI 桥接

通过 `ndk_context::android_context().context()` 获取 NativeActivity 实例指针，以 instance method 调用 Kotlin 方法，**不使用 `FindClass`**（避免 native 线程 class loader 问题）。

| 函数 | Kotlin 方法 | 用途 |
|------|------------|------|
| `set_system_bar_theme()` | `MainActivity.setSystemBarTheme(Z)V` | 切换系统栏图标亮/暗色 |

jni 0.22 API：线程挂载使用闭包形式 `attach_current_thread(|env| { ... })`，方法调用使用 `JNIString` + `jni_sig!()` 宏。

## 持久化

`AndroidBackend` 通过 JNI 调用 `getFilesDir()` 获取 app 私有目录，以 JSON 文件存储键值对：

```
<filesDir>/persist.json
```

封装为 `PersistStorage<AndroidBackend>`，提供类型安全的 `load::<Theme>()` / `save(&theme)` 接口。主题变更时自动持久化并同步状态栏主题。

## 构建

```bash
# clippy
cargo clippy -p mt-note-mobile --target aarch64-linux-android

# 构建 APK
dx bundle -p mt-note-mobile --android --target aarch64-linux-android --package-types apk
```