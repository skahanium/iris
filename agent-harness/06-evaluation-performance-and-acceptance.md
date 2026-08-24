# 06. 评测、性能与验收

## 1. 验收原则

- “执行了研究流程”“回答看起来更聪明”和“事实得到支持”是三件不同的事。
- 硬门禁使用确定性合同和固定夹具；真实模型 pilot 用于协议与性能校准，不证明客观事实本身。
- 质量、性能、安全和技术债同时验收，不能通过放宽其中一项换取另一项表面改善。
- 只有当前工作树的命名测试通过后，状态才能写为“已验证”。

## 2. 质量与安全门槛

| 指标                                  | 最低要求 |
| ------------------------------------- | -------: |
| 事实召回率                            |      90% |
| 引用支持率                            |      95% |
| 约束遵循率                            |      95% |
| Web 关闭/classified 外发              |        0 |
| 本地内容进入 Web query                |        0 |
| foreign/retired evidence 成为精确引用 |        0 |
| 未确认写操作                          |        0 |
| 凭证、正文或原始 Provider 输出泄漏    |        0 |

结构化或精确当前事实还必须覆盖陈旧数据、缺地域、缺单位、缺时点、字段漂移和来源冲突负例。

## 3. 性能门槛

| 档位     | 硬时限 | 搜索 | 抓取 | 模型续接 | evidence | 其他要求             |
| -------- | -----: | ---: | ---: | -------: | -------: | -------------------- |
| Quick    |    20s |    1 |    2 |        2 |        4 | 简单事实证据充分即停 |
| Standard |    45s |    3 |    6 |        4 |        8 | 默认当前研究档位     |
| Deep     |    90s |    5 |   10 |        6 |       12 | 仅显式触发           |

- 全局继续受 8 模型轮次、24 工具调用和 32K Web packet 限制。
- 单轮抓取并发最多 3；连续两轮无新增证据必须停止。
- first progress event 的 Host 目标为 500ms 内。
- 建立固定基线后，同 profile p95 延迟或 token 增长超过 20% 为回归；只有质量获得可量化提升并记录例外时可接受。

## 4. 自动化测试分层

### 单元测试

- task shape、FreshFactDomain、时间窗、地域和 profile 选择；
- `EvidenceGap`、query/URL 去重、URL provenance 和各类预算；
- DTO 字段、时效、单位、地域、来源和 operation 一致性；
- evidence binding、失效、fallback 和 finalization；
- 平台路径安全语义与 deterministic fixture parity。

### 组件与仓储测试

- 并发 Run 幂等、终态事务、sink 故障和恢复；
- 首轮不足后只针对 gap 继续研究，并持久化剩余预算；
- current-Run 与 foreign/retired evidence 隔离；
- 结构化 snapshot 冻结、hash drift、provider ID 剥离和恢复不重执行；
- 同轮并发抓取不越界，取消与 deadline 能终止所有待执行动作。

### 前端合同测试

- 新 Run accepted 至首个 answer delta 期间不显示上一 Run 正文；
- 迟到 frame/event 不修改新 Run；
- Quick/Standard/Deep、搜索、抓取、证据不足和降级过程可恢复；
- 来源组、精确引用、未知 binding 和失败语义显示诚实。

### Agent capacity eval

- smoke 必须执行完整 24-case online deterministic matrix，`caseCount`、`completedCaseCount`、`passed` 均为 24，`failed` 为 0。
- full 执行 48-case、压力阶梯、硬边界、安全轨和组合终端；不得用旧版本化结果代替当前运行。
- 固定多轮场景覆盖 runtime 日期、近期影视、对推荐时效的质疑、Web 能力诚实、跨 Run UI 隔离和证据不足。

## 5. 当前评测证据

2026-08-24 当前工作树已经取得：

- `node --test scripts/agent-eval.test.mjs`：8/8；
- Windows real stdio MCP discovery/search：通过；
- 单一 headless online Web evidence binding：通过；
- `npm run agent:eval:smoke`：24/24。
- `npm run agent:eval`：48-case、压力阶梯、硬边界、安全轨和组合终端通过。
- `npm run lint`、`npm run format:check`、`npm run typecheck`、`npm run test`：通过，Vitest 353 个文件、2460 个测试通过。
- `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check` 与 `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`：通过。
- `cargo test --manifest-path src-tauri/Cargo.toml`：库测试 1822 通过、0 失败、3 忽略；后续集成测试与 doc-tests 全部通过。
- `npm run docs:check`：通过。

## 6. Live pilot 与隐私

本轮工作明确排除真实 Provider 与外部性能试点。Live pilot 只在后续用户明确授权、确认模型/profile 和单次费用 checkpoint 后执行。结果记录日期、模型、匿名 profile、配置 hash、场景 ID、p50/p95、token 和闭集 verdict；不得保存 prompt、answer、路径、URL、凭证或原始工具正文。

真实网页或真实笔记不得用于默认 CI。确定性 fixture 与真实 pilot 分开报告，任一方不能替代另一方。

## 7. 完成定义

一个 Harness 阶段只有同时满足以下条件才能结案：

1. 目标命名测试先红后绿，且相关全量测试通过；
2. 对应旧分支、旧测试和旧文档在同阶段删除；
3. 没有新增第二事实源、无界循环或未说明的新依赖；
4. 质量和安全门槛全部满足；
5. 性能位于档位硬预算内，基线无未解释的 20% 回归；
6. 状态与测试追踪表、`ARCHITECTURE.md` 已实现事实及根 `ROADMAP.md` 不冲突。

## 8. 最终质量命令

```bash
npm run docs:check
npm run agent:eval:smoke
npm run agent:eval
npm run lint
npm run format:check
npm run typecheck
npm run test
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

任一命令未通过时必须记录为阻塞，不得把局部通过描述成整体完成。
