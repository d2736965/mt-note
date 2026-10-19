//! 路径展示文本：为 UI 菜单提供人类可读的路径/名称文本。

use std::path::Path;

/// 条目展示名：目录/文件的末级名称；无法提取（如根路径）时回退完整路径。
pub fn entry_display_name(target: &Path) -> String {
    target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| target.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::entry_display_name;

    #[test]
    fn entry_name_of_file_and_dir() {
        assert_eq!(entry_display_name(Path::new("/a/b/c.md")), "c.md");
        assert_eq!(entry_display_name(Path::new("/a/b")), "b");
    }

    #[test]
    fn entry_name_root_falls_back_to_full() {
        assert_eq!(entry_display_name(Path::new("/")), "/");
    }

    #[test]
    fn entry_name_trailing_slash_resolved() {
        // PathBuf 归一化后尾斜杠不影响 file_name 提取
        assert_eq!(entry_display_name(Path::new("/a/b/")), "b");
    }
}
