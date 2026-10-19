use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};

use dioxus::logger::tracing;
use js_bridge_bindgen::JsBridge;

const BUNDLE_JS: &str = include_str!("../../ts/dist/mt-note-mobile-ts.js");
const GLOBAL_FACTORY: &str = "__mt_note_mobile";

static BUNDLE_INJECTED: AtomicBool = AtomicBool::new(false);

static ENSURE_BUNDLE_JS: LazyLock<String> = LazyLock::new(|| {
    let bundle_json = match serde_json::to_string(BUNDLE_JS) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("js_bridge: failed to serialize BUNDLE_JS: {e}");
            return String::new();
        }
    };
    format!("window.{GLOBAL_FACTORY}||(0,eval)({bundle_json})")
});

#[expect(
    clippy::disallowed_methods,
    reason = "唯一的合法裸 JS 调用点：注入 JS bundle 到运行时"
)]
fn ensure_bundle_injected() {
    if !BUNDLE_INJECTED.load(Ordering::Relaxed) {
        dioxus::document::eval(&ENSURE_BUNDLE_JS);
        BUNDLE_INJECTED.store(true, Ordering::Relaxed);
    }
}

pub struct DioxusBridge;

impl JsBridge for DioxusBridge {
    const GLOBAL_OBJECT: &str = "window";

    #[expect(
        clippy::disallowed_methods,
        reason = "JsBridge trait 实现：唯一的合法裸 JS 调用点"
    )]
    fn eval_js(js: &str) {
        tracing::debug!("eval_js: {}", &js[..js.len().min(200)]);
        dioxus::document::eval(js);
    }

    #[expect(
        clippy::disallowed_methods,
        reason = "JsBridge trait 实现：唯一的合法裸 JS 调用点"
    )]
    async fn eval_async(js: &str) -> serde_json::Value {
        let wrapped = format!("return (async()=>{{{js}}})()");
        dioxus::document::eval(&wrapped)
            .join::<serde_json::Value>()
            .await
            .unwrap_or(serde_json::Value::Null)
    }

    fn ensure_injected() {
        ensure_bundle_injected();
    }
}

pub mod js_api {
    include!(concat!(env!("OUT_DIR"), "/js_api.rs"));
}
