---
name: "mt-note-dioxus-toml"
description: "Dioxus.toml 配置参考（0.7.10 版本）。当用户涉及 Dioxus.toml 配置编写、打包设置、权限声明、Android intent-filter、AndroidManifest 自定义、平台特定配置时调用。含 Android 配置生效状态验证（哪些字段实际生效、哪些是死字段）。"
---

# Dioxus.toml 配置参考（Dioxus 0.7.10）

基于 Dioxus 0.7.10 CLI 源码（`packages/cli/src/config/` + `packages/cli/src/build/android.rs` +
`packages/cli/assets/android/`）整理，并经源码验证各配置项的实际生效状态。

## 顶层结构

```toml
[application]    # 应用基本信息
[web]            # Web 平台配置
[bundle]         # 打包身份与资源
[components]     # 组件配置
[permissions]    # 跨平台权限声明
[deep_links]     # 深度链接
[background]     # 后台模式
[ios]            # iOS 特定配置
[android]        # Android 特定配置
[macos]          # macOS 特定配置
[windows]        # Windows 特定配置
[linux]          # Linux 特定配置
```

## [application] — 应用基本信息

| 字段                      | 类型              | 默认值     | 说明                       |
| ------------------------- | ----------------- | ---------- | -------------------------- |
| `name`                    | `Option<String>`  | None       | 应用显示名称               |
| `asset_dir`               | `Option<PathBuf>` | None       | 全局资源目录               |
| `out_dir`                 | `Option<PathBuf>` | None       | 构建输出目录               |
| `public_dir`              | `Option<PathBuf>` | `"public"` | 静态文件目录；设 `""` 禁用 |
| `tailwind_input`          | `Option<PathBuf>` | None       | Tailwind 输入文件          |
| `tailwind_output`         | `Option<PathBuf>` | None       | Tailwind 输出文件          |
| `ios_info_plist`          | `Option<PathBuf>` | None       | 自定义 iOS Info.plist      |
| `macos_info_plist`        | `Option<PathBuf>` | None       | 自定义 macOS Info.plist    |
| `ios_entitlements`        | `Option<PathBuf>` | None       | 自定义 iOS entitlements    |
| `macos_entitlements`      | `Option<PathBuf>` | None       | 自定义 macOS entitlements  |
| `android_manifest`        | `Option<PathBuf>` | None       | 自定义 AndroidManifest.xml（**整体替换**，跳过 hbs 模板，见下方 Android 节详解） |
| `android_main_activity`   | `Option<PathBuf>` | None       | 自定义 MainActivity.kt     |
| `android_min_sdk_version` | `Option<u32>`     | None       | Android 最低 SDK 版本      |

## [web] — Web 平台配置

### [web.app]

| 字段        | 类型             | 默认值   | 说明         |
| ----------- | ---------------- | -------- | ------------ | ------------------- |
| `title`     | `String`         | `"dioxus | ⛺"`         | HTML title 标签内容 |
| `base_path` | `Option<String>` | None     | 部署基础路径 |

### [web.watcher]

| 字段           | 类型           | 默认值                | 说明             |
| -------------- | -------------- | --------------------- | ---------------- |
| `watch_path`   | `Vec<PathBuf>` | `["src", "examples"]` | 监听路径         |
| `reload_html`  | `bool`         | `false`               | HTML 变更时重载  |
| `index_on_404` | `bool`         | `true`                | 404 时回退 index |

**⚠️ 常见坑**：`watch_path` 默认值**不包含** `assets` 目录，SCSS/CSS 等资源文件变更不会触发热更新。自定义 `watch_path` 时会**覆盖**默认值，必须显式包含 `src`：

```toml
[web.watcher]
watch_path = ["src", "assets", "android"]
```

### [web.resource]

| 字段     | 类型                   | 说明          |
| -------- | ---------------------- | ------------- |
| `style`  | `Option<Vec<PathBuf>>` | 额外 CSS 文件 |
| `script` | `Option<Vec<PathBuf>>` | 额外 JS 文件  |

### [web.resource.dev]

| 字段     | 类型           | 说明             |
| -------- | -------------- | ---------------- |
| `style`  | `Vec<PathBuf>` | 开发模式额外 CSS |
| `script` | `Vec<PathBuf>` | 开发模式额外 JS  |

### [web.proxy]

数组，每项：

| 字段      | 类型     | 说明         |
| --------- | -------- | ------------ |
| `backend` | `String` | 代理后端地址 |

### [web.https]

| 字段        | 类型             | 说明                     |
| ----------- | ---------------- | ------------------------ |
| `enabled`   | `Option<bool>`   | 启用 HTTPS               |
| `mkcert`    | `Option<bool>`   | 使用 mkcert 生成本地证书 |
| `key_path`  | `Option<String>` | TLS 私钥路径             |
| `cert_path` | `Option<String>` | TLS 证书路径             |

### Web 其他

| 字段           | 类型   | 默认值  | 说明                             |
| -------------- | ------ | ------- | -------------------------------- |
| `pre_compress` | `bool` | `false` | release 构建时预压缩 wasm/assets |

