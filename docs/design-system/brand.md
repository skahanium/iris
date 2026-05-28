# Iris 品牌标识 · 几何 monogram v3

> **N · Notion** 灰阶：圆角方框 + 斜切衬线大写「I」；全平台同一套 SVG 几何。无渐变、无彩色 accent、无墨迹手写。

## 图形规格（viewBox `0 0 32 32`）

| 元素   | 参数                                |
| ------ | ----------------------------------- |
| 外框   | `x=2 y=2 w=28 h=28 rx=8`            |
| 上衬线 | `x=8.8 y=7.2 w=13.8 h=3.4 rx=1.7`   |
| 下衬线 | `x=10.2 y=21.4 w=11.6 h=3.4 rx=1.7` |
| 竖干   | 贝塞尔路径 `I_STEM_PATH`            |
| 字组   | `skewX(-7°)`，中心 `(16,16)`        |

## Token（`globals.css`）

| Token               | 暗色 `:root` | 亮色 `.light` |
| ------------------- | ------------ | ------------- |
| `--iris-mark-frame` | `0 0% 20%`   | `0 0% 89%`    |
| `--iris-mark-ink`   | `0 0% 94%`   | `0 0% 10%`    |

**桌面壳图标**（`app-icon.png`）：四角透明 + 圆角矩形灰底 `#e8e8e8` + 放大字组「I」（无内框）。应用内 UI 仍用 `IrisMark`（含方框）。PNG hex 见 `scripts/iris-mark-paths.mjs`。

## 使用场景

| 场景                    | 实现                                               |
| ----------------------- | -------------------------------------------------- |
| 顶栏、欢迎页            | `IrisMark` + 可选 Inter「Iris」文案                |
| favicon / 托盘          | `public/brand/iris-mark.svg`、`iris-mark-tray.svg` |
| Windows 任务栏 / 安装包 | `src-tauri/icons/icon.ico` ← `npm run icon:tauri`  |

## 路径源（单一真相）

| 文件                                                                                     | 说明                    |
| ---------------------------------------------------------------------------------------- | ----------------------- |
| [scripts/iris-mark-paths.mjs](../../scripts/iris-mark-paths.mjs)                         | 构建与 SVG 导出         |
| [src/components/brand/iris-mark-paths.ts](../../src/components/brand/iris-mark-paths.ts) | React（须与 .mjs 同步） |

## 生成

```bash
npm run icon:gen
npm run icon:tauri   # 写入 icons-staging 再同步到 icons/
```

**Windows 任务栏图标**嵌在 `iris.exe` 内，仅改 `icon.ico` 不够。更新图标后须：

1. **完全退出** Iris（含托盘）
2. 执行 `npm run icon:tauri`
3. **重新编译**：`npm run tauri build` 或重启 `npm run dev:desktop`（触发 Rust 重编）
4. 若任务栏仍显示旧「眼眸」：取消固定后重新固定，或从开始菜单新快捷方式启动

`icon:tauri` 先写入 `icons-staging/`，避免运行中进程锁定 `icons/` 导致 `icon.ico` 未更新。

## 禁止

- 墨迹手写、混排衬线「ris」、不可读连笔 wordmark
- 方框+竖条旧版（无衬线 I 结构）
- 纯黑 `#000`、蓝紫渐变

## 无障碍

装饰性默认 `aria-hidden`；需命名时 `<IrisMark title="Iris" />`。
