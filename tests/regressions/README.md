# 防御性回归测试清单

本目录/清单用于记录线上暴露过的问题、根因和对应防御测试，防止今后被再次改坏。

## 空标题中文 IME 输入

- **现象**：`#` / `##` / `###` 空标题输入中文后，标题节点变成正文段落，字号缩水，目录岛丢失该标题。
- **根因**：WebKit 在空 heading 上进入中文/日文 IME 组合态时，会把组合文本包成 `<p>`，ProseMirror 归一化成“空 heading + 段落”；另外 `markdown-prose.css` 若被 Tailwind 层覆盖或加载顺序错误，标题字号会失效。
- **防御测试**：
  - `tests/editor-empty-heading-ime-guard.test.ts`：空标题保持 ZWSP 占位、中文插入后仍是 heading、outline 可提取、序列化不泄漏 ZWSP。
  - `tests/editor-heading-dom-guard.test.ts`：模拟“空标题 + 紧随中文段落”的坏事务，验证合并回 heading、outline 与序列化保持。
  - `tests/editor-heading-guard-wiring.test.ts`：验证三个 guard 扩展同时存在于真实 `TipTapEditor` 和生产 round-trip 扩展工厂。
  - `tests/prose-tokens.test.ts` / `tests/editor-zoom.test.ts`：验证 `markdown-prose.css` 在 `globals.css` 之后加载、不在 `@layer components` 内、h1-h3 字号规则存在。

## 涉密保险柜文档标题重命名被还原

- **现象**：`.classified/` 下文档修改标题后，保存时被还原成原标题。
- **根因**：普通 `document_rename_by_title` 命令不支持涉密路径；涉密路径必须走 `classified_rename`，否则移动失败后在 `finally` 中恢复旧标题。
- **防御测试**：
  - `tests/use-open-note-classified-rename.test.tsx`：验证 `.classified/` 路径只调用 `classifiedRename`、绝不调用 `documentRenameByTitle`，成功/失败/空标题/嵌套目录行为均覆盖。
  - `tests/classified-ipc.test.ts`：验证 `classifiedRename` IPC 封装与后端命令注册。
