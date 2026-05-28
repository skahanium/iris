# UI 命令浮层与纸墨抛光 — 实现计划

**版本里程碑**：`v0.3.1-ui`（仅体验，不阻塞 v0.3 功能线与 v1.0）  
**设计宪章**：[design-system.md](../design-system.md) § 命令浮层 / 纸页视口 / 圆角与动效  
**架构线框**：[ARCHITECTURE.md](../../ARCHITECTURE.md) § 悬浮层系统

**产品决策（已定稿）**

| #   | 决策                                                                        |
| --- | --------------------------------------------------------------------------- |
| 1   | 打开命令浮层时 **保持 AI 侧栏**，全窗 scrim 盖住（含 AI），不裁切编辑区宽度 |
| 2   | **同时仅一个** 命令浮层；新开则替换                                         |
| 3   | 纸页 **固定视口高度**，**仅纸内滚动**，纸边始终可见                         |
| 4   | 暗色主题：**暗纸 A**（深暖灰纸 + 浅字，护眼）                               |
| 5   | 圆角 **柔和 14–20px**（现代 SaaS）                                          |
| 6   | 版本浮层 **近全屏**（双栏）；图谱 **几乎全屏**                              |
| 7   | 范围含 **TabBar / StatusBar / AI 气泡** 一并翻新                            |
| 8   | 排期见 [ROADMAP § v0.3.1-ui](../../ROADMAP.md#v031-ui--命令浮层与纸墨抛光)  |

---

## 一、组件清单

### 1. 基础设施（新建 / 重构）

| 组件 / 模块                     | 路径                                            | 职责                                                                         |
| ------------------------------- | ----------------------------------------------- | ---------------------------------------------------------------------------- |
| **`IrisOverlay`**               | `src/components/ui/iris-overlay.tsx`            | 统一命令浮层：全屏 scrim、`size` 变体、enter/exit 动效、焦点陷阱、`Esc` 关闭 |
| **`overlay-sizes.ts`**          | `src/lib/overlay-sizes.ts`                      | `compact` / `command` / `wide` / `near-full` / `graph` 的 vw/vh/max 常量     |
| **`useOverlayManager`（增强）** | `src/hooks/useOverlayManager.ts`                | 单一 `activeOverlay`；`openOverlay(id)` 互斥；与 Quick Open 统一语义         |
| **Token 扩展**                  | `src/styles/globals.css` + `tailwind.config.js` | `--radius-*`、`--shadow-*`、`--motion-*`、`--overlay-scrim`；暗纸 token      |
| **`dialog.tsx` 对齐**           | `src/components/ui/dialog.tsx`                  | 与 `IrisOverlay` 共享 scrim/圆角/动效 class；Quick Open 用 `compact`         |

**`IrisOverlay` size 规范**

| size      | 典型用途                     | 尺寸（约）                         |
| --------- | ---------------------------- | ---------------------------------- |
| `compact` | Quick Open                   | max-w-xl，高 auto，居中            |
| `command` | 搜索、文件、设置、反链、标签 | 宽 80vw max-w-3xl，高 78vh         |
| `wide`    | 版本时间线（双栏）           | 宽 92vw max-w-7xl，高 88vh         |
| `graph`   | 知识图谱                     | 宽 96vw，高 92vh，圆角略小仍 ≥14px |

**废弃**：`src/components/ui/side-panel.tsx` 贴边形态 → 迁移完成后删除或仅保留 `@deprecated` 薄包装。

### 2. 命令浮层业务面板（迁移目标）

| 面板       | 现组件                | 迁移后 size | 备注                                 |
| ---------- | --------------------- | ----------- | ------------------------------------ |
| Quick Open | `QuickOpen.tsx`       | `compact`   | 已居中 Dialog，对齐 token            |
| 文件       | `FileSheet.tsx`       | `command`   | 去掉 `aiPanelOpen` / `right-[280px]` |
| 搜索       | `SearchPanel.tsx`     | `command`   | 同上                                 |
| 设置       | `SettingsPanel.tsx`   | `command`   | 同上                                 |
| 反链       | `BacklinksPanel.tsx`  | `command`   | 同上                                 |
| 标签       | `TagView.tsx`         | `command`   | 同上                                 |
| 版本       | `VersionTimeline.tsx` | `wide`      | 双栏对比占满浮层主体                 |
| 图谱       | `GraphView.tsx`       | `graph`     | 近全屏居中，非贴边                   |

### 3. 编辑区（纸页视口）

| 项          | 路径                                         | 改动要点                                                                          |
| ----------- | -------------------------------------------- | --------------------------------------------------------------------------------- |
| 纸页外壳    | `globals.css` `.iris-editor` / `.iris-paper` | 新增 `.iris-paper` 固定 `min-height: calc(100dvh - chrome)`，**overflow-y: auto** |
| ProseMirror | `globals.css` `.ProseMirror`                 | 去掉随内容收缩；宽度 max 42rem；圆角 16px；暗纸 token                             |
| 编辑器包装  | `TipTapEditor.tsx`                           | 结构：`iris-editor` > `iris-paper` > `EditorContent`                              |
| 欢迎页      | `WelcomeEmpty.tsx`                           | 与纸页圆角/阴影一致                                                               |

### 4. Chrome 翻新（v0.3.1-ui 范围）

| 组件             | 路径                                                         | 改动要点                                            |
| ---------------- | ------------------------------------------------------------ | --------------------------------------------------- |
| 标签栏           | `TabBar.tsx`                                                 | 圆角 pill 标签、hover/切换 150–200ms、焦点环        |
| 状态栏           | `StatusBar.tsx`                                              | 高度/分隔/字阶；与 token 对齐                       |
| 应用壳           | `AppShell.tsx`                                               | z-index 层：编辑 < AI < scrim < overlay             |
| AI 面板          | `AiPanel.tsx`                                                | 气泡圆角 14–16px、引用卡、输入区；非 Messenger 色块 |
| AI 相关          | 引用卡区块（若在 AiPanel 内）                                | 纸色/暗纸适配、细边框                               |
| 浮动工具条       | `FloatingToolbar.tsx`                                        | 圆角 `rounded-2xl`、阴影 elevation                  |
| 按钮/输入/对话框 | `button.tsx`, `input.tsx`, `dialog.tsx`, `ConfirmDialog.tsx` | 默认圆角升至 `rounded-xl` 档                        |

### 5. 测试

| 测试                  | 路径                                                        |
| --------------------- | ----------------------------------------------------------- |
| overlay 互斥          | `tests/use-overlay-manager.test.ts`（新建）                 |
| 纸页 CSS 快照（可选） | `tests/editor-paper-layout.test.tsx`（新建，测 class 结构） |
| 既有                  | `version-timeline-*` 等更新挂载容器为 `IrisOverlay`         |

---

## 二、文件级改造顺序

依赖：**P0 文档（已完成）→ P1 token → P2 IrisOverlay → P3 面板迁移 → P4 纸页 → P5 chrome → P6 抛光与测试**

```
P0 文档
  ROADMAP.md, design-system.md, ARCHITECTURE.md, 本文件

P1 设计 token（globals + tailwind）
  src/styles/globals.css
  tailwind.config.js

P2 浮层基础设施
  src/lib/overlay-sizes.ts
  src/components/ui/iris-overlay.tsx
  src/hooks/useOverlayManager.ts
  src/components/ui/dialog.tsx          # 与 IrisOverlay 对齐

P3 面板迁移（每步：替换 SidePanel → IrisOverlay，删 aiPanelOpen）
  3a src/components/file/SearchPanel.tsx
  3b src/components/file/FileSheet.tsx
  3c src/components/settings/SettingsPanel.tsx
  3d src/components/file/BacklinksPanel.tsx
  3e src/components/tag/TagView.tsx
  3f src/components/version/VersionTimeline.tsx   # size=wide
  3g src/components/graph/GraphView.tsx           # size=graph
  3h src/components/file/QuickOpen.tsx            # size=compact
  3i src/App.tsx                                  # overlays 接线，移除 aiPanelOpen 透传
  3j 删除或废弃 src/components/ui/side-panel.tsx

P4 纸页视口（方案甲）
  src/styles/globals.css                          # .iris-paper、暗纸 A、滚动
  src/components/editor/TipTapEditor.tsx
  src/components/layout/WelcomeEmpty.tsx

P5 Chrome + AI
  src/components/layout/TabBar.tsx
  src/components/layout/StatusBar.tsx
  src/components/layout/AppShell.tsx              # z-index
  src/components/ai/AiPanel.tsx
  src/components/editor/FloatingToolbar.tsx
  src/components/ui/button.tsx, input.tsx, ConfirmDialog.tsx

P6 验收
  pnpm run lint && pnpm run typecheck && pnpm run test
  手工：AI 展开 + Ctrl+Shift+F/V/G；纸页首屏满高；暗色纸面；Esc；reduced-motion
```

### 单 PR 建议切分（便于 review）

| PR   | 范围                                             |
| ---- | ------------------------------------------------ |
| PR-1 | P1 + P2（token + IrisOverlay + overlay manager） |
| PR-2 | P3（全部面板迁移）                               |
| PR-3 | P4 + P5（纸页 + chrome/AI）                      |
| PR-4 | P6 + ROADMAP checklist 勾选                      |

---

## 三、z-index 与层叠（实现时遵守）

| 层                       | z-index 建议   | 内容                                   |
| ------------------------ | -------------- | -------------------------------------- |
| 编辑画布                 | 0              | 纸页、浮动工具条                       |
| AI 侧栏                  | 10             | 常驻 dock                              |
| 全屏 scrim               | 40             | 命令浮层蒙层（盖住 AI，不 unmount AI） |
| 命令浮层内容             | 50             | IrisOverlay 卡片                       |
| Quick Open / 冲突 Dialog | 50（同层互斥） | 与命令浮层二选一                       |

---

## 四、验收清单（与 ROADMAP 同步）

- [ ] `Ctrl+Shift+*` 打开的为居中圆角浮层，非右侧长条
- [ ] AI 展开时打开浮层：编辑区宽度不变，仅 dim；AI 仍可见于蒙层下
- [ ] 同时仅一个命令浮层；打开 B 自动关闭 A
- [ ] 空文档纸页首屏 ≥ 可用写作高度，纸边四边可见；滚动仅发生在纸内
- [ ] 暗色主题：暗暖灰纸 + 浅字，对比度 ≥ 4.5:1（正文）
- [ ] 版本 `wide`、图谱 `graph` 达近全屏 / 几乎全屏
- [ ] TabBar / StatusBar / AI 气泡圆角与动效与 design-system 一致
- [ ] `prefers-reduced-motion`：无 scale 或 instant
- [ ] `pnpm run lint` / `typecheck` / `test` 通过

---

_最后更新：2026-05-26_
