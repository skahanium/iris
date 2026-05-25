# 品牌资源

| 文件 | 说明 |
|------|------|
| `app-icon.png` | 应用图标源图（32×32），由 `scripts/gen-icon.mjs` 生成 |

生成 Tauri 图标集：

```bash
node scripts/gen-icon.mjs
npx tauri icon scripts/assets/app-icon.png -o src-tauri/icons
```

根目录不应再放置 `app-icon.png`（已加入 `.gitignore`）。
