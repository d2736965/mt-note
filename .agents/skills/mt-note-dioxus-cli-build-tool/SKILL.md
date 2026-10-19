---
name: mt-note-dioxus-cli-build-tool
description: dioxus-cli 通用构建工具参考（dx serve/bundle 命令、Dioxus.toml 配置、多平台构建）。当用户涉及 dx 命令行工具、Dioxus.toml 配置、跨平台（Web/Android/Desktop）构建问题时调用。（mt-note 项目自带版本）
---

# dioxus-cli 构建工具参考

dioxus-cli 是通用跨平台构建工具，任何 Rust 项目都可用 `dx serve` / `dx bundle` 编译打包到 Web、Android、Desktop。

> 版本基线：0.7.10。已知问题 #5719：bin target 的 crate name ≠ package name 时，官方版 `dx serve` 热补丁失效并静默回退整页重建（mt-note 维护者的 dx 构建自已修复该问题的补丁分支，贡献者用官方版即可，热补丁异常时表现为整页重建属正常兜底）。

## 命令

```bash
dx serve                # Desktop 开发模式
dx serve --web          # Web 开发模式
dx serve --android      # Android 开发模式
dx bundle --web --release         # Web release
dx bundle --android --package-types apk --target aarch64-linux-android  # Android release
```

## 配置

### Dioxus.toml — 通用

```toml
[application]
asset_dir = "assets"
```

### Dioxus.toml — Web

```toml
[web.app]
title = "App Title"

[web.watcher]
watch_path = ["src", "assets"]   # 必须含 assets，否则资源变更不触发热更新

[application]
public_dir = "assets"            # Web 平台静态文件目录

[web.resource]
style = ["style.css"]            # 相对于 public_dir
```

### Dioxus.toml — Android

```toml
[android]
min_sdk = 26
target_sdk = 36
gradle_dependencies = ["androidx.games:games-activity:3.0.5"]

[application]
android_main_activity = "android/MainActivity.kt"
```

```kotlin
// android/MainActivity.kt
package dev.dioxus.main

typealias BuildConfig = com.example.app.foo.BuildConfig

class MainActivity : com.google.androidgamesdk.GameActivity()
```

**注意**：`MainActivity.kt`、`build.gradle.kts` 等是 dx 每次覆盖写入的，需在项目源目录通过 Dioxus.toml 配置修改，不能直接改 `target/dx/` 下的副本。

### Cargo.toml — Bevy Features

```toml
bevy = { version = "0.18", default-features = false, features = [
    "default_platform",     # 必须
    "ui_bevy_render",       # UI 项目必须（含 bevy_text、bevy_ui、bevy_render）
] }
```

| Feature | 说明 |
|---------|------|
| `default_platform` | 必须。含 winit、clipboard、default_font、multi_threaded |
| `ui_bevy_render` | UI 项目必须。含 `common_api`(bevy_text) + `bevy_ui` + `bevy_render` |
| `android-game-activity` | Android 必须。拉入 android-activity、ndk-sys |
| ~~`default_app`~~ | 冗余，已被 ui_api 包含 |
| ~~`bevy_text`~~ | 冗余，已被 common_api 包含 |

依赖链：`ui_bevy_render → ui_api → { default_app, common_api(含 bevy_text), bevy_ui }`

### Cargo.toml — 编译优化

```toml
[profile.dev]
opt-level = 1                    # 自己代码低优化，编译快

[profile.dev.package."*"]
opt-level = 3                    # 依赖全优化，WASM 体积小运行快
```

首次编译慢（依赖全优化），后续增量编译只重编自己代码。

---

## Desktop

CWD 为 `target/dx/<name>/debug/windows/app/`，不是项目根目录。资产路径用绝对路径：

```rust
format!("{}/assets", env!("CARGO_MANIFEST_DIR"))
```

| 问题 | 原因 | 解决 |
|------|------|------|
| `Path not found: .../assets/...` | 用了相对路径 `"assets"` | 改用 `env!("CARGO_MANIFEST_DIR")` 绝对路径 |