### [web.wasm_opt]

| 字段             | 类型           | 默认值  | 说明                                                |
| ---------------- | -------------- | ------- | --------------------------------------------------- |
| `level`          | `WasmOptLevel` | `"z"`   | 优化级别：`"z"`/`"s"`/`"0"`/`"1"`/`"2"`/`"3"`/`"4"` |
| `debug`          | `bool`         | `false` | 保留 debug 符号                                     |
| `keep_names`     | `bool`         | `false` | 保留 name section（用于 panic 回溯）                |
| `memory_packing` | `bool`         | `false` | 启用内存打包                                        |
| `extra_features` | `Vec<String>`  | `[]`    | 额外 wasm-opt 参数，如 `["--enable-simd"]`          |

## [bundle] — 打包身份与资源

| 字段                 | 类型                  | 说明                                                                     |
| -------------------- | --------------------- | ------------------------------------------------------------------------ |
| **`identifier`**     | `Option<String>`      | **应用唯一标识符**（反向域名格式，如 `"com.example.app"`），各平台回退值 |
| `publisher`          | `Option<String>`      | 发布者名称                                                               |
| `icon`               | `Option<Vec<String>>` | 图标文件路径列表                                                         |
| `resources`          | `Option<Vec<String>>` | 额外资源文件                                                             |
| `version`            | `Option<String>`      | 应用版本号                                                               |
| `copyright`          | `Option<String>`      | 版权信息                                                                 |
| `category`           | `Option<String>`      | 应用分类                                                                 |
| `short_description`  | `Option<String>`      | 简短描述                                                                 |
| `long_description`   | `Option<String>`      | 详细描述                                                                 |
| `external_bin`       | `Option<Vec<String>>` | 外部二进制文件                                                           |
| `file_version`       | `Option<String>`      | 文件版本（Windows）                                                      |
| `original_file_name` | `Option<String>`      | 原始文件名                                                               |
| `trademark`          | `Option<String>`      | 商标                                                                     |

### identifier 解析优先级

每个平台有独立 `identifier` 字段，若设置则覆盖 `[bundle].identifier`；未设置则回退到 `[bundle].identifier`。Web 平台无 identifier。

```
resolved_identifier(platform) = platform.identifier ?? bundle.identifier
```

## [permissions] — 跨平台权限声明

统一声明权限，自动映射到平台原生标识（AndroidManifest.xml / Info.plist）。

| 字段                  | TOML 键名             | 类型                         | 映射说明                                                                                                                        |
| --------------------- | --------------------- | ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `location`            | `location`            | `{ precision, description }` | precision: `"fine"`/`"coarse"`；Android→ACCESS_FINE/COARSE_LOCATION，iOS→NSLocationWhenInUseUsageDescription                    |
| `camera`              | `camera`              | `{ description }`            | Android→CAMERA，iOS→NSCameraUsageDescription                                                                                    |
| `microphone`          | `microphone`          | `{ description }`            | Android→RECORD_AUDIO，iOS→NSMicrophoneUsageDescription                                                                          |
| `notifications`       | `notifications`       | `{ description }`            | Android→POST_NOTIFICATIONS                                                                                                      |
| `photos`              | `photos`              | `{ access, description }`    | access: `"read"`/`"write"`/`"read-write"`；Android→READ_MEDIA_IMAGES/WRITE_EXTERNAL_STORAGE，iOS→NSPhotoLibraryUsageDescription |
| `bluetooth`           | `bluetooth`           | `{ description }`            | Android→BLUETOOTH_CONNECT+SCAN，iOS→NSBluetoothAlwaysUsageDescription                                                           |
| `background_location` | `background-location` | `{ description }`            | Android→ACCESS_BACKGROUND_LOCATION，iOS→NSLocationAlwaysAndWhenInUseUsageDescription                                            |
| `contacts`            | `contacts`            | `{ access, description }`    | access: `"read"`/`"write"`/`"read-write"`；Android→READ/WRITE_CONTACTS，iOS→NSContactsUsageDescription                          |
| `calendar`            | `calendar`            | `{ access, description }`    | access: `"read"`/`"write"`/`"read-write"`；Android→READ/WRITE_CALENDAR，iOS→NSCalendarsUsageDescription                         |
| `biometrics`          | `biometrics`          | `{ description }`            | Android→USE_BIOMETRIC，iOS→NSFaceIDUsageDescription                                                                             |
| `nfc`                 | `nfc`                 | `{ description }`            | Android→NFC，iOS→NFCReaderUsageDescription                                                                                      |
| `motion`              | `motion`              | `{ description }`            | Android→ACTIVITY_RECOGNITION，iOS→NSMotionUsageDescription                                                                      |
| `health`              | `health`              | `{ access, description }`    | access: `"read"`/`"write"`/`"read-write"`；Android→BODY_SENSORS，iOS→NSHealthShare/UpdateUsageDescription                       |
| `speech`              | `speech`              | `{ description }`            | Android→RECORD_AUDIO，iOS→NSSpeechRecognitionUsageDescription                                                                   |
| `media_library`       | `media-library`       | `{ description }`            | Android→READ_MEDIA_AUDIO，iOS→NSAppleMusicUsageDescription                                                                      |
| `siri`                | `siri`                | `{ description }`            | iOS only→NSSiriUsageDescription                                                                                                 |
| `homekit`             | `homekit`             | `{ description }`            | iOS only→NSHomeKitUsageDescription                                                                                              |
| `local_network`       | `local-network`       | `{ description }`            | iOS only→NSLocalNetworkUsageDescription                                                                                         |
| `nearby_wifi`         | `nearby-wifi`         | `{ description }`            | Android only→NEARBY_WIFI_DEVICES                                                                                                |

