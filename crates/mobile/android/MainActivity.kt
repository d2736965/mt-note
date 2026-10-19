package dev.dioxus.main

import com.d2736965.app.mt_note_mobile.R

import android.content.Intent
import android.os.Bundle
import android.os.Environment
import android.provider.Settings
import android.provider.MediaStore
import android.provider.DocumentsContract
import android.provider.OpenableColumns
import android.content.ContentUris
import android.util.Log
import android.view.View
import android.graphics.Color
import androidx.annotation.Keep
import android.webkit.WebView
import android.widget.FrameLayout
import androidx.activity.OnBackPressedCallback
import androidx.core.view.WindowCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import java.io.File

typealias BuildConfig = com.d2736965.app.mt_note_mobile.BuildConfig

class MainActivity : WryActivity() {
    // WebView 引用，用于 JS 注入和返回键拦截
    private var mWebViewRef: RustWebView? = null
    // 当前系统栏是否为暗色主题（true=暗色背景白色图标, false=亮色背景黑色图标）
    private var mSystemBarDark = false

    // 读取并清除待处理的分享文件路径（由 Rust 侧通过 JNI 调用）
    @Keep
    fun getAndClearPendingSharedFile(): String? {
        val f = pendingSharedFile
        pendingSharedFile = null
        return f
    }

    // 读取系统语言标签（BCP-47，如 "zh-CN"），用于默认界面语言解析（由 Rust 侧通过 JNI 调用）
    @Keep
    fun getSystemLanguage(): String {
        return resources.configuration.locales[0].toLanguageTag()
    }

