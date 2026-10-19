//! 类型安全持久化存储引擎。

use models::Persistable;
use serde::{Serialize, de::DeserializeOwned};

use crate::StorageBackend;

/// 类型安全持久化存储。
///
/// 封装 [`StorageBackend`]，提供 JSON 序列化。
/// 对该实例的所有操作都在后端的某个存储文件上执行。
///
/// ```ignore
/// pub type AndroidStorage = PersistStorage<AndroidBackend>;
///
/// let storage = PersistStorage::new(AndroidBackend::new()?);
/// storage.save(&theme)?;
/// let t = storage.load::<Theme>()?;
/// let name = storage.file_name();
/// ```
pub struct PersistStorage<B: StorageBackend> {
    backend: B,
}

impl<B: StorageBackend> PersistStorage<B>
where
    B::Error: From<serde_json::Error>,
{
    /// 使用指定后端创建存储实例。
    pub fn new(backend: B) -> Self {
        Self { backend }
    }

    /// 存储文件名，委托给后端。
    pub fn file_name(&self) -> Option<&str> {
        self.backend.file_name()
    }

    /// 加载类型 `T` 对应的值，键由 `T::KEY` 确定。
    ///
    /// 键不存在时返回 `Ok(None)`。
    pub fn load<T: Persistable + DeserializeOwned>(&self) -> Result<Option<T>, B::Error> {
        match self.backend.load_raw(T::KEY)? {
            None => Ok(None),
            Some(json) => Ok(Some(serde_json::from_str(&json)?)),
        }
    }

    /// 以 `T::KEY` 为键保存值，JSON 序列化后写入后端。
    pub fn save<T: Persistable + Serialize>(&self, value: &T) -> Result<(), B::Error> {
        let json = serde_json::to_string(value)?;
        self.backend.save_raw(T::KEY, &json)
    }

    /// 删除 `T::KEY` 对应的值。
    pub fn remove<T: Persistable>(&self) -> Result<(), B::Error> {
        self.backend.remove_raw(T::KEY)
    }
}
