# Changelog

本项目的重要变更记录于此，格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)，版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## [1.2.6] — Current

开发中，尚未发布。计划范围见 [ROADMAP.md](./ROADMAP.md) 与 [RAG 优化设计](./docs/specs/v1.2.6-rag-optimization.md)。本节只在功能完成并经验证后记录用户可见变更。

### Changed

- 将项目当前开发分支的受控版本事实校准为 1.2.6。
- 收口文档体系：删除失效施工资料，以代码事实统一安全、Skills、迁移和检索说明。

## [1.2.5] — Released

已发布标签。保留该标签及其发布产物，不回写或重打标签。

## [1.2.4] — Historical baseline

历史功能包括本地 Markdown 编辑、文件索引与 FTS 搜索、知识图谱、版本快照、统一 AI 助手、LLM 配置和本地加密凭据。详细差异以对应提交及发布标签为准。

## 维护规则

- 只记录已交付、可验证的变更；计划不写入本文件。
- 发布版本时使用 `npm run version:set -- <version>` 后运行 `npm run version:check`。
- 旧的执行计划、审计草稿和已被替代的规格通过 git 历史追溯。
