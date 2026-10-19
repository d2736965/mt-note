//! 平台存储后端 trait。
//!
//! 平台只需实现 [`StorageBackend`]，[`PersistStorage`](crate::PersistStorage)
//! 在此基础上自动提供 JSON 序列化和类型安全接口。

/// 平台存储后端，实现 raw k:v 操作。
///
/// 后端结构体有状态（如持有文件名），平台只需实现此 trait，
/// [`super::PersistStorage`] 在此基础上自动提供 JSON 序列化和类型安全接口。
///
/// ```ignore
/// impl StorageBackend for AndroidBackend {
///     type Error = AndroidStorageError;
///
///     fn file_name(&self) -> Option<&str> {
///         Some(&self.file_name)
///     }
///     fn load_raw(&self, key: &str) -> Result<Option<String>, Self::Error> {
///         jni_get_string(key)
///     }
///     fn save_raw(&self, key: &str, value: &str) -> Result<(), Self::Error> {
///         jni_set_string(key, value)
///     }
///     fn remove_raw(&self, key: &str) -> Result<(), Self::Error> {
///         jni_remove_string(key)
///     }
/// }
/// ```
pub trait StorageBackend {
    type Error: std::error::Error;

    /// 存储文件名，Web 的 localStorage 为 `None`。
    fn file_name(&self) -> Option<&str>;

    /// 读 key 对应的 raw 字符串。
    fn load_raw(&self, key: &str) -> Result<Option<String>, Self::Error>;
    /// 写 key → value 字符串。
    fn save_raw(&self, key: &str, value: &str) -> Result<(), Self::Error>;
    /// 删除 key。
    fn remove_raw(&self, key: &str) -> Result<(), Self::Error>;
}