### 权限示例

```toml
[permissions]
location = { precision = "fine", description = "追踪您的位置" }
camera = { description = "拍照用于个人资料" }
photos = { access = "read-write", description = "访问相册" }
```

## [deep_links] — 深度链接

```toml
[deep_links]
schemes = ["myapp", "com.example.myapp"]   # URL scheme（myapp://path）
hosts = ["example.com", "*.example.com"]    # Universal/App Link 域名
paths = ["/app/*", "/share/*"]              # 路径模式，空则匹配所有
```

映射：iOS→CFBundleURLSchemes + Associated Domains，Android→intent-filter + App Links。

## [background] — 后台模式

```toml
[background]
location = true              # 后台定位
audio = true                 # 后台音频
fetch = true                 # 后台数据获取
remote-notifications = true  # 远程推送
voip = false                 # VoIP 通话
bluetooth = false            # 后台蓝牙
external-accessory = false   # 外部配件
processing = false           # 后台处理
```

映射：iOS→UIBackgroundModes，Android→对应前台服务类型。

## [ios] — iOS 特定配置

### Bundle 覆盖字段

`identifier`、`publisher`、`icon`、`resources`、`copyright`、`category`、`short_description`、`long_description` 均可覆盖 `[bundle]` 对应值。

### iOS 专有字段

| 字段                | 类型              | 说明                                         |
| ------------------- | ----------------- | -------------------------------------------- |
| `deployment_target` | `Option<String>`  | 最低 iOS 版本，如 `"15.0"`                   |
| `info_plist`        | `Option<PathBuf>` | 自定义 Info.plist 路径                       |
| `url_schemes`       | `Vec<String>`     | 额外 URL scheme（合并到 deep_links.schemes） |
| `background_modes`  | `Vec<String>`     | 额外后台模式（合并到 background）            |

### [ios.entitlements]

| 字段                     | TOML 键名                | 类型             | 说明                             |
| ------------------------ | ------------------------ | ---------------- | -------------------------------- |
| `app_groups`             | `app-groups`             | `Vec<String>`    | App Group                        |
| `aps_environment`        | `aps-environment`        | `Option<String>` | `"development"` / `"production"` |
| `associated_domains`     | `associated-domains`     | `Vec<String>`    | 关联域名                         |
| `icloud`                 | -                        | `bool`           | iCloud                           |
| `keychain_access_groups` | `keychain-access-groups` | `Vec<String>`    | 钥匙串访问组                     |
| `apple_pay`              | `apple-pay`              | `bool`           | Apple Pay                        |
| `healthkit`              | -                        | `bool`           | HealthKit                        |
| `homekit`                | -                        | `bool`           | HomeKit                          |
| `additional`             | -                        | `HashMap`        | 额外 entitlements（任意键值）    |

### [ios.plist]

任意 `HashMap<String, serde_json::Value>`，合并到 Info.plist。

### [ios.raw]

| 字段           | 说明                               |
| -------------- | ---------------------------------- |
| `info_plist`   | 注入 Info.plist 的原始 XML         |
| `entitlements` | 注入 entitlements.plist 的原始 XML |

### [[ios.document_types]]

| 字段         | 类型             | 说明                                     |
| ------------ | ---------------- | ---------------------------------------- |
| `name`       | `String`         | 文档类型名                               |
| `extensions` | `Vec<String>`    | 文件扩展名                               |
| `mime_types` | `Vec<String>`    | MIME 类型                                |
| `types`      | `Vec<String>`    | UTI 类型                                 |
| `icon`       | `Option<String>` | 图标                                     |
| `role`       | `Option<String>` | `"Editor"`/`"Viewer"`/`"Shell"`/`"None"` |

### [[ios.widget_extensions]]

| 字段                | 类型             | 说明                         |
| ------------------- | ---------------- | ---------------------------- |
| `source`            | `String`         | Swift 源码目录（相对项目根） |
| `display_name`      | `String`         | Widget 显示名                |
| `bundle_id_suffix`  | `String`         | Bundle ID 后缀               |
| `deployment_target` | `Option<String>` | 最低部署版本                 |
| `module_name`       | `String`         | Swift 模块名                 |

## [android] — Android 特定配置

### Bundle 覆盖字段