    override fun onWebViewCreate(webView: WebView) {
        mWebViewRef = webView as RustWebView

        // 允许 WebView 通过 file:// 协议访问本地文件（用于 Asset Handler 的 local-file 协议）
        webView.settings.allowFileAccess = true
        webView.settings.allowFileAccessFromFileURLs = true
        webView.settings.allowUniversalAccessFromFileURLs = true
        // 禁用 Android 原生 overscroll 发光效果（橡皮筋）。
        // 仅靠 CSS overscroll-behavior 不够——进入后台再回来时 WebView 状态可能重置，原生层会重新出现。
        webView.overScrollMode = View.OVER_SCROLL_NEVER

        Log.d(TAG, "onWebViewCreate: pendingSharedFile=$pendingSharedFile")
        // 冷启动时 handleIncomingIntent 在 WebView 创建前执行，
        // notifySharedFileToWebView 因 mWebViewRef 为 null 而跳过。
        // WebView 就绪后在此补发，确保事件不丢失。
        pendingSharedFile?.let { path ->
            Log.d(TAG, "onWebViewCreate: re-dispatching shared file: $path")
            notifySharedFileToWebView(path)
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        // 启用 WebView 远程调试（仅 debug 构建生效，release 自动禁用）
        WebView.setWebContentsDebuggingEnabled(true)
        // 沉浸式状态栏：内容延伸到状态栏区域
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        currentInstance = this
        // 监听软键盘高度变化，给内容区域设置底部 padding 避免键盘遮挡
        setupKeyboardResize()
        // 初始系统栏背景色，避免主题加载前透出默认窗口背景
        window.decorView.setBackgroundColor(Color.parseColor("#F3EDF7"))
        window.navigationBarColor = Color.parseColor("#F3EDF7")
        // 处理外部应用分享/打开文件
        handleIncomingIntent(intent)

        // 拦截返回键：通过 CustomEvent 传递给 Dioxus 前端处理（如关闭编辑器、抽屉等）
        onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
            override fun handleOnBackPressed() {
                val webView = mWebViewRef ?: run {
                    // WebView 未就绪，退化为系统默认行为
                    isEnabled = false
                    onBackPressedDispatcher.onBackPressed()
                    return
                }
                val js = "window.dispatchEvent(new CustomEvent('back-pressed'));"
                webView.evaluateJavascript(js) { _ -> }
            }
        })
    }

    private fun enableEdgeToEdge() {
        WindowCompat.setDecorFitsSystemWindows(window, false)
        window.decorView.systemUiVisibility =
            View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR or
            View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR
    }

    // 系统栏避让：给内容区域设置 padding 避免被状态栏/导航栏遮挡
    // 竖屏：状态栏在上、导航栏在下；横屏：导航栏在右（或左）
    private fun setupKeyboardResize() {
        val contentView = window.decorView.findViewById<FrameLayout>(android.R.id.content)
        ViewCompat.setOnApplyWindowInsetsListener(contentView) { view, insets ->
            val statusBar = insets.getInsets(WindowInsetsCompat.Type.statusBars())
            val navBar = insets.getInsets(WindowInsetsCompat.Type.navigationBars())
            val ime = insets.getInsets(WindowInsetsCompat.Type.ime())

            val topPadding = statusBar.top
            val bottomPadding = if (ime.bottom > 0) ime.bottom else navBar.bottom
            val leftPadding = if (ime.left > 0) ime.left else navBar.left
            val rightPadding = if (ime.right > 0) ime.right else navBar.right

            view.setPadding(leftPadding, topPadding, rightPadding, bottomPadding)
            // 设置 padding 后系统可能覆盖 systemUiVisibility，立即恢复
            if (mSystemBarDark) {
                window.decorView.systemUiVisibility = 0
            } else {
                window.decorView.systemUiVisibility =
                    View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR or
                    View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR
            }
            insets
        }
    }

    // 将 app 退到后台（由 Rust 侧通过 JNI 调用）
    @Keep
    fun moveTaskToBack() {
        runOnUiThread {
            moveTaskToBack(true)
        }
    }

    // 切换系统栏（状态栏+导航栏）主题：图标颜色 + 背景色，由前端传入颜色值
    @Keep
    fun setSystemBarTheme(isDark: Boolean, surfaceColor: String) {
        runOnUiThread {
            mSystemBarDark = isDark
            val color = try {
                Color.parseColor(surfaceColor)
            } catch (e: Exception) {
                Log.e(TAG, "setSystemBarTheme: invalid color '$surfaceColor'", e)
                return@runOnUiThread
            }
            if (isDark) {
                window.decorView.systemUiVisibility = 0
            } else {
                window.decorView.systemUiVisibility =
                    View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR or
                    View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR
            }
            window.decorView.setBackgroundColor(color)
            window.navigationBarColor = color
        }
    }

    // 检查是否已授予"所有文件访问"权限（Android 11+）
    @Keep
    fun isStorageManagerGranted(): Boolean {
        return Environment.isExternalStorageManager()
    }

    // 跳转到系统设置页面，让用户手动授予"所有文件访问"权限
    @Keep
    fun openAllFilesAccessSettings() {
        val intent = Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION).apply {
            data = android.net.Uri.parse("package:com.d2736965.app.mt_note_mobile")
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        }
        startActivity(intent)
    }

    // 用系统默认应用打开文件（如用其他 Markdown 编辑器打开 .md 文件）
    @Keep
    fun openWithExternalApp(path: String) {
        runOnUiThread {
            try {
                val file = File(path)
                val authority = "${packageName}.fileprovider"
                val uri = androidx.core.content.FileProvider.getUriForFile(this, authority, file)
                val intent = Intent(Intent.ACTION_VIEW).apply {
                    setDataAndType(uri, contentResolver.getType(uri) ?: "*/*")
                    addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                }
                startActivity(Intent.createChooser(intent, getString(R.string.open_with)))
            } catch (e: Exception) {
                Log.e(TAG, "openWithExternalApp failed: $path", e)
            }
        }
    }

    // 外部应用再次分享文件时（app 已在运行），通过此方法接收新 Intent
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        handleIncomingIntent(intent)
    }

    override fun onResume() {
        super.onResume()
        currentInstance = this
    }

    override fun onPause() {
        super.onPause()
        if (currentInstance === this) currentInstance = null
    }

    // 处理外部应用分享/打开文件的 Intent
    private fun handleIncomingIntent(intent: Intent?) {
        Log.d(TAG, "handleIncomingIntent: action=${intent?.action}, type=${intent?.type}, data=${intent?.data}")
        when (intent?.action) {
            Intent.ACTION_SEND -> {
                val uri = intent.getParcelableExtra<android.net.Uri>(Intent.EXTRA_STREAM)
                Log.d(TAG, "ACTION_SEND: uri=$uri")
                if (uri != null && intent.type != null) {
                    val path = resolveSharedFilePath(uri, intent.type!!)
                    Log.d(TAG, "ACTION_SEND: resolved path=$path")
                    if (path != null) {
                        pendingSharedFile = path
                        notifySharedFileToWebView(path)
                    }
                }
            }
            Intent.ACTION_VIEW -> {
                val uri = intent.data
                Log.d(TAG, "ACTION_VIEW: uri=$uri")
                if (uri != null) {
                    val mimeType = intent.type ?: resolveMimeType(uri)
                    val path = resolveSharedFilePath(uri, mimeType)
                    Log.d(TAG, "ACTION_VIEW: mimeType=$mimeType, resolved path=$path")
                    if (path != null) {
                        pendingSharedFile = path
                        notifySharedFileToWebView(path)
                    }
                }
            }
        }
    }

    private fun resolveMimeType(uri: android.net.Uri): String {
        return contentResolver.getType(uri) ?: "application/octet-stream"
    }

    // 将分享文件路径通知给 WebView 前端（通过 CustomEvent）
    private fun notifySharedFileToWebView(path: String) {
        Log.d(TAG, "notifySharedFileToWebView: path=$path, mWebViewRef=${mWebViewRef != null}")
        runOnUiThread {
            try {
                val webView = mWebViewRef ?: run {
                    Log.w(TAG, "notifySharedFileToWebView: mWebViewRef is null, skipping")
                    return@runOnUiThread
                }
                val escaped = path.replace("\\", "\\\\").replace("'", "\\'")
                val js = "window.dispatchEvent(new CustomEvent('shared-file',{detail:{path:'$escaped'}}));"
                Log.d(TAG, "notifySharedFileToWebView: evaluating JS")
                webView.evaluateJavascript(js) { result ->
                    Log.d(TAG, "notifySharedFileToWebView: JS result=$result")
                }
            } catch (e: Exception) {
                Log.e(TAG, "notifySharedFileToWebView failed", e)
            }
        }
    }

    // 解析分享文件的真实路径：优先通过 ContentProvider 获取 _data 列，再尝试 DocumentsContract
    private fun resolveSharedFilePath(uri: android.net.Uri, mimeType: String): String? {
        resolveRealPath(uri)?.let { return it }
        return copySharedFileToInternal(uri, mimeType)
    }

    // 通过 ContentProvider 查询真实文件路径
    private fun resolveRealPath(uri: android.net.Uri): String? {
        if (uri.scheme == "content") {
            try {
                val projection = arrayOf(MediaStore.MediaColumns.DATA)
                contentResolver.query(uri, projection, null, null, null)?.use { cursor ->
                    if (cursor.moveToFirst()) {
                        val colIndex = cursor.getColumnIndex(MediaStore.MediaColumns.DATA)
                        if (colIndex >= 0) {
                            val path = cursor.getString(colIndex)
                            if (!path.isNullOrEmpty() && File(path).exists()) {
                                return path
                            }
                        }
                    }
                }
            } catch (_: Exception) {
                // Some content providers don't support _data column
            }
        }

        if (DocumentsContract.isDocumentUri(this, uri)) {
            if ("com.android.externalstorage.documents" == uri.authority) {
                val docId = DocumentsContract.getDocumentId(uri)
                val split = docId.split(":")
                if (split.size >= 2 && "primary" == split[0]) {
                    val path = Environment.getExternalStorageDirectory().toString() + "/" + split[1]
                    if (File(path).exists()) {
                        return path
                    }
                }
            }
            if ("com.android.providers.downloads.documents" == uri.authority) {
                val docId = DocumentsContract.getDocumentId(uri)
                try {
                    val contentUri = ContentUris.withAppendedId(
                        MediaStore.Downloads.EXTERNAL_CONTENT_URI,
                        docId.toLong()
                    )
                    return resolveRealPath(contentUri)
                } catch (_: NumberFormatException) {
                    // ignore
                }
            }
        }

        if (uri.scheme == "file") {
            return uri.path
        }

        return null
    }

    // ContentProvider 无法直接获取路径时，将文件复制到内部存储
    private fun copySharedFileToInternal(uri: android.net.Uri, mimeType: String): String? {
        return try {
            val inputStream = contentResolver.openInputStream(uri) ?: return null
            val fileName = resolveFileName(uri, mimeType)
            val destDir = File(filesDir, "shared")
            if (!destDir.exists()) destDir.mkdirs()
            destDir.listFiles()?.forEach { it.delete() }
            val destFile = File(destDir, fileName)
            destFile.outputStream().use { out -> inputStream.copyTo(out) }
            inputStream.close()
            destFile.absolutePath
        } catch (e: Exception) {
            Log.e("MainActivity", "copySharedFile failed", e)
            null
        }
    }

    // 从 ContentProvider 查询文件名，或根据 MIME 类型推断默认文件名
    private fun resolveFileName(uri: android.net.Uri, mimeType: String): String {
        contentResolver.query(uri, null, null, null, null)?.use { cursor ->
            val nameIndex = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
            if (nameIndex >= 0 && cursor.moveToFirst()) {
                return cursor.getString(nameIndex)
            }
        }
        uri.lastPathSegment?.let { return it }
        val ext = when {
            mimeType.contains("typst") -> "typ"
            mimeType.contains("markdown") -> "md"
            mimeType.contains("pdf") -> "pdf"
            mimeType.contains("svg") -> "svg"
            else -> "txt"
        }
        return "shared_file.$ext"
    }

    // 静态成员：供 Rust 侧通过 JNI 调用，以及跨 Activity 生命周期共享状态
    companion object {
        private const val TAG = "MainActivity"

        @Volatile
        private var currentInstance: MainActivity? = null

        @Volatile
        private var pendingSharedFile: String? = null

        @JvmStatic
        fun getCurrentInstance(): MainActivity? = currentInstance
    }
}