// deny：Dioxus 宏内部生成 #[allow] 导致冲突，无法使用 forbid
#![deny(
    unused_must_use,   // #[component] 展开包含 #[allow(unused)]
    clippy::panic,     // #[derive(Props)] 展开包含 #[allow(clippy::panic)]
    clippy::disallowed_methods,      // 启用 clippy.toml disallowed-methods 清单（deny 允许 #[expect] 豁免 js_bridge.rs）
)]
// forbid：无法被 #[allow]/#[expect] 覆盖，零例外
#![forbid(
    clippy::let_underscore_must_use, // 禁止 let _ = 忽略 #[must_use]，必须用 if let Err
    clippy::unwrap_used,             // 禁止 .unwrap()，必须显式处理错误
    clippy::expect_used,             // 禁止 .expect()，必须显式处理错误
    clippy::disallowed_types,        // 启用 clippy.toml disallowed-types 清单
    clippy::allow_attributes,        // 禁止 #[allow]，强制 #[expect] 并附注原因
    unfulfilled_lint_expectations,   // #[expect] 不再触发时报错，提醒移除
)]

mod frontend;

use dioxus::prelude::*;
use frontend::panic_hook;
use frontend::router::Route;

fn main() {
    // Android 下在 launch 前初始化 tracing subscriber（logcat layer）。
    // dioxus::launch 内部会调 dioxus_logger::initialize_default()，
    // 但它检测 dispatcher::has_been_set() == true 会跳过，不冲突。
    //
    // 关键：必须同时设置 dispatcher（旧 API）和 subscriber（新 API），
    // 因为 dioxus_logger::initialize_default() 用 dispatcher::has_been_set()
    // 检查，而我们的 subscriber 用 subscriber::set_global_default 设置。
    // 两者是不同的全局状态，只设 subscriber 不设 dispatcher 的话，
    // dioxus 会认为没有初始化，用 FmtSubscriber 覆盖我们的设置。
    #[cfg(target_os = "android")]
    {
        use dioxus::logger::tracing;
        use tracing_subscriber::prelude::*;
        let layer = match tracing_android::layer("MtNote") {
            Ok(l) => l,
            Err(e) => unreachable!("tracing-android layer 创建失败: {e}"),
        };
        let filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug"));
        let subscriber = tracing_subscriber::Registry::default()
            .with(filter)
            .with(layer);
        if let Err(e) = tracing::subscriber::set_global_default(subscriber) {
            unreachable!("tracing subscriber already set: {e}");
        }
    }

    // panic hook 必须在 tracing subscriber 之后安装，
    // 因为 hook 内用 tracing::error! 输出，subscriber 未初始化时日志会丢失。
    panic_hook::install();

    dioxus::launch(app);
}

/// 根组件，挂载路由器。`App` 由 `#[layout(App)]` 标记为布局组件，
/// 自动包裹所有子路由。
fn app() -> Element {
    use_local_file_asset_handler();

    rsx! {
        Router::<Route> {}
    }
}

fn use_local_file_asset_handler() {
    dioxus::mobile::use_asset_handler("local-file", |request, responder| {
        let path_str = request.uri().path();
        let relative = path_str.strip_prefix("/local-file/").unwrap_or(path_str);
        // percent-decode：request.uri().path() 返回 encoded 路径（%E7%9F%A5...），
        // 文件系统需要解码后的原始路径（知识/...）。未解码会导致含中文/非 ASCII
        // 字符的本地资源（如 _res/ 目录下的图片）读不到。
        let decoded = urlencoding::decode(relative)
            .unwrap_or_else(|_| relative.into())
            .into_owned();
        let file_path = format!("/{decoded}");

        match std::fs::read(&file_path) {
            Ok(content) => {
                let mime = mime_type_from_path(&file_path);
                let response = dioxus::mobile::wry::http::Response::builder()
                    .header("Content-Type", mime)
                    .body(content);
                match response {
                    Ok(r) => responder.respond(r),
                    Err(e) => {
                        dioxus::logger::tracing::error!(
                            "local-file handler: response build failed: {e}"
                        );
                        responder.respond(not_found_response());
                    }
                }
            }
            Err(e) => {
                dioxus::logger::tracing::debug!("local-file handler: file not found: {e}");
                responder.respond(not_found_response());
            }
        }
    });
}

fn mime_type_from_path(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("");
    match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "pdf" => "application/pdf",
        "md" => "text/markdown",
        "txt" => "text/plain",
        "html" => "text/html",
        "css" => "text/css",
        "js" => "application/javascript",
        "json" => "application/json",
        _ => "application/octet-stream",
    }
}

fn not_found_response() -> dioxus::mobile::wry::http::Response<Vec<u8>> {
    dioxus::mobile::wry::http::Response::new(Vec::new())
}
