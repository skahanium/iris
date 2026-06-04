# 统一桌面顶栏设计说明

**日期**：2026-05-30  
**状态**：已实现（统一顶栏 + macOS 32px 交通灯对齐）

## 目标

1. 消除「系统标题栏 + 应用 Tab 栏」双层冗余，仅保留一行 `DesktopTitleBar`。
2. **禁止**在窗口任意位置出现「Tauri App」文案；对外名称统一为 **Iris**。
3. macOS / Windows 均达到现代桌面应用顶栏观感；顶栏高度按平台原生比例区分。

## 架构

- **前端**：[`DesktopTitleBar.tsx`](../../../src/components/layout/DesktopTitleBar.tsx) 合并原 `TabBar` 与 `MinimalWindowChrome`；`variant: document | splash`。
- **平台**：[`platform-chrome.ts`](../../../src/lib/platform-chrome.ts) — macOS 使用系统交通灯，Windows/Linux 使用 [`WindowControls`](../../../src/components/layout/WindowControls.tsx)。
- **指标 SSOT**：[`chrome_metrics.rs`](../../../src-tauri/src/chrome_metrics.rs) + IPC `get_desktop_chrome_metrics`；[`useMacOSWindowChromeSync.ts`](../../../src/hooks/useMacOSWindowChromeSync.ts) 写入 CSS 变量并在退出全屏时 `reapply_window_chrome`。
- **后端**：[`window_chrome.rs`](../../../src-tauri/src/window_chrome.rs) 启动时 `set_title("Iris")`；[`macos_traffic_lights.rs`](../../../src-tauri/src/macos_traffic_lights.rs) 将 Overlay 容器固定为 32px 并垂直居中交通灯。
- **配置**：[`tauri.macos.conf.json`](../../../src-tauri/tauri.macos.conf.json) — `trafficLightPosition` 初值 `x:12, y:10`。

## Token（平台表）

| 平台            | `--titlebar-height` | `--titlebar-traffic-inset`           |
| --------------- | ------------------- | ------------------------------------ |
| macOS           | `2rem`（32px）      | `72px`（约 `4.5rem`），可由 IPC 覆盖 |
| Windows / Linux | `2.5rem`（40px）    | `0`                                  |

## 全屏 vs 窗口模式

| 模式                   | 交通灯位置                                                                      | 应用顶栏                            |
| ---------------------- | ------------------------------------------------------------------------------- | ----------------------------------- |
| **窗口**               | AppKit Overlay 标题容器 = 32px；与 `DesktopTitleBar` 同一中线（`items-center`） | 对齐目标                            |
| **全屏（悬停菜单栏）** | 灯在系统菜单栏黑条内，由 macOS 居中                                             | **不追求**与 `DesktopTitleBar` 对齐 |

退出全屏后必须重新 `inset` 交通灯并刷新 CSS 变量，否则窗口模式可能错位。

## 验收

- [ ] `npm run tauri dev` 下仅一行顶栏，无「Tauri App」
- [ ] **macOS 窗口模式**：交通灯与 Iris（无 Tab）/ Tab 文字（有 Tab）同一水平线；顶栏约 32px
- [ ] **macOS**：全屏悬停菜单栏灯可不对齐；退出全屏后窗口模式仍对齐
- [ ] **Windows**：顶栏仍为 40px；右侧自定义三键与 Tab 无回归
- [ ] 有 Tab 时无宽 `AppBrandZone`；无 vault 时 `splash` 变体保留品牌区