同 iOS，`identifier`、`publisher`、`icon`、`resources`、`copyright`、`category`、`short_description`、`long_description` 可覆盖。

### Android 专有字段

| 字段                       | 类型              | 说明                                              |
| -------------------------- | ----------------- | ------------------------------------------------- |
| `min_sdk`                  | `Option<u32>`     | 最低 SDK 版本                                     |
| `target_sdk`               | `Option<u32>`     | 目标 SDK 版本                                     |
| `compile_sdk`              | `Option<u32>`     | 编译 SDK 版本                                     |
| `lib_name`                 | `Option<String>`  | NativeActivity 库名（默认 `"main"`→`libmain.so`） |
| `features`                 | `Vec<String>`     | 硬件/软件特性需求                                 |
| `~~manifest~~`             | ~~`Option<PathBuf>`~~ | **⚠️ 死字段，不生效**（见下方 AndroidManifest 生成逻辑） |
| `gradle_dependencies`      | `Vec<String>`     | Gradle 依赖                                       |
| `gradle_plugins`           | `Vec<String>`     | Gradle 插件                                       |
| `proguard_rules`           | `Vec<PathBuf>`    | ProGuard 规则文件                                 |
| `url_schemes`              | `Vec<String>`     | 额外 URL scheme                                   |
| `foreground_service_types` | `Vec<String>`     | 前台服务类型                                      |

### 自定义 MainActivity

通过 `[application]` 中的 `android_main_activity` 指定自定义 Kotlin Activity 文件路径：

```toml
[application]
android_main_activity = "android/MainActivity.kt"
```

Kotlin Activity 必须继承 `WryActivity`（Dioxus 内置的 NativeActivity 子类）。

**⚠️ 必须包含 `typealias BuildConfig`**：CLI 模板生成的 MainActivity 包含此行，自定义时也必须保留，否则编译失败。`typealias` 必须放在 import 之后（Kotlin 文件结构：`package` → `import` → `typealias` → `class`）。

```kotlin
package dev.dioxus.main

import android.os.Bundle

typealias BuildConfig = com.example.myapp.BuildConfig

class MainActivity : WryActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
    }
}
```

其中 `com.example.myapp` 即 `[bundle].identifier` 的值，直接使用，不做任何转换。

**AndroidManifest.xml 生成逻辑**（CLI 源码 `android.rs:264-270`）：

1. 如果 `[application] android_manifest = "路径"` 指定了文件，**直接读取该文件内容作为最终 AndroidManifest.xml**（整体替换，**完全跳过 hbs 模板**）。用户必须自行写全所有必要内容（permissions、NativeActivity meta-data、MAIN/LAUNCHER intent-filter 等）。
2. 否则使用 CLI 内置的 `AndroidManifest.xml.hbs` 模板，Handlebars 渲染注入：`permissions`、`features`、`raw_manifest`、`url_schemes`、`app_link_hosts`、`uses_cleartext_traffic` 等。

**⚠️ `[android] manifest` 是死字段**：config struct 有此字段（`manifest.rs:616`），但 CLI 代码从不读取它。配了不报错也不生效。自定义 manifest 只能用 `[application] android_manifest`。

#### `[application] android_manifest` 互斥关系（核心避坑）

整体替换 manifest 后，**只有 AndroidManifest.xml.hbs 模板被跳过**。其它模板（build.gradle.kts.hbs、strings.xml.hbs）照常执行，不受影响。

但 AndroidManifest.xml.hbs 本应填充的以下内容**不再自动生成**，用户必须在自定义 manifest 中手写：

| 被 hbs 模板填充、整体替换后丢失的内容 | 来源配置节 | 说明 |
|---------------------------------------|-----------|------|
| `<uses-permission>` | `[permissions]` + `[android.permissions]` | **不再自动生成**，须手写 |
| `<uses-feature>` | `[android.features]` | **不再自动生成**，须手写 |
| `raw_manifest` XML | `[android.raw.manifest]` | **不再注入**，直接写在 manifest 里即可 |
| `<intent-filter>` VIEW（deep link） | `[deep_links]` / `[android.url_schemes]` | **不再自动生成**，须手写 intent-filter |
| `<activity>` NativeActivity（含 meta-data） | hbs 模板固定生成 | **不再生成**，须手写 activity + `android.app.lib_name`/`func_name` meta-data |
| `<intent-filter>` MAIN/LAUNCHER | hbs 模板固定生成 | **不再生成**，须手写 |
| foreground service | `[background]` / `[android.foreground_service_types]` | **不再生成**，须手写 |

**不受影响（其它模板照常执行）**：

| 内容 | 来源 | 仍生效 |
|------|------|--------|
| build.gradle.kts（SDK 版本、签名、gradle 依赖） | `[android] min_sdk/target_sdk` / `[android] gradle_dependencies` / `[bundle.android]` | ✅ |
| strings.xml（app_name 等） | `[application] name` / `[web.app] title` | ✅ |
| MainActivity.kt | `[application] android_main_activity` | ✅ |
| ProGuard 规则 | `[android] proguard_rules` | ✅ |

