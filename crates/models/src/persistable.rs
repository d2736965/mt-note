/// 标记可持久化类型。
///
/// 通过 [`#[derive(Persistable)]`](models_macros::Persistable) 派生，
/// 以 `#[persistable(key = "...")]` 声明存储键。
///
/// ```ignore
/// #[derive(Persistable)]
/// #[persistable(key = "theme")]
/// pub enum Theme {
///     #[default] Light,
///     Dark,
/// }
/// ```
///
/// 枚举变体标注 `#[default]` 则同时生成 `impl Default`。
pub trait Persistable {
    /// 存储键，各平台以此键读写底层 k:v 存储。
    const KEY: &'static str;
}
