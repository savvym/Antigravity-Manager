pub mod account;
pub mod quota;
pub mod config;
pub mod logger;
pub mod db;
pub mod process;
pub mod oauth;
pub mod migration;
pub mod i18n;

// GUI 专用模块
#[cfg(feature = "gui")]
pub mod tray;
#[cfg(feature = "gui")]
pub mod oauth_server;

use crate::models;

// 重新导出常用函数到 modules 命名空间顶级，方便外部调用
pub use account::*;
pub use quota::*;
pub use config::*;
pub use logger::*;

pub async fn fetch_quota(access_token: &str, email: &str) -> crate::error::AppResult<(models::QuotaData, Option<String>)> {
    quota::fetch_quota(access_token, email).await
}