**实践建议**：**推荐使用 `[application] android_manifest` 整体替换**，而非依赖 hbs 模板变量。
原因：hbs 模板的注入点有限（permissions/features/raw_manifest/url_schemes 等），
intent-filter、activity-alias、service 等 application 内部元素无法通过 hbs 配置注入
（intent_filters 配置不生效、raw.application 无注入点）。整体替换 manifest 后不受这些限制，
manifest 的全部能力都可用，后期扩展（如接收分享、文件关联、深层链接、自定义 service 等）不受限。

代价是 permissions 等内容需在 manifest 文件中手动维护，不再从 `[permissions]`/`[android.permissions]`
自动填充。但 manifest 权限声明本就是静态的，手动维护成本可控。

**自定义 manifest 基础模板**：以下是 CLI 内置的 `AndroidManifest.xml.hbs` 源码（`packages/cli/assets/android/gen/app/src/main/AndroidManifest.xml.hbs`）。整体替换时以此为基础，把 `{{...}}` 变量替换为实际值，再添加自定义内容（intent-filter 等）：

```xml
<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android">
    <!-- Default permissions -->
    <uses-permission android:name="android.permission.INTERNET" />

    <!-- Permissions from Dioxus.toml [permissions] section -->
    {{#each permissions}}
    <uses-permission android:name="{{ this }}" />
    {{/each}}

    <!-- Hardware features from Dioxus.toml [android] section -->
    {{#each features}}
    <uses-feature android:name="{{ this }}" android:required="true" />
    {{/each}}

    <!-- Raw manifest XML from Dioxus.toml [android.raw.manifest] -->
    {{{ raw_manifest }}}

    <application android:hasCode="true" android:icon="@mipmap/ic_launcher"
        android:extractNativeLibs="true"
        android:allowNativeHeapPointerTagging="false"
        android:label="@string/app_name"
        android:supportsRtl="{{#if supports_rtl}}{{ supports_rtl }}{{else}}true{{/if}}"
        android:theme="{{#if app_theme}}{{ app_theme }}{{else}}@style/AppTheme{{/if}}"
        {{#if uses_cleartext_traffic}}android:usesCleartextTraffic="{{ uses_cleartext_traffic }}"{{/if}}
        {{#if large_heap}}android:largeHeap="{{ large_heap }}"{{/if}}
        android:networkSecurityConfig="@xml/network_security_config">
        <activity android:configChanges="orientation|screenLayout|screenSize|keyboardHidden" android:exported="true"
            android:label="@string/app_name" android:name="dev.dioxus.main.MainActivity">
            <meta-data android:name="android.app.lib_name" android:value="{{ lib_name }}" />
            <meta-data android:name="android.app.func_name" android:value="ANativeActivity_onCreate" />
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>

            {{#each url_schemes}}
            <!-- URL scheme deep link: {{ this }}:// -->
            <intent-filter>
                <action android:name="android.intent.action.VIEW" />
                <category android:name="android.intent.category.DEFAULT" />
                <category android:name="android.intent.category.BROWSABLE" />
                <data android:scheme="{{ this }}" />
            </intent-filter>
            {{/each}}

            {{#each app_link_hosts}}
            <!-- App link: https://{{ this }} -->
            <intent-filter android:autoVerify="true">
                <action android:name="android.intent.action.VIEW" />
                <category android:name="android.intent.category.DEFAULT" />
                <category android:name="android.intent.category.BROWSABLE" />
                <data android:scheme="https" android:host="{{ this }}" />
            </intent-filter>
            {{/each}}
        </activity>

        {{#if foreground_service_type}}
        <service
            android:name=".DioxusForegroundService"
            android:foregroundServiceType="{{ foreground_service_type }}"
            android:exported="false" />
        {{/if}}
    </application>
</manifest>
```

变量替换说明：
- `{{ lib_name }}` → NativeActivity 库名，默认 `"main"`（对应 `libmain.so`）
- `{{#each permissions}}` → 展开为每个权限一行 `<uses-permission>`
- `{{#each features}}` → 展开为每个特性一行 `<uses-feature>`
- `{{#each url_schemes}}` / `{{#each app_link_hosts}}` → 深度链接 intent-filter
- `supports_rtl`/`app_theme`/`uses_cleartext_traffic`/`large_heap` → application 属性，不配则用默认值

### 三个权限配置节的区别（核心易混淆点）

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

### [android.signing]

| 字段           | 类型      | 说明                   |
| -------------- | --------- | ---------------------- |
| `jks_file`     | `PathBuf` | Java keystore 文件路径 |
| `jks_password` | `String`  | Keystore 密码          |
| `key_alias`    | `String`  | 密钥别名               |
| `key_password` | `String`  | 密钥密码               |

### [android.permissions]

`HashMap<String, RawPermission>`，键为完整 Android 权限字符串：

```toml
[android.permissions]
"android.permission.FOREGROUND_SERVICE" = { description = "后台服务" }
"android.permission.MANAGE_EXTERNAL_STORAGE" = { description = "文件管理" }
```

