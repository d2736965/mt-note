/// JNI 操作错误。
#[derive(thiserror::Error, Debug)]
pub enum JniError {
    #[error("JNI error: {0}")]
    Jni(#[from] jni::errors::Error),
}

/// 设置 Android 系统栏（状态栏+导航栏）主题：图标颜色+背景色。
///
/// 通过 `ndk_context` 获取 NativeActivity 实例，以 instance method 调用
/// `MainActivity.setSystemBarTheme(ZLjava/lang/String;)V`，无需 `FindClass`。
#[cfg(target_os = "android")]
pub fn set_system_bar_theme(is_dark: bool, surface_color: &str) -> Result<(), JniError> {
    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) };
    vm.attach_current_thread::<_, _, JniError>(|env| {
        let activity = unsafe { jni::objects::JObject::from_raw(env, ctx.context().cast()) };
        let is_dark = jni::objects::JValue::Bool(is_dark);
        let surface_color = env.new_string(surface_color)?;
        env.call_method(
            &activity,
            jni::strings::JNIString::from("setSystemBarTheme"),
            jni::jni_sig!("(ZLjava/lang/String;)V"),
            &[is_dark, (&surface_color).into()],
        )?;
        Ok(())
    })
}

#[cfg(not(target_os = "android"))]
pub fn set_system_bar_theme(_is_dark: bool, _surface_color: &str) -> Result<(), JniError> {
    Ok(())
}

#[cfg(target_os = "android")]
pub fn is_storage_manager_granted() -> Result<bool, JniError> {
    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) };
    vm.attach_current_thread::<_, _, JniError>(|env| {
        let activity = unsafe { jni::objects::JObject::from_raw(env, ctx.context().cast()) };
        let result = env.call_method(
            &activity,
            jni::strings::JNIString::from("isStorageManagerGranted"),
            jni::jni_sig!("()Z"),
            &[],
        )?;
        match result {
            jni::JValueOwned::Bool(b) => Ok(b),
            _ => Ok(false),
        }
    })
}

#[cfg(not(target_os = "android"))]
pub fn is_storage_manager_granted() -> Result<bool, JniError> {
    Ok(true)
}

#[cfg(target_os = "android")]
pub fn open_all_files_access_settings() -> Result<(), JniError> {
    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) };
    vm.attach_current_thread::<_, _, JniError>(|env| {
        let activity = unsafe { jni::objects::JObject::from_raw(env, ctx.context().cast()) };
        env.call_method(
            &activity,
            jni::strings::JNIString::from("openAllFilesAccessSettings"),
            jni::jni_sig!("()V"),
            &[],
        )?;
        Ok(())
    })
}

#[cfg(not(target_os = "android"))]
pub fn open_all_files_access_settings() -> Result<(), JniError> {
    Ok(())
}

#[cfg(target_os = "android")]
pub fn get_and_clear_pending_shared_file() -> Result<Option<String>, JniError> {
    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) };
    vm.attach_current_thread::<_, _, JniError>(|env| {
        let activity = unsafe { jni::objects::JObject::from_raw(env, ctx.context().cast()) };
        let result = env.call_method(
            &activity,
            jni::strings::JNIString::from("getAndClearPendingSharedFile"),
            jni::jni_sig!("()Ljava/lang/String;"),
            &[],
        )?;
        match result {
            jni::JValueOwned::Object(obj) => {
                if obj.is_null() {
                    Ok(None)
                } else {
                    let jstr = unsafe { jni::objects::JString::from_raw(env, obj.into_raw()) };
                    let s = jstr.try_to_string(env)?;
                    Ok(Some(s))
                }
            }
            _ => Ok(None),
        }
    })
}

#[cfg(not(target_os = "android"))]
pub fn get_and_clear_pending_shared_file() -> Result<Option<String>, JniError> {
    Ok(None)
}

#[cfg(target_os = "android")]
pub fn move_task_to_back() -> Result<(), JniError> {
    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) };
    vm.attach_current_thread::<_, _, JniError>(|env| {
        let activity = unsafe { jni::objects::JObject::from_raw(env, ctx.context().cast()) };
        env.call_method(
            &activity,
            jni::strings::JNIString::from("moveTaskToBack"),
            jni::jni_sig!("()V"),
            &[],
        )?;
        Ok(())
    })
}

#[cfg(not(target_os = "android"))]
pub fn move_task_to_back() -> Result<(), JniError> {
    Ok(())
}

/// 弹出 Android 系统选择器，用外部应用打开文件。
///
/// 通过 `FileProvider` 将文件路径转为 `content://` URI，
/// 构造 `Intent.ACTION_VIEW` + `Intent.createChooser` 弹出选择器。
/// 需在 `AndroidManifest.xml` 注册 `FileProvider`。
#[cfg(target_os = "android")]
pub fn open_with_external_app(path: &str) -> Result<(), JniError> {
    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) };
    vm.attach_current_thread::<_, _, JniError>(|env| {
        let activity = unsafe { jni::objects::JObject::from_raw(env, ctx.context().cast()) };
        let jstr = env.new_string(path)?;
        env.call_method(
            &activity,
            jni::strings::JNIString::from("openWithExternalApp"),
            jni::jni_sig!("(Ljava/lang/String;)V"),
            &[jni::objects::JValue::Object(&jstr.into())],
        )?;
        Ok(())
    })
}

#[cfg(not(target_os = "android"))]
pub fn open_with_external_app(_path: &str) -> Result<(), JniError> {
    Ok(())
}

/// 读取系统语言标签（BCP-47，如 "zh-CN"），用于默认界面语言解析。
///
/// 通过 `ndk_context` 获取 Activity 实例，以 instance method 调用
/// `MainActivity.getSystemLanguage()`，无需 `FindClass`。
#[cfg(target_os = "android")]
pub fn get_system_language() -> Result<String, JniError> {
    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) };
    vm.attach_current_thread::<_, _, JniError>(|env| {
        let activity = unsafe { jni::objects::JObject::from_raw(env, ctx.context().cast()) };
        let result = env.call_method(
            &activity,
            jni::strings::JNIString::from("getSystemLanguage"),
            jni::jni_sig!("()Ljava/lang/String;"),
            &[],
        )?;
        match result {
            jni::JValueOwned::Object(obj) => {
                if obj.is_null() {
                    Ok(String::new())
                } else {
                    let jstr = unsafe { jni::objects::JString::from_raw(env, obj.into_raw()) };
                    Ok(jstr.try_to_string(env)?)
                }
            }
            _ => Ok(String::new()),
        }
    })
}

#[cfg(not(target_os = "android"))]
pub fn get_system_language() -> Result<String, JniError> {
    // 非 Android 宿主无 JNI：返回空串，上层按读取失败回退默认语言。
    Ok(String::new())
}