---

## Web（WASM）

### 资产路径

dioxus-cli 将 `public_dir` 复制到 web 根目录，`AssetPlugin.file_path` 设为 `"."`：

```rust
if cfg!(target_arch = "wasm32") {
    ".".to_string()
} else {
    format!("{}/assets", env!("CARGO_MANIFEST_DIR"))
}
```

### Canvas 自适应

```rust
.set(WindowPlugin {
    primary_window: Some(Window {
        fit_canvas_to_parent: true,
        ..default()
    }),
    ..default()
})
```

### CSS

`assets/style.css`：

```css
html, body {
  margin: 0;
  padding: 0;
  width: 100%;
  height: 100%;
}
```

### 常见问题

| 问题 | 原因 | 解决 |
|------|------|------|
| Canvas 不跟随窗口 | 未设 `fit_canvas_to_parent` 或 body 有 margin | 两项均配置 |
| 资产修改后未热更新 | dioxus-cli 缓存 | 重启 `dx serve` |
| 字体加载 200 但不显示 | 异步未就绪 | 首帧异步属正常，等待即可 |

---

## Android

### 入口：android_main

Bevy 的 Android feature 通过 `android-activity` crate 胶水代码调用 `android_main`——**不是 Android 系统直接调用**。

| Feature | Activity 基类 | 入口链路 |
|---------|--------------|---------|
| `android-native-activity` | `NativeActivity`（内置） | `dlsym("ANativeActivity_onCreate")` → `rust_glue_entry()` → `android_main()` |
| `android-game-activity` | `GameActivity`（Jetpack） | GameActivity C glue → native 线程 → `_rust_glue_entry()` → `android_main()` |

**完整调用链（GameActivity）**：

```
Android 创建 MainActivity（Kotlin: extends GameActivity）
  → GameActivity.onCreate()
    → JNI → android_native_app_glue.c（SDK 的 C 胶水代码）
      → 创建 native 线程
        → _rust_glue_entry(ptr)              // android-activity crate #[no_mangle] extern "C"
          → 构造 AndroidApp
          → extern "Rust" { android_main(app) }  // 链接时解析符号
            → ANDROID_APP.set(app)           // 存储到 OnceLock ← 关键步骤
            → main() → Bevy 启动
              → AndroidAssetReader
                → ANDROID_APP.get() → asset_manager() → 从 APK 读取
```

**符号匹配**：`android-activity` 声明 `extern "Rust" { fn android_main(app: AndroidApp); }` 不生成符号，由应用 crate 的 `#[no_mangle] fn android_main` 链接时提供。

两种写法，**二选一**（同时使用会重复符号链接错误）：

**方式一：`#[bevy_main]` 宏（简洁）**

```rust
#[bevy_main]
fn main() {
    App::new()
        // ...
        .run();
}
```

宏展开等价于方式二的手动实现，仅在 `target_os = "android"` 时生成 `android_main` 入口，其他平台是普通 `main()`。

**方式二：手动实现（透明，可自定义错误处理）**

```rust
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(android_app: bevy::android::android_activity::AndroidApp) {
    if let Err(_e) = bevy::android::ANDROID_APP.set(android_app) {
        eprintln!("ANDROID_APP.set 失败：重复调用 android_main");
        return;
    }
    main();
}
```

### AndroidAssetReader

Bevy 编译时通过 `#[cfg(target_os = "android")]` 选择 `AndroidAssetReader`，**完全忽略 `AssetPlugin.file_path`**：

```rust
// Bevy 源码
#[cfg(target_os = "android")]
return Box::new(AndroidAssetReader);  // 忽略 _path，直接读 APK assets/
```

因此 `file_path` 设什么值都不影响行为，但建议显式跳过 Android 以保持代码清晰：

