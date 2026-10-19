//! Android panic 捕获 + 诊断日志。
//!
//! 复用 dioxus::logger::tracing（已配置 tracing_android layer 输出到 logcat），
//! 无需直写 __android_log_write FFI。

use dioxus::logger::tracing;

/// 安装 panic hook，把 panic 信息通过 tracing 输出到 logcat。
pub fn install() {
    std::panic::set_hook(Box::new(|info| {
        let payload = info.payload();
        let msg = if let Some(s) = payload.downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = payload.downcast_ref::<String>() {
            s.clone()
        } else {
            "<non-string panic payload>".to_string()
        };
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "<unknown>".to_string());
        tracing::error!("RUST PANIC at {location}: {msg}");
    }));
}
