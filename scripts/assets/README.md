# 品牌栅格资源

由 `npm run icon:gen` 从几何 monogram v3 生成。

| 文件 | 尺寸 | 用途 |
|------|------|------|
| `app-icon.png` | 1024×1024 | 亮色底、大「I」（无内框），**Tauri / 任务栏主源图** |
| `app-icon-dark.png` | 1024×1024 | 暗色底 + 方框 monogram（备用） |
| `app-icon-light.png` | 1024×1024 | 亮色底 + 方框 monogram |
| `iris-mark-transparent.png` | 512×512 | 透明底（亮色 UI 色） |
| `tray-icon-16.png` | 16×16 | 系统托盘 |
| `tray-icon-22.png` | 22×22 | macOS 菜单栏 |
| `tray-icon-32.png` | 32×32 | Windows 托盘 |

```bash
npm run icon:gen
npm run icon:tauri
```

矢量见 `public/brand/` 与 `src/components/brand/IrisMark.tsx`。设计说明见 [docs/design-system/brand.md](../../docs/design-system/brand.md)。
