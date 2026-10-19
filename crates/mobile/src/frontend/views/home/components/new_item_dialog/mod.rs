//! 新建文件/目录对话框。

use std::path::PathBuf;

use dioxus::prelude::*;
use dioxus_i18n::prelude::i18n;
use ui::components::overlay::Overlay;

use crate::frontend::i18n as i18n_support;

stylance::import_style!(styles, "new_item_dialog.module.scss");

/// 新建项类型。
#[derive(Clone, Copy, PartialEq)]
pub enum NewItemKind {
    /// 文件
    File,
    /// 目录
    Directory,
}

/// 新建请求：目标目录 + 类型 + 是否在收藏夹浏览中。
///
/// `in_favorites` 决定创建成功后递增哪个刷新计数器
/// （Tab1 的 `refresh_counter` 或 FavDirBrowser 的 `fav_refresh_counter`）。
#[derive(Clone, PartialEq)]
pub struct NewItemTarget {
    /// 在哪个目录下创建
    pub dir: PathBuf,
    /// 文件 or 目录
    pub kind: NewItemKind,
    /// 是否在收藏夹目录浏览中创建（决定刷新哪个 counter）
    pub in_favorites: bool,
}

/// 新建文件/目录对话框属性。
#[derive(Props, Clone, PartialEq)]
pub struct NewItemDialogProps {
    /// 目标目录 + 类型
    pub target: NewItemTarget,
    /// 输入值（由 Home 拥有，便于打开/关闭时清空）
    pub value: Signal<String>,
    /// 错误提示（Some 时在输入框下方红色展示，如「路径已存在」）
    pub error: Signal<Option<String>>,
    /// 输入回调：Home 更新 value + 清空 error
    pub on_input: EventHandler<String>,
    /// 确定回调：Home 执行 create_file/create_dir，成功关闭+刷新，失败设 error
    pub on_confirm: EventHandler<()>,
    /// 取消/关闭回调：Home 关闭对话框 + 清空 value/error
    pub on_cancel: EventHandler<()>,
}

/// 新建文件/目录对话框。
///
/// 用公共 `Overlay` 组件做遮罩（点击外部关闭），
/// 卡片为兄弟节点，`stop_propagation` 阻止点击卡片触发关闭。
#[component]
pub fn NewItemDialog(props: NewItemDialogProps) -> Element {
    // i18n Context（渲染期取词自动订阅语言变化）
    let i18n_ctx = i18n();
    let title = match props.target.kind {
        NewItemKind::File => i18n_support::translate(&i18n_ctx, "common-new-file"),
        NewItemKind::Directory => i18n_support::translate(&i18n_ctx, "common-new-dir"),
    };
    let value = props.value;
    let error = props.error;

    rsx! {
        Overlay { on_close: move |_| props.on_cancel.call(()) }

        div {
            class: styles::dialog_card,
            onclick: move |e: Event<MouseData>| e.stop_propagation(),

            div { class: styles::dialog_title, "{title}" }

            input {
                class: styles::dialog_input,
                r#type: "text",
                value: "{value}",
                autofocus: true,
                placeholder: i18n_support::translate(&i18n_ctx, "common-name"),
                oninput: move |e| props.on_input.call(e.value()),
                onkeydown: move |e: Event<KeyboardData>| {
                    if e.key() == Key::Enter {
                        props.on_confirm.call(());
                    }
                },
            }

            if let Some(msg) = error() {
                div { class: styles::dialog_error, "{msg}" }
            }

            div { class: styles::dialog_actions,
                button {
                    class: "{styles::btn} {styles::btn_cancel}",
                    onclick: move |_| props.on_cancel.call(()),
                    { i18n_support::translate(&i18n_ctx, "common-cancel") }
                }
                button {
                    class: "{styles::btn} {styles::btn_ok}",
                    onclick: move |_| props.on_confirm.call(()),
                    { i18n_support::translate(&i18n_ctx, "common-confirm") }
                }
            }
        }
    }
}
