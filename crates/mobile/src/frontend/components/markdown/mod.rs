pub mod renderer;

use dioxus::prelude::*;
use dioxus_i18n::prelude::i18n;

use crate::frontend::components::markdown::renderer::{MarkdownRenderContext, Render};
use crate::frontend::i18n as i18n_support;
use crate::frontend::js_bridge::{DioxusBridge, js_api};

stylance::import_style!(styles, "markdown.module.scss");

/// Markdown 内容渲染组件。
///
/// 使用 pulldown-cmark 解析 Markdown 文本并生成 HTML，
/// 再通过 `dangerous_inner_html` 注入 DOM。
/// 代码高亮通过 highlight.js 实现，LaTeX 公式通过 KaTeX auto-render 渲染。
///
/// # 本地链接跳转机制
///
/// JS 侧拦截非 HTTP 链接点击，派发 `CustomEvent('link-click')`，
/// Rust 侧在 `App` 组件（根布局，跨路由不卸载）中通过
/// `Bridge.waitForEvent('link-click')` 等待该事件并导航。
/// 循环放在 App 层而非本组件，避免组件卸载/重渲染导致 spawn 循环失效。
#[component]
pub fn Markdown(content: Signal<String>, file_path: Option<String>) -> Element {
    let i18n_ctx = i18n();
    let md_id = use_hook(|| {
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        format!(
            "md-{}",
            COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        )
    });

    let ctx = use_context::<Signal<MarkdownRenderContext>>();
    let renderer = ctx.read().renderer.clone();

    let file_path_for_resource = file_path.clone();
    let file_path_for_attr = file_path.clone();

    let html_resource: Resource<String> = use_resource(move || {
        let renderer = renderer.clone();
        let content = content.read().clone();
        let file_path = file_path_for_resource.clone();
        async move {
            renderer
                .ask(Render { content, file_path })
                .await
                .unwrap_or_default()
        }
    });

    let id_for_effect = md_id.clone();
    use_effect(move || {
        if html_resource().is_some() {
            render_highlight_and_katex(&id_for_effect);
        }
    });

    // 灯箱的返回键拦截已上移到 App 层全局注册（Typst 预览等视图也会开灯箱）。
    // 组件卸载时强制关闭灯箱：浮层挂在 body 下不受组件树管理，
    // 否则路由切换后浮层会残留遮住整个界面（closeLightbox 幂等，未打开时为空操作）
    use_drop(move || {
        js_api::markdown::close_lightbox::<DioxusBridge>();
    });

    match html_resource() {
        None => rsx! {
            div { class: styles::md_content, { i18n_support::translate(&i18n_ctx, "common-rendering") } }
        },
        Some(safe_html) => rsx! {
            document::Stylesheet { href: asset!("/ts/dist/katex/katex.min.css") }
            style { {katex_font_faces()} }
            div {
                class: styles::md_content,
                id: "{md_id}",
                "data-md-base-path": file_path_for_attr.as_deref().unwrap_or(""),
                dangerous_inner_html: "{safe_html}",
            }
        },
    }
}

fn katex_font_faces() -> String {
    format!(
        r#"
        @font-face{{font-family:KaTeX_AMS;src:url({}) format('woff2')}}
        @font-face{{font-family:KaTeX_Caligraphic;font-weight:700;src:url({}) format('woff2')}}
        @font-face{{font-family:KaTeX_Fraktur;font-weight:700;src:url({}) format('woff2')}}
        @font-face{{font-family:KaTeX_Fraktur;src:url({}) format('woff2')}}
        @font-face{{font-family:KaTeX_Main;src:url({}) format('woff2')}}
        @font-face{{font-family:KaTeX_Math;font-style:italic;src:url({}) format('woff2')}}
        @font-face{{font-family:KaTeX_SansSerif;font-weight:700;src:url({}) format('woff2')}}
        @font-face{{font-family:KaTeX_SansSerif;font-style:italic;src:url({}) format('woff2')}}
        @font-face{{font-family:KaTeX_SansSerif;src:url({}) format('woff2')}}
        @font-face{{font-family:KaTeX_Script;src:url({}) format('woff2')}}
        @font-face{{font-family:KaTeX_Size1;src:url({}) format('woff2')}}
        @font-face{{font-family:KaTeX_Size2;src:url({}) format('woff2')}}
        @font-face{{font-family:KaTeX_Size3;src:url({}) format('woff2')}}
        @font-face{{font-family:KaTeX_Size4;src:url({}) format('woff2')}}
        @font-face{{font-family:KaTeX_Typewriter;src:url({}) format('woff2')}}
        "#,
        asset!("/ts/dist/katex/fonts/KaTeX_AMS-Regular.woff2"),
        asset!("/ts/dist/katex/fonts/KaTeX_Caligraphic-Bold.woff2"),
        asset!("/ts/dist/katex/fonts/KaTeX_Fraktur-Bold.woff2"),
        asset!("/ts/dist/katex/fonts/KaTeX_Fraktur-Regular.woff2"),
        asset!("/ts/dist/katex/fonts/KaTeX_Main-Regular.woff2"),
        asset!("/ts/dist/katex/fonts/KaTeX_Math-Italic.woff2"),
        asset!("/ts/dist/katex/fonts/KaTeX_SansSerif-Bold.woff2"),
        asset!("/ts/dist/katex/fonts/KaTeX_SansSerif-Italic.woff2"),
        asset!("/ts/dist/katex/fonts/KaTeX_SansSerif-Regular.woff2"),
        asset!("/ts/dist/katex/fonts/KaTeX_Script-Regular.woff2"),
        asset!("/ts/dist/katex/fonts/KaTeX_Size1-Regular.woff2"),
        asset!("/ts/dist/katex/fonts/KaTeX_Size2-Regular.woff2"),
        asset!("/ts/dist/katex/fonts/KaTeX_Size3-Regular.woff2"),
        asset!("/ts/dist/katex/fonts/KaTeX_Size4-Regular.woff2"),
        asset!("/ts/dist/katex/fonts/KaTeX_Typewriter-Regular.woff2"),
    )
}

fn render_highlight_and_katex(selector_id: &str) {
    js_api::markdown::render::<DioxusBridge>(selector_id);
}