### [android.application]

| 字段                     | 类型             | 说明               |
| ------------------------ | ---------------- | ------------------ |
| `uses_cleartext_traffic` | `Option<bool>`   | 允许 HTTP 明文流量 |
| `theme`                  | `Option<String>` | 应用主题           |
| `supports_rtl`           | `Option<bool>`   | RTL 布局支持       |
| `large_heap`             | `Option<bool>`   | 大堆内存           |

### [android.raw]

| 字段                | 生效 | 说明                                                                 |
| ------------------- | ---- | -------------------------------------------------------------------- |
| `manifest`          | ✅   | 注入 `<manifest>` 元素内、`<application>` **之前**的原始 XML          |
| ~~`application_attrs`~~ | ❌ | config 有字段但 hbs 模板**无对应注入点**，不生效                      |
| ~~`application`~~   | ❌   | config 有字段但 hbs 模板**无对应注入点**，不生效                      |

**⚠️ `raw.manifest` 的注入点在 `<application>` 标签之前**（hbs 模板 L17 `{{{ raw_manifest }}}`）。
这意味着：放 `<uses-permission>` 等顶层元素合法；但放 `<activity>`/`<activity-alias>` 等
application 子元素**非法**（在 application 外面），会被 gradle merger 丢弃。

**如需在 `<application>` 内注入内容**（如 activity、activity-alias、service），`raw.application`
理论上应该用，但 hbs 模板没有渲染它——这是 dioxus CLI 的 bug/遗漏。变通方案见下方"自定义 intent-filter"。

```toml
[android.raw]
manifest = """
<uses-permission android:name="android.permission.NFC" />
"""
```

### ~~[[android.intent_filters]]~~ （⚠️ 不生效）

config struct 有此字段（`manifest.rs`），TOML 能正确解析，但 hbs 模板**没有对应的渲染点**，
配置被静默丢弃。`ManifestMapper` 会把它存进 `android_intent_filters`，但 `AndroidManifest.xml.hbs`
模板从不读取这个变量。

| 字段          | 类型                     | 说明               |
| ------------- | ------------------------ | ------------------ |
| `actions`     | `Vec<String>`            | Intent action      |
| `categories`  | `Vec<String>`            | Intent category    |
| `data`        | `Vec<AndroidIntentData>` | 数据规格           |
| `auto_verify` | `bool`                   | App Links 自动验证 |

**自定义 intent-filter 的变通方案**：用 `[application] android_manifest` 整体替换 manifest
（见上方"AndroidManifest.xml 生成逻辑"），在自定义 manifest 中手写 intent-filter。
注意整体替换会跳过 hbs 模板，需自行写全 permissions、NativeActivity meta-data 等。

### [android.queries]

| 字段       | 类型               | 说明            |
| ---------- | ------------------ | --------------- |
| `packages` | `Vec<String>`      | 可查询的包名    |
| `intents`  | `Vec<QueryIntent>` | 可查询的 Intent |

### Android 配置生效状态总览

基于 CLI 源码（`android.rs` + `AndroidManifest.xml.hbs` + `build.gradle.kts.hbs`）验证：

**✅ 走 hbs 模板（生效）**

| 配置 | hbs 变量 | 填入 |
|------|---------|------|
| `[permissions]` / `[android.permissions]` | `permissions` | manifest `<uses-permission>` |
| `[android.features]` | `features` | manifest `<uses-feature>` |
| `[android] min_sdk/target_sdk/compile_sdk` | `min_sdk/target_sdk/compile_sdk` | build.gradle.kts |
| `[android] gradle_dependencies` | `gradle_dependencies` | build.gradle.kts |
| `[android] gradle_plugins` | `gradle_plugins` | build.gradle.kts |
| `[android.application] uses_cleartext_traffic/theme/supports_rtl/large_heap` | 对应变量 | manifest `<application>` 属性 |
| `[android.raw.manifest]` | `raw_manifest` | manifest 顶层（application **之前**） |
| `[bundle] identifier` | `application_id` | build.gradle.kts namespace |
| `[bundle.android]` (jks 等) | `android_bundle` | build.gradle.kts signingConfigs |
| `[[deep_links]] schemes/hosts` | `url_schemes`/`app_link_hosts` | manifest `<intent-filter>` VIEW |

**✅ 不走模板，直接替换（生效）**

| 配置 | 机制 |
|------|------|
| `[application] android_manifest` | **整体替换** manifest（直接读文件内容，跳过 hbs） |
| `[application] android_main_activity` | **整体替换** MainActivity.kt |
| `[bundle.android] jks_file` 等 | gradle 签名配置 |

**❌ config 有字段但不生效（死字段）**

| 配置 | 原因 |
|------|------|
| `[android] manifest` | CLI 代码从不读取此字段 |
| `[[android.intent_filters]]` | hbs 模板无对应渲染点 |
| `[android.raw.application]` | hbs 模板无对应注入点 |
| `[android.raw.application_attrs]` | hbs 模板无对应注入点 |

