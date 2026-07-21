# Changelog

本项目的重要变更记录于此，格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)，版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## [1.2.13] — Current

开发中，尚未发布。计划范围见 [ROADMAP.md](./ROADMAP.md) 与 [RAG 优化设计](./docs/specs/v1.2.6-rag-optimization.md)。本节只在功能完成并经验证后记录用户可见变更。

### Added

- 内置 BGE-small-zh-v1.5 中文嵌入模型（512 维），通过 `model:prepare` 脚本离线准备并校验 SHA-256。
- 增量嵌入代际迁移（044–046）：`embedding_generation_state`、`chunk_embeddings_v2`、`anchor_embeddings_v2`、`regulation_embeddings_v2`、`files_metadata_fts`。
- Rank v2 融合排序：加权 RRF（k=60）+ MMR 去重（λ=0.75）+ 精确法规优先 + 来源配额 + corpus 角色加权。
- 混合检索 broker 逐层诊断（`RetrievalLayerDiagnostic`），覆盖 FTS、向量、图谱、精确法规与模板各层。
- `ContextScope` 新增 `requiredTags`，支持 AND 语义标签约束。
- `search_embedding_status` IPC 命令与 `EmbeddingIndexStatus` 类型，暴露模型、维度、重建进度与降级状态。
- rag-v2-vault 评测夹具：48 篇合成笔记 + 60 条分级查询 + 质量门禁测试。
- 前端 `@` 提及 scope selector 与 `ContextScopeChips` 组件。

### Changed

- 将项目当前开发分支的受控版本事实校准为 1.2.6。
- 收口文档体系：删除失效施工资料，以代码事实统一安全、Skills、迁移和检索说明。
- 语义搜索默认使用 BGE v2 512 维 raw f32 LE 格式；旧 384 维量化格式仅保留兼容读取。

## [1.2.5] — 2026-07-09

### Added

- LLM 凭据管理重构：AES-256-GCM 本地加密存储，主密钥与密文分离存放，`Zeroizing` 自动清零。
- LLM 推理路由：模型注册、能力解析、推理模式（reasoning）支持与 token 预算强制。
- AI 工具调用增强：内部工具参数过滤、可见内容处理与输出净化。
- 新建笔记按钮（标签栏溢出处理优化）。

### Changed

- AI 消息处理性能优化。
- LLM API 请求体构建重构，输入 token 预算在构造阶段强制执行。
- GitHub Actions 工作流升级至最新官方 actions 版本，新增草稿发布能力。

### Fixed

- CI 合并后验证失败修复。

## [1.2.4] — Historical baseline

历史功能包括本地 Markdown 编辑、文件索引与 FTS 搜索、知识图谱、版本快照、统一 AI 助手、LLM 配置和本地加密凭据。详细差异以对应提交及发布标签为准。

## 维护规则

- 只记录已交付、可验证的变更；计划不写入本文件。
- 发布版本时使用 `npm run version:set -- <version>` 后运行 `npm run version:check`。
- 旧的执行计划、审计草稿和已被替代的规格通过 git 历史追溯。
