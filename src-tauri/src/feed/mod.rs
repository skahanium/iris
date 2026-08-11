//! 订阅资料库模块（阶段 1：仓储层；阶段 2 起扩展 fetch/normalize/sync）。
//!
//! 数据契约以 `docs/rss-subscription-library.md` 为准；仓储只读写应用级
//! SQLite，不触碰用户 Vault，不发起任何网络请求。

pub mod model;
pub mod repository;

#[cfg(test)]
mod repository_tests;
