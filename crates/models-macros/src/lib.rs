#![forbid(
    unused_must_use,
    clippy::panic,
    clippy::let_underscore_must_use,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::allow_attributes,
    unfulfilled_lint_expectations
)]

mod persistable;

/// 为类型派生 [`Persistable`] trait，通过属性指定存储键。
///
/// 若枚举变体标注 `#[default]`，同时生成 `impl Default`。
///
/// # 用法
///
/// ```ignore
/// #[derive(Persistable)]
/// #[persistable(key = "theme")]
/// pub enum Theme {
///     #[default]
///     Light,
///     Dark,
/// }
/// ```
#[proc_macro_derive(Persistable, attributes(persistable, default))]
pub fn derive_persistable(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    persistable::expand_derive(input)
}
