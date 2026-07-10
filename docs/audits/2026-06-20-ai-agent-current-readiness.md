# AI Agent Current Readiness

## 阶段 0 历史基线

AAR-001 及相关问题来自阶段 0 历史基线，记录的是当时的风险入口，不等于当前仍然开放。已修复项保留在 issue matrix 中用于追溯。

## 当前状态

- 最小实现已覆盖任务生命周期、checkpoint、权限预检和工具确认。
- 真实 LLM 路径保留 provider 差异，新增适配必须有合同测试。
- 前端新增字段均为可选字段，以便旧会话和旧 checkpoint 继续读取。
- 历史基线与当前 readiness 分开维护，避免把已修复风险误当成发布阻塞。
