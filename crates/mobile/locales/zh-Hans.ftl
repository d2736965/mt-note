# mt-note 简体中文翻译。
#
# 键集必须与 en.ftl 完全一致（mobile/build.rs 构建期强制）。
# 翻译约定：
# - 文件名用 BCP-47 完整标签：zh-Hans 是简体中文，繁体中文将是 zh-Hant，
#   不能只写 zh（zh 不止简体一种）
# - {$name} 是 Fluent 插值占位符，译文可按语序移动位置，但不可删除
# - datetime-* 的值是应用内日期引擎的格式模式，仅支持数字占位符
#   （yyyy/MM/dd/HH/mm/ss），不支持月份名称
# - language-self-name 是语言自名，固定用该语言原文书写，不翻译

language-self-name = 简体中文

# 权限门
perm-checking = 正在检查存储权限…
perm-title = 需要所有文件访问权限
perm-desc = 应用需要「所有文件访问」权限来浏览和编辑你的笔记文件。
perm-hint = 在系统设置中授权后，回到这里点击「重新检查」继续。
perm-open-settings = 前往系统设置
perm-recheck = 重新检查

# 目录选择器
dirpicker-title = 选择目录
dirpicker-no-subdirs = 没有子目录
dirpicker-current = 当前：
dirpicker-select = 选择此目录

# 文件变更提示
file-changed-title = 文件已在外部修改
file-changed-body = 「{$name}」已被其他应用修改，是否重新加载？
file-changed-reload = 重新加载

# 设置页
settings-title = 设置
settings-section-general = 通用
settings-startup-page = 启动页面
settings-startup-page-desc = 应用启动时进入的首页标签页
settings-startup-files = 文件
settings-startup-favorites = 收藏
settings-default-dir = 默认打开目录
settings-default-dir-desc = 应用启动时文件浏览器显示的目录，留空使用系统默认
settings-default-dir-placeholder = 如 /sdcard/Documents
settings-language = 语言
settings-language-desc = 界面显示语言
settings-follow-system = 跟随系统

# 日期时间格式
datetime-full = yyyy年MM月dd日 HH:mm
datetime-date = yyyy年MM月dd日
datetime-short = MM月dd日 HH:mm


# 通用
common-app-state-error = 应用状态加载失败
common-rename = 重命名
common-cancel = 取消
common-confirm = 确定
common-delete = 删除
common-back = ← 返回
common-new = 新建
common-new-file = 新建文件
common-new-dir = 新建目录
common-refresh = 刷新
common-more = 更多
common-show-hidden = 显示隐藏文件
common-search = 搜索
common-advanced-search = 高级搜索
common-settings = 设置
common-about = 关于
common-empty-dir = 空目录
common-name = 名称

# 首页
home-tab-files = 文件
home-tab-favorites = 收藏
home-tab-more = 更多
home-favorites = 收藏夹
home-no-favorites = 暂无收藏
home-no-favorites-hint = 长按目录可添加到收藏夹

# 长按菜单
ctx-add-favorite = 加入收藏夹
ctx-remove-favorite = 移出收藏夹
ctx-copy-path = 复制路径
ctx-copied-full = 已复制完整路径
ctx-full-path = 完整路径
ctx-copied-name = 已复制名称
ctx-dir-name = 目录名
ctx-copied-dir-path = 已复制目录路径
ctx-dir = 目录
ctx-file-name = 文件名

# 高级搜索
search-query-placeholder = 输入搜索关键词...
search-history = 搜索历史...
search-regex = 正则表达式
search-case-sensitive = 大小写敏感
search-content = 搜索文件内容
search-only-first = 仅首个匹配
search-in-progress = 搜索中...
search-filter-placeholder = 过滤结果...
search-type-dir = 目录
search-type-file = 文件
search-result-count = {$done} / {$total} 条结果
search-empty-hint = 输入关键词后点击搜索
search-no-match = 无匹配结果
search-browse = 浏览

# 错误包装
err-delete = 删除失败: {$e}
err-rename = 重命名失败: {$e}
err-create = 创建失败: {$e}
err-name-empty = 名称不能为空
err-browse = 浏览失败: {$e}
err-search = 搜索失败: {$e}