```rust
let mut asset_plugin = AssetPlugin::default();
if !cfg!(target_os = "android") {
    asset_plugin.file_path = if cfg!(target_arch = "wasm32") {
        ".".into()
    } else {
        format!("{}/assets", env!("CARGO_MANIFEST_DIR"))
    };
}
```

### 资产打包

dioxus-cli 的资产管线分两套机制：

| 机制 | 入口 | 场景 |
|------|------|------|
| `asset!()` 宏（Manganis） | 编译时嵌入 `__ASSETS__` → `write_assets()` 提取 | Dioxus 静态资源 |
| 文件系统资产 | 需自行确保到达 APK 的 `assets/` | Bevy `AssetServer::load()` 等 |

dioxus-cli 创建 Android 项目时，`assets/` 目录只做 `create_dir_all`，**不写任何文件**。后端 `write_assets()` 也只复制 Manganis 资产。因此 Bevy 的文件系统资产（字体等）**不会自动进入 APK**。

**解决**：`build.rs` 在编译 Android 目标时，将项目 `assets/` 复制到 Android 构建目录：

```rust
// build.rs
use std::path::Path;

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() != "android" {
        return;
    }

    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let pkg = std::env::var("CARGO_PKG_NAME").unwrap();
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());

    let target_dir = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(&manifest).join("target"));

    let src = Path::new(&manifest).join("assets");
    let dest = target_dir
        .join("dx").join(&pkg).join(&profile).join("android")
        .join("app").join("app").join("src").join("main").join("assets");

    if dest.exists() {
        copy_dir(&src, &dest);
    }
}

fn copy_dir(src: &Path, dest: &Path) {
    for entry in std::fs::read_dir(src).into_iter().flatten() {
        let entry = entry.unwrap();
        let dest_path = dest.join(entry.file_name());
        if entry.path().is_dir() {
            std::fs::create_dir_all(&dest_path).unwrap();
            copy_dir(&entry.path(), &dest_path);
        } else {
            std::fs::create_dir_all(dest_path.parent().unwrap()).unwrap();
            std::fs::copy(entry.path(), &dest_path).unwrap();
        }
    }
}
```

**原理**：dx 构建顺序是同步的，`cargo build`（触发 build.rs）在 Gradle 打包之前，且 dx 只 `remove_dir_all` jniLibs，不清 assets：

```
prepare_build_dir()  → remove_dir_all(jniLibs) + create_dir_all(assets)  // 不清 assets
build_android_app_dir() → create_dir_all(assets) 不写文件
cargo build           → build.rs 复制项目 assets 到位
write_assets()        → 只处理 asset!() 宏，不删其他文件
assemble_android()    → Gradle mergeDebugAssets 打包
```

**验证**：`unzip -l app-debug.apk | grep assets/`

---

## 字体 .meta 文件

Bevy 0.18 要求每个字体旁有 `.meta` 文件：

```ron
(
    meta_format_version: "1.0",
    asset: Load(
        loader: "bevy_text::font_loader::FontLoader",
        settings: (),
    ),
)
```

- `loader` 必须用完整类型路径，`"Font"` 短名称无效
- `settings` 不可省略（FontLoader 的 Settings 是 `()`)
- `meta_format_version` 固定 `"1.0"`

---

## 验证清单

### Desktop
- [ ] `AssetPlugin.file_path` 使用 `env!("CARGO_MANIFEST_DIR")` 绝对路径
- [ ] `dx serve` 无 `Path not found`

### Web
- [ ] `file_path` 在 wasm32 下为 `"."`
- [ ] `fit_canvas_to_parent: true`
- [ ] CSS body 无 margin、100% 宽高
- [ ] `public_dir = "assets"`，`watch_path` 含 `"assets"`

### Android
- [ ] `build.rs` 复制 assets 到 Android 构建目录
- [ ] `android_main` 中 `ANDROID_APP.set()` 正确处理返回值（不用 `let _ =`）
- [ ] 自定义字体有 `.meta` 文件
- [ ] `dx bundle --android` 后 `unzip -l <apk> | grep assets/` 验证资产打包
