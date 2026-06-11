# Notion UI 重建 — 实现计划

**版本**：v0.4.0-ui  
**设计宪章**：[design-system.md](../design-system.md) · [notion-reference-summary.md](./2026-06-11-notion-reference-summary.md)

## 目标

- 纸墨 + 信纸 → Notion 式扁平编辑
- 去掉行线、纸页卡片、衬线、段首缩进
- 保留：主标题、Tab 同步、折叠、目录、Zen、AI 280px

## 阶段

1. Token：`globals.css`、`tailwind.config.js`、`index.html`（Inter）
2. 编辑器：`TipTapEditor` → `iris-editor-canvas`
3. Chrome：TabBar、StatusBar、Welcome、AiPanel、ui 组件
4. 清理：死变量、更新 `design-tokens.test.ts`

## 验收

- `pnpm run lint` / `typecheck` / `test`
- 亮/暗主题、缩放、目录、折叠、Zen 手动走查
