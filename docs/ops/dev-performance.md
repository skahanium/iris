# 开发环境性能建议

## 启动

- 只保留**一个** `npm run dev:desktop`（或 `npm run tauri dev`）终端。
- 不要同时用浏览器单独打开 `http://127.0.0.1:1420`，避免重复 Vite 实例。
- 开发时避免并行 `npm run test:watch`，会叠加第二套文件监听。

## Vault 目录

- 笔记库放在**本地非同步**路径（勿用 iCloud Desktop、OneDrive 根目录）。
- 大库首次打开会触发增量索引；等待状态栏「笔记库已同步」后再密集使用 AI 检索。

## 可选环境变量

| 变量                     | 作用                                                          |
| ------------------------ | ------------------------------------------------------------- |
| `VITE_SKIP_AUTO_INDEX=1` | 开发时跳过启动自动 `index_rescan`（需手动在命令面板重建索引） |

## 与生产差异

开发态会额外运行 **Node（Vite）** 与 **Cargo 监视重编译**；Activity Monitor 里 `node` 偏高属正常。验证生产负载请使用 `npm run tauri build` 后的 `.app`。

详见 [performance-baseline.md](./performance-baseline.md)。
