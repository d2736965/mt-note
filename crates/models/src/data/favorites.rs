//! 收藏夹数据。

use serde::{Deserialize, Serialize};

/// 收藏的目录路径列表。
///
/// 持久化存储用户收藏的目录路径，在首页收藏夹 tab 中展示。
#[derive(Clone, Debug, Default, Serialize, Deserialize, crate::Persistable)]
#[persistable(key = "favorites")]
pub struct Favorites {
    /// 收藏的目录绝对路径列表（按添加顺序）。
    #[serde(default)]
    pub dirs: Vec<String>,
}
