use persist::{PersistStorage, StorageBackend};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(thiserror::Error, Debug)]
pub enum AndroidStorageError {
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[cfg(not(target_os = "android"))]
    #[error("Android storage not available on this platform")]
    NotAvailable,
}

/// Android 文件型存储后端。
///
/// 通过 `getFilesDir()` 获取 app 私有文件目录，
/// 以 JSON 文件 `persist.json` 存储键值对。
/// 不使用 `FindClass`，通过 `ndk_context` 获取 Activity 实例直接调用方法。
pub struct AndroidBackend {
    files_dir: PathBuf,
}

impl AndroidBackend {
    pub fn new() -> Result<Self, AndroidStorageError> {
        #[cfg(target_os = "android")]
        {
            let dir = android_files_dir().ok_or(AndroidStorageError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "getFilesDir failed",
            )))?;
            Ok(Self {
                files_dir: PathBuf::from(dir),
            })
        }
        #[cfg(not(target_os = "android"))]
        {
            Err(AndroidStorageError::NotAvailable)
        }
    }

    #[expect(dead_code, reason = "预留文件目录访问，模板项目未使用")]
    pub fn files_dir(&self) -> &Path {
        &self.files_dir
    }
}

impl StorageBackend for AndroidBackend {
    type Error = AndroidStorageError;

    fn file_name(&self) -> Option<&str> {
        Some("persist.json")
    }

    fn load_raw(&self, key: &str) -> Result<Option<String>, Self::Error> {
        let config = read_config(&self.files_dir)?;
        Ok(config.get(key).cloned())
    }

    fn save_raw(&self, key: &str, value: &str) -> Result<(), Self::Error> {
        let mut config = read_config(&self.files_dir)?;
        config.insert(key.to_string(), value.to_string());
        write_config(&self.files_dir, &config)
    }

    fn remove_raw(&self, key: &str) -> Result<(), Self::Error> {
        let mut config = read_config(&self.files_dir)?;
        config.remove(key);
        write_config(&self.files_dir, &config)
    }
}

/// Android 平台持久化存储类型别名。
pub type AndroidStorage = PersistStorage<AndroidBackend>;

fn persist_path(files_dir: &Path) -> PathBuf {
    files_dir.join("persist.json")
}

fn read_config(files_dir: &Path) -> Result<HashMap<String, String>, AndroidStorageError> {
    let path = persist_path(files_dir);
    match std::fs::read_to_string(&path) {
        Ok(content) => Ok(serde_json::from_str(&content).unwrap_or_default()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
        Err(e) => Err(AndroidStorageError::Io(e)),
    }
}

fn write_config(
    files_dir: &Path,
    config: &HashMap<String, String>,
) -> Result<(), AndroidStorageError> {
    let path = persist_path(files_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(config)?;
    std::fs::write(&path, content)?;
    Ok(())
}

#[cfg(target_os = "android")]
fn android_files_dir() -> Option<String> {
    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) };
    vm.attach_current_thread::<_, _, jni::errors::Error>(|env| {
        let activity = unsafe { jni::objects::JObject::from_raw(env, ctx.context().cast()) };

        let dir = env
            .call_method(
                &activity,
                jni::strings::JNIString::from("getFilesDir"),
                jni::jni_sig!("()Ljava/io/File;"),
                &[],
            )?
            .l()
            .map_err(|_| jni::errors::Error::NullPtr("getFilesDir returned null"))?;

        let path = env
            .call_method(
                &dir,
                jni::strings::JNIString::from("getAbsolutePath"),
                jni::jni_sig!("()Ljava/lang/String;"),
                &[],
            )?
            .l()
            .map_err(|_| jni::errors::Error::NullPtr("getAbsolutePath returned null"))?;

        let jstr = unsafe { jni::objects::JString::from_raw(env, path.into_raw()) };
        jstr.try_to_string(env)
    })
    .ok()
}

#[cfg(target_os = "android")]
pub(crate) fn android_cache_dir() -> Option<String> {
    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) };
    vm.attach_current_thread::<_, _, jni::errors::Error>(|env| {
        let activity = unsafe { jni::objects::JObject::from_raw(env, ctx.context().cast()) };

        let dir = env
            .call_method(
                &activity,
                jni::strings::JNIString::from("getCacheDir"),
                jni::jni_sig!("()Ljava/io/File;"),
                &[],
            )?
            .l()
            .map_err(|_| jni::errors::Error::NullPtr("getCacheDir returned null"))?;

        let path = env
            .call_method(
                &dir,
                jni::strings::JNIString::from("getAbsolutePath"),
                jni::jni_sig!("()Ljava/lang/String;"),
                &[],
            )?
            .l()
            .map_err(|_| jni::errors::Error::NullPtr("getAbsolutePath returned null"))?;

        let jstr = unsafe { jni::objects::JString::from_raw(env, path.into_raw()) };
        jstr.try_to_string(env)
    })
    .ok()
}
