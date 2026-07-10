# 安全策略

## 支持版本

当前开发版本 `v1.2.6` 处于活跃支持状态。历史标签的修复通过后续发布交付；请使用最新版本验证问题。

## 报告漏洞

请勿公开提交安全漏洞。发送邮件至 `skahanium@gmail.com`，附受影响版本、复现步骤、影响和可选修复建议。项目目标是在 48 小时内确认收到，并在 5 个工作日内给出初步评估。

## API Key 与凭据

Iris 使用本地 **AES-256-GCM** 加密凭据存储，刻意不调用 Windows Credential Manager、macOS Keychain 或 Linux Secret Service，避免系统密码弹窗打断常规 LLM/MCP 使用。

- 每条凭据使用随机 12 字节 nonce，服务名作为 AAD；密文以 Base64 记录在本地凭据文件中。
- 32 字节主密钥由 `OsRng` 生成，保存在平台配置目录；密文位于应用数据目录，两者分离。
- macOS/Linux 使用私有目录/文件权限；Windows 仅允许当前用户访问。
- 解密值以 `Zeroizing<String>` 保存，离开作用域自动清零。
- API Key 永不写入明文文件、SQLite、日志、错误消息或环境变量；仅在 HTTPS 请求的授权头中短暂使用。

## 数据与网络边界

- 用户笔记是标准 UTF-8 `.md` 文件；Iris 不提供 Vault 目录级加密。需要静态磁盘保护时，请使用 BitLocker、FileVault 或 LUKS。
- LLM 和自定义 provider 必须使用 HTTPS；`http://` endpoint 会被配置边界拒绝。
- 文件操作进行 Vault 路径校验；数据库查询参数化；渲染 HTML 经 DOMPurify 清理。
- 日志、诊断与错误中不得包含 API Key、token、用户笔记正文、完整 prompt 或原始模型思维链。

## 依赖安全

提交前可运行 `npm audit` 与 `npm run audit:rust`。发现高风险依赖漏洞时，应优先升级或隔离受影响路径，并在 CHANGELOG 的已发布版本中记录安全修复。