# 通用（第二组）
common-rendering = 渲染中...
common-loading = 加载中...
common-save = 保存
common-close = 关闭
common-unnamed = 未命名
common-unknown-error = 未知错误
common-return = 返回
common-home = 首页
common-license = 许可证

# 关于页
about-desc = 轻量级 Markdown / Typst 笔记应用

# 编辑器错误页
err-editor-title = 编辑器渲染错误
err-editor-desc = 编辑器在渲染文件内容时遇到了无法恢复的错误。
err-editor-detail = 错误详情

# 日期时间插入对话框
dt-title = 日期时间
dt-date = 日期
dt-time = 时间
dt-use-current = 使用当前日期时间
dt-preview = 预览
dt-recent = 最近使用
dt-only-date = 仅日期
dt-only-time = 仅时间

# 编辑器
editor-saved = 已保存
err-save = 保存失败: {$e}
editor-zoom-in = 放大
editor-zoom-out = 缩小
editor-fit-content = 内容适应
editor-fit-paper = 纸张适应
editor-zoom = 缩放
editor-hide-text = 隐藏文字层
editor-show-text = 显示文字层
editor-save-as = 另存为
editor-save-as-ext = 另存为 {$ext}
editor-exporting-pdf = 导出 PDF 中...
editor-exported = 已导出: {$path}
err-write-file = 写文件失败: {$e}
err-export = 导出失败: {$e}
err-reload = 重新加载失败: {$e}
editor-references = 引用 ({$n})
editor-rename-label = 重命名:
editor-default-filename = 未命名.{$extension}
typst-compiling = 编译中...
err-typst-compile = 编译失败: {$e}

# 预览
preview-unsupported = 不支持预览此文件类型

# 设置页（第二组）
settings-section-appearance = 外观
settings-theme-mode = 主题模式
settings-theme-mode-desc = 选择应用的主题外观
settings-theme-light = 浅色
settings-theme-dark = 暗色
settings-section-editor = 编辑器
settings-wrap-mode = 折行模式
settings-wrap-mode-desc = 折行时自动换行并显示续行符
settings-wrap-off = 不折行
settings-wrap-on = 折行
settings-show-line-numbers = 显示行号
settings-show-line-numbers-desc = 在编辑器左侧显示行号
settings-wrap-column = 折行列数
settings-wrap-column-desc = 每行显示的最大字符数，超出后折行显示
settings-section-typst = Typst 编译
settings-font-paths = 字体搜索路径
settings-font-paths-desc = Typst 编译时搜索字体的目录
settings-font-path-placeholder = 输入字体目录路径

# 错误变体翻译（i18n/localize.rs，底层 Display 已英文化）
err-fs-walk = 目录遍历失败 {path}: {source}
err-fs-io = IO 错误 {path}: {source}
err-fs-not-found = 路径不存在: {path}
err-fs-not-dir = 路径不是目录: {path}
err-fs-already-exists = 路径已存在: {path}
err-fs-invalid-input = 无效输入: {msg}
err-typst-compile-failed = Typst 编译失败: {msg}
err-typst-file-not-found = 文件未找到: {path}
err-typst-io = IO 错误 {path}: {source}
err-typst-font-load = 字体加载失败: {msg}

# Markdown 渲染工具条（TS 侧 @fluent/bundle 取词）
md-copy = 复制
md-copied = 已复制
md-code = 代码
md-table = 表格

# 图片灯箱工具栏（TS 侧 @fluent/bundle 取词）
lightbox-toolbar = 图片预览工具栏
lightbox-fit-width = 适应宽度
lightbox-zoom-100 = 缩放 100%（原始尺寸）
lightbox-center-pin = 居中并钉住
lightbox-unpin = 解除钉住
lightbox-close = 关闭

# 原生平台资源（native- 前缀键由 build.rs 生成 android/res/values*/strings-gen.xml；
# 前缀仅标记进原生资源，键全平台共用）
native-open-with = 使用其他应用打开
