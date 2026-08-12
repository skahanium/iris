//! 订阅资料库模块（阶段 1：仓储层；阶段 2：安全获取/规范化/同步）。
//!
//! 数据契约以 `docs/rss-subscription-library.md` 为准；仓储只读写应用级
//! SQLite，获取只访问经 SSRF 校验的 HTTPS 公开地址，不触碰用户 Vault。

pub mod model;
pub mod opml;
pub mod repository;

pub(crate) mod discovery;
pub(crate) mod fetch;
pub(crate) mod normalize;
pub(crate) mod sync;

#[cfg(test)]
mod discovery_tests;
#[cfg(test)]
mod fetch_tests;
#[cfg(test)]
mod normalize_tests;
#[cfg(test)]
mod opml_tests;
#[cfg(test)]
mod repository_tests;
#[cfg(test)]
mod sync_tests;
#[cfg(test)]
pub(crate) mod test_http;
