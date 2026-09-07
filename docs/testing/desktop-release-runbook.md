# 桌面发版手册

本手册描述当前可执行的打包路径。官方平台仅 **macOS ARM64** 与 **Windows x64**。

## 两段式门禁

1. **有界 push CI**（`pull_request` / `main` push）：前端质量、macOS ARM64 Rust fmt/clippy/test/audit、Agent 24-case smoke。
2. **同 SHA 手动发布就绪**（`ci.yml` 的 `workflow_dispatch`）：
   - `npm run agent:eval:contract`
   - `model:prepare` 后的 `embedding_model_smoke`
   - 供应模型 RAG 质量门与 50k sqlite-vec 性能门
   - Windows x64 桌面持久化 E2E
3. **打包**（tag `v*` 或手动 `package-desktop.yml`）：要求该 SHA 已是 `origin/main` 祖先，且存在成功的 **push** CI 与成功的 **workflow_dispatch** CI。随后产出 macOS DMG / Windows NSIS 与 updater 签名物，tag 路径再生成 **draft** Release。

缺少任一门禁不得产出安装包。

## 产品门（不阻断打包）

`npm run agent:eval` 需要两份绝对路径的 live v4 报告与一份人工审阅文件。HR-7 在该门通过前保持「实测未通过」。

- 可以打 draft 安装包做内部验证。
- **不得**在发行说明或 CHANGELOG 中把 Agent 写成已放行。
- 对外宣称 v1.3.0 Agent 质量通过前，必须完成双路 live Campaign 与人工评分。

## 签名边界

当前 `package-local` 使用 macOS ad-hoc 签名，Windows 安装包不带 Authenticode。GitHub 打包只保证 Tauri updater 私钥签名（`TAURI_SIGNING_PRIVATE_KEY`）。面向最终用户的 Developer ID、公证与 Authenticode 不在本流水线内；未配置这些证书时，包仅适合自用或明确知情的内测。

## 操作顺序

1. 合并到 `main`，确认该 commit 的 push CI 全绿。
2. 在同一 commit 上手动 Dispatch `CI` workflow，等待发布就绪与 Windows E2E 全绿。
3. 打 `vX.Y.Z` tag（须先 `npm run version:set` / `version:check`），或手动 Dispatch `Package Desktop`。
4. 检查 draft Release 资产、`latest.json` 与签名文件。
5. 发布 Release 后由 `verify-release.yml` 校验 updater 指针。非 prerelease 才更新 GitHub `latest`。
