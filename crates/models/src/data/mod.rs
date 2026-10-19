//! 可持久化数据模型。
//!
//! 每个子模块定义一个数据类型并为其实现 [`Persistable`](crate::Persistable)，
//! 声明唯一的 [`KEY`](crate::Persistable::KEY) 供平台存储层映射到底层 k:v 键。
//!
//! # 扩展方式
//!
//! 新增可持久化数据时，在此目录下新建模块，通过 `#[derive(Persistable)]` 注册 `KEY`：
//!
//! ```ignore
//! // models/src/data/user_prefs.rs
//! #[derive(Persistable)]
//! #[persistable(key = "user_prefs")]
//! pub struct UserPrefs { ... }
//! ```
//!
//! 然后在 `app.rs` 中通过 `storage.load::<UserPrefs>()` / `storage.save(&prefs)` 读写。
//!
//! # 当前注册的 KEY
//!
//! | 模块      | 类型     | KEY       | 说明         |
//! | --------- | -------- | --------- | ------------ |
//! | [`theme`] | `Theme`  | `"theme"` | 亮/暗主题    |
//! | [`editor_prefs`] | `OpenMode` | `"open_mode"` | 打开模式偏好 |
//! | [`editor_prefs`] | `EditorPrefs` | `"editor_prefs"` | 编辑器显示偏好 |
//! | [`typst_prefs`] | `TypstPrefs` | `"typst_prefs"` | Typst 编译偏好 |
//! | [`favorites`] | `Favorites` | `"favorites"` | 收藏的目录路径列表 |
//! | [`general_prefs`] | `GeneralPrefs` | `"general_prefs"` | 通用偏好（启动页等） |

pub mod browser_prefs;
pub mod editor_prefs;
pub mod favorites;
pub mod general_prefs;
pub mod language;
pub mod theme;
pub mod typst_prefs;