## [macos] — macOS 特定配置

### Bundle 覆盖 + 专有字段

| 字段                     | 类型                        | 说明                                                  |
| ------------------------ | --------------------------- | ----------------------------------------------------- |
| `minimum_system_version` | `Option<String>`            | 最低 macOS 版本，如 `"11.0"`                          |
| `info_plist`             | `Option<PathBuf>`           | 自定义 Info.plist                                     |
| `signing_identity`       | `Option<String>`            | 签名身份，如 `"Developer ID Application: My Company"` |
| `provider_short_name`    | `Option<String>`            | 公证提供者短名                                        |
| `entitlements_file`      | `Option<String>`            | 自定义 entitlements 文件                              |
| `exception_domain`       | `Option<String>`            | ATS 例外域名                                          |
| `license`                | `Option<String>`            | DMG 许可文件                                          |
| `hardened_runtime`       | `Option<bool>`              | 加固运行时（ad-hoc 签名时可关）                       |
| `bundle_version`         | `Option<String>`            | CFBundleVersion                                       |
| `bundle_name`            | `Option<String>`            | CFBundleShortVersionString                            |
| `frameworks`             | `Vec<String>`               | 嵌入的框架                                            |
| `url_schemes`            | `Vec<String>`               | 额外 URL scheme                                       |
| `category`               | `Option<String>`            | Mac App Store 分类                                    |
| `files`                  | `HashMap<PathBuf, PathBuf>` | 额外文件（Contents 目录映射）                         |

### [macos.entitlements]

| 字段                               | TOML 键名                          | 说明                 |
| ---------------------------------- | ---------------------------------- | -------------------- |
| `app_sandbox`                      | `app-sandbox`                      | App Sandbox          |
| `files_user_selected`              | `files-user-selected`              | 用户选择文件读写     |
| `files_user_selected_readonly`     | `files-user-selected-readonly`     | 用户选择文件只读     |
| `network_client`                   | `network-client`                   | 出站网络             |
| `network_server`                   | `network-server`                   | 入站网络             |
| `camera`                           | -                                  | 摄像头               |
| `microphone`                       | -                                  | 麦克风               |
| `usb`                              | -                                  | USB                  |
| `bluetooth`                        | -                                  | 蓝牙                 |
| `print`                            | -                                  | 打印                 |
| `location`                         | -                                  | 定位                 |
| `addressbook`                      | -                                  | 通讯录               |
| `calendars`                        | -                                  | 日历                 |
| `disable_library_validation`       | `disable-library-validation`       | 禁用库验证           |
| `allow_jit`                        | `allow-jit`                        | 允许 JIT             |
| `allow_unsigned_executable_memory` | `allow-unsigned-executable-memory` | 允许无签名可执行内存 |
| `additional`                       | -                                  | 额外 entitlements    |

### [macos.plist] / [macos.raw]

同 iOS 格式。

## [windows] — Windows 特定配置

### Bundle 覆盖 + 专有字段

| 字段                      | 类型              | 说明              |
| ------------------------- | ----------------- | ----------------- |
| `digest_algorithm`        | `Option<String>`  | 代码签名摘要算法  |
| `certificate_thumbprint`  | `Option<String>`  | 证书指纹          |
| `timestamp_url`           | `Option<String>`  | 时间戳服务器 URL  |
| `tsp`                     | `Option<bool>`    | 使用 RFC 3161 TSP |
| `icon_path`               | `Option<PathBuf>` | 自定义图标路径    |
| `allow_downgrades`        | `Option<bool>`    | 允许降级安装      |
| `capabilities`            | `Vec<String>`     | UWP/MSIX 能力     |
| `restricted_capabilities` | `Vec<String>`     | 受限能力          |
| `device_capabilities`     | `Vec<String>`     | 设备能力          |

### [windows.wix]

| 字段                          | 类型                             | 说明                               |
| ----------------------------- | -------------------------------- | ---------------------------------- |
| `language`                    | `Vec<(String, Option<PathBuf>)>` | 语言及 locale 路径                 |
| `template`                    | `Option<PathBuf>`                | WiX 模板                           |
| `fragment_paths`              | `Vec<PathBuf>`                   | Fragment 文件                      |
| `license`                     | `Option<PathBuf>`                | 许可文件                           |
| `upgrade_code`                | `Option<Uuid>`                   | MSI 升级码 GUID（跨版本必须不变）  |
| `version`                     | `Option<String>`                 | MSI 版本 `major.minor.patch.build` |
| `skip_webview_install`        | `Option<bool>`                   | 跳过 WebView2 安装                 |
| `enable_elevated_update_task` | `Option<bool>`                   | 提权更新任务                       |
| `banner_path`                 | `Option<PathBuf>`                | 横幅图                             |
| `dialog_image_path`           | `Option<PathBuf>`                | 对话框图                           |
| `fips_compliant`              | `Option<bool>`                   | FIPS 合规                          |

### [windows.nsis]

