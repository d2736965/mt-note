# mt-note English translation.
#
# The key set must stay identical across all locales (enforced by
# mobile/build.rs at build time). Conventions:
# - File names are full BCP-47 tags: zh-Hans is Simplified Chinese;
#   Traditional Chinese will be zh-Hant (never bare "zh")
# - {$name} is a Fluent interpolation placeholder; move it to fit the word
#   order of the translation, but never drop it
# - datetime-* values are format patterns parsed by the in-app date engine,
#   numeric specifiers only (yyyy/MM/dd/HH/mm/ss); month names unsupported
# - language-self-name is the language endonym: always written in that
#   language's own script, never translated

language-self-name = English

# Permission gate
perm-checking = Checking storage permission…
perm-title = All-files access required
perm-desc = This app needs "All files access" to browse and edit your note files.
perm-hint = Grant access in system settings, then come back and tap "Re-check" to continue.
perm-open-settings = Open system settings
perm-recheck = Re-check

# Directory picker
dirpicker-title = Choose a directory
dirpicker-no-subdirs = No subdirectories
dirpicker-current = Current:
dirpicker-select = Select this directory

# File changed dialog
file-changed-title = File modified externally
file-changed-body = "{$name}" has been changed by another app. Reload it?
file-changed-reload = Reload

# Settings
settings-title = Settings
settings-section-general = General
settings-startup-page = Startup page
settings-startup-page-desc = The home tab shown when the app starts
settings-startup-files = Files
settings-startup-favorites = Favorites
settings-default-dir = Default directory
settings-default-dir-desc = Directory shown in the file browser at startup; leave empty for the system default
settings-default-dir-placeholder = e.g. /sdcard/Documents
settings-language = Language
settings-language-desc = Display language of the interface
settings-follow-system = Follow system

# Date and time formats
datetime-full = yyyy-MM-dd HH:mm
datetime-date = yyyy-MM-dd
datetime-short = MM-dd HH:mm

# Common
common-app-state-error = Failed to load app state
common-rename = Rename
common-cancel = Cancel
common-confirm = OK
common-delete = Delete
common-back = ← Back
common-new = New
common-new-file = New file
common-new-dir = New folder
common-refresh = Refresh
common-more = More
common-show-hidden = Show hidden files
common-search = Search
common-advanced-search = Advanced search
common-settings = Settings
common-about = About
common-empty-dir = Empty folder
common-name = Name

# Home
home-tab-files = Files
home-tab-favorites = Favorites
home-tab-more = More
home-favorites = Favorites
home-no-favorites = No favorites yet
home-no-favorites-hint = Long-press a folder to add it to favorites

# Context menu
ctx-add-favorite = Add to favorites
ctx-remove-favorite = Remove from favorites
ctx-copy-path = Copy path
ctx-copied-full = Full path copied
ctx-full-path = Full path
ctx-copied-name = Name copied
ctx-dir-name = Folder name
ctx-copied-dir-path = Folder path copied
ctx-dir = Folder
ctx-file-name = File name

# Advanced search
search-query-placeholder = Enter search keywords...
search-history = Search history...
search-regex = Regular expression
search-case-sensitive = Case sensitive
search-content = Search file contents
search-only-first = First match only
search-in-progress = Searching...
search-filter-placeholder = Filter results...
search-type-dir = Folders
search-type-file = Files
search-result-count = {$done} / {$total} results
search-empty-hint = Enter keywords and tap Search
search-no-match = No matches
search-browse = Browse

# Error wrappers
err-delete = Delete failed: {$e}
err-rename = Rename failed: {$e}
err-create = Failed to create: {$e}
err-name-empty = Name cannot be empty
err-browse = Browse failed: {$e}
err-search = Search failed: {$e}

# Common (second set)
common-rendering = Rendering...
common-loading = Loading...
common-save = Save
common-close = Close
common-unnamed = Untitled
common-unknown-error = Unknown error
common-return = Back
common-home = Home
common-license = License

# About page
about-desc = A lightweight Markdown / Typst note-taking app

# Editor error page
err-editor-title = Editor rendering error
err-editor-desc = The editor encountered an unrecoverable error while rendering the file.
err-editor-detail = Error details

# Datetime insert dialog
dt-title = Date & Time
dt-date = Date
dt-time = Time
dt-use-current = Use current date & time
dt-preview = Preview
dt-recent = Recent
dt-only-date = Date only
dt-only-time = Time only

# Editor
editor-saved = Saved
err-save = Save failed: {$e}
editor-zoom-in = Zoom in
editor-zoom-out = Zoom out
editor-fit-content = Fit content
editor-fit-paper = Fit paper
editor-zoom = Zoom
editor-hide-text = Hide text layer
editor-show-text = Show text layer
editor-save-as = Save as
editor-save-as-ext = Save as {$ext}
editor-exporting-pdf = Exporting PDF...
editor-exported = Exported: {$path}
err-write-file = Failed to write file: {$e}
err-export = Export failed: {$e}
err-reload = Reload failed: {$e}
editor-references = References ({$n})
editor-rename-label = Rename:
editor-default-filename = Untitled.{$extension}
typst-compiling = Compiling...
err-typst-compile = Compilation failed: {$e}

# Preview
preview-unsupported = Cannot preview this file type

# Settings (second set)
settings-section-appearance = Appearance
settings-theme-mode = Theme mode
settings-theme-mode-desc = Choose the app's theme appearance
settings-theme-light = Light
settings-theme-dark = Dark
settings-section-editor = Editor
settings-wrap-mode = Word wrap
settings-wrap-mode-desc = Wrap long lines and show continuation marks
settings-wrap-off = No wrap
settings-wrap-on = Wrap
settings-show-line-numbers = Show line numbers
settings-show-line-numbers-desc = Show line numbers on the left of the editor
settings-wrap-column = Wrap column
settings-wrap-column-desc = Maximum characters per line before wrapping
settings-section-typst = Typst compilation
settings-font-paths = Font search paths
settings-font-paths-desc = Directories searched for fonts during Typst compilation
settings-font-path-placeholder = Enter a font directory path

# Error variant translations (i18n/localize.rs; underlying Displays are English)
err-fs-walk = Failed to walk directory {path}: {source}
err-fs-io = IO error at {path}: {source}
err-fs-not-found = Path not found: {path}
err-fs-not-dir = Not a directory: {path}
err-fs-already-exists = Path already exists: {path}
err-fs-invalid-input = Invalid input: {msg}
err-typst-compile-failed = Typst compilation failed: {msg}
err-typst-file-not-found = File not found: {path}
err-typst-io = IO error at {path}: {source}
err-typst-font-load = Failed to load fonts: {msg}

# Markdown rendering toolbar (TS side, @fluent/bundle)
md-copy = Copy
md-copied = Copied
md-code = Code
md-table = Table

# Image lightbox toolbar (TS side, @fluent/bundle)
lightbox-toolbar = Image preview toolbar
lightbox-fit-width = Fit width
lightbox-zoom-100 = Zoom 100% (actual size)
lightbox-center-pin = Center & pin
lightbox-unpin = Unpin
lightbox-close = Close

# Native platform resources (native- prefixed keys are generated into
# android/res/values*/strings-gen.xml by build.rs; the prefix only marks
# native-resource membership, the keys are shared across all platforms)
native-open-with = Open with
