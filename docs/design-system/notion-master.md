# Iris · N · Notion 编辑（设计参考）

> 由 v0.4.0-ui 定稿；实现以 `src/styles/globals.css` 与 [design-system.md](../design-system.md) 为准。

## 气质

内容优先、扁平灰阶壳层；编辑区与外壳同色，无浮动纸页与行线网格。AI 为右侧 280px 校对台。

## 色彩（HSL 分量）

| Token | 暗色 `:root` | 亮色 `.light` |
|-------|--------------|---------------|
| `--background` | `0 0% 10%` | `0 0% 100%` |
| `--foreground` | `0 0% 93%` | `0 0% 13%` |
| `--primary` | `210 18% 62%` | `210 12% 45%` |
| `--border` | `0 0% 18%` | `0 0% 90%` |

`--editor-paper` / `--editor-ink` 与 `--background` / `--foreground` 对齐（兼容旧类名）。

## 字体

- UI 与正文：`Inter` + 系统无衬线（`font-sans`）
- 代码：`font-mono`

## 圆角

`4px` / `6px` / `8px` / `12px`（浮层）

## 反模式

- 衬线正文、信纸行线、段首缩进、纸页阴影卡片
- 赭铜 accent、高饱和紫渐变