| 字段                        | 类型                  | 说明                                    |
| --------------------------- | --------------------- | --------------------------------------- |
| `template`                  | `Option<PathBuf>`     | NSIS 模板                               |
| `license`                   | `Option<PathBuf>`     | 许可文件                                |
| `header_image`              | `Option<PathBuf>`     | 头部图                                  |
| `sidebar_image`             | `Option<PathBuf>`     | 侧边图                                  |
| `installer_icon`            | `Option<PathBuf>`     | 安装器图标                              |
| `install_mode`              | `Option<String>`      | `"CurrentUser"`/`"PerMachine"`/`"Both"` |
| `languages`                 | `Option<Vec<String>>` | 语言列表                                |
| `display_language_selector` | `Option<bool>`        | 显示语言选择器                          |
| `start_menu_folder`         | `Option<String>`      | 开始菜单文件夹名                        |
| `minimum_webview2_version`  | `Option<String>`      | 最低 WebView2 版本                      |

### [windows.sign_command]

| 字段   | 类型          | 说明                          |
| ------ | ------------- | ----------------------------- |
| `cmd`  | `String`      | 签名命令                      |
| `args` | `Vec<String>` | 参数，`"%1"` 为二进制路径占位 |

## [linux] — Linux 特定配置

### Bundle 覆盖 + 专有字段

| 字段                  | 类型          | 说明           |
| --------------------- | ------------- | -------------- |
| `categories`          | `Vec<String>` | Desktop 分类   |
| `keywords`            | `Vec<String>` | Desktop 关键词 |
| `mime_types`          | `Vec<String>` | MIME 类型      |
| `flatpak_permissions` | `Vec<String>` | Flatpak 权限   |
| `dbus_access`         | `Vec<String>` | D-Bus 访问     |

### [linux.deb]

| 字段                  | 类型                        | 说明         |
| --------------------- | --------------------------- | ------------ |
| `depends`             | `Option<Vec<String>>`       | 依赖         |
| `recommends`          | `Option<Vec<String>>`       | 推荐         |
| `provides`            | `Option<Vec<String>>`       | 提供         |
| `conflicts`           | `Option<Vec<String>>`       | 冲突         |
| `replaces`            | `Option<Vec<String>>`       | 替换         |
| `files`               | `HashMap<PathBuf, PathBuf>` | 额外文件映射 |
| `desktop_template`    | `Option<PathBuf>`           | Desktop 模板 |
| `section`             | `Option<String>`            | Debian 分区  |
| `priority`            | `Option<String>`            | 优先级       |
| `changelog`           | `Option<PathBuf>`           | 变更日志     |
| `pre_install_script`  | `Option<PathBuf>`           | 预安装脚本   |
| `post_install_script` | `Option<PathBuf>`           | 后安装脚本   |
| `pre_remove_script`   | `Option<PathBuf>`           | 预卸载脚本   |
| `post_remove_script`  | `Option<PathBuf>`           | 后卸载脚本   |

## [components] — 组件配置

| 字段             | 类型                | 说明           |
| ---------------- | ------------------- | -------------- |
| `registry`       | `ComponentRegistry` | 默认组件注册表 |
| `components_dir` | `Option<PathBuf>`   | 组件存储目录   |

## 内联配置

可在 Rust 源文件模块级文档注释中嵌入 Dioxus.toml 配置，与主配置深度合并（内联优先）：

````rust
//! 示例应用
//!
//! ```dioxus.toml
//! [bundle]
//! identifier = "com.example.test"
//!
//! [permissions]
//! camera = { description = "拍照" }
//! ```
````

## 完整配置示例

```toml
[application]
name = "My App"
public_dir = "public"

[web.app]
title = "My App"
base_path = "/app"

[web.resource]
style = ["styles/global.css"]
script = []

[web.resource.dev]
style = []
script = []

[web.https]
enabled = true
mkcert = true

[bundle]
identifier = "com.example.myapp"
publisher = "Example Inc"
icon = ["assets/icon.png"]
version = "1.0.0"
copyright = "Copyright 2025 Example Inc"
category = "Utility"
short_description = "A cross-platform app"

[permissions]
location = { precision = "fine", description = "追踪您的位置" }
camera = { description = "拍照" }
notifications = { description = "发送通知" }

[deep_links]
schemes = ["myapp"]
hosts = ["example.com"]
paths = ["/app/*"]

[background]
audio = true
fetch = true

[ios]
deployment_target = "15.0"
identifier = "com.example.myapp.ios"

[ios.entitlements]
app-groups = ["group.com.example.app"]
aps-environment = "development"

[android]
min_sdk = 24
target_sdk = 34
identifier = "com.example.myapp.android"

[android.signing]
jks_file = "keystore.jks"
jks_password = "password"
key_alias = "mykey"
key_password = "keypassword"

[macos]
minimum_system_version = "11.0"
signing_identity = "Developer ID Application: My Company"

[windows]
allow_downgrades = true

[windows.nsis]
install_mode = "CurrentUser"

[linux]
categories = ["Utility", "Network"]
```
