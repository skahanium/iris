# 安全策略

> 架构与安全实现见 [ARCHITECTURE.md](./ARCHITECTURE.md)；文档索引见 [docs/README.md](./docs/README.md)。

## 支持版本

| 版本   | 支持状态                         |
| ------ | -------------------------------- |
| latest | :white_check_mark: 活跃支持      |
| < 1.0  | :warning: 开发阶段，安全保证有限 |

在 v1.0 正式发布之前，安全更新将直接包含在新版本中。

## 报告漏洞

如果你发现了安全漏洞：

1. **不要公开提交 Issue。**
2. 发送邮件到 [待补充] 并附上：
   - 受影响版本号
   - 漏洞的详细描述和复现步骤
   - 建议的修复方案（如果有）
3. 我们将在 48 小时内确认收到报告，并在 5 个工作日内提供初步评估。

我们承诺：

- 在修复发布前，不会公开讨论漏洞
- 在修复后，会在 CHANGELOG 中鸣谢报告者（除非你要求匿名）
- 不会对善意测试和报告漏洞的研究者采取法律行动

## Iris 的安全边界

理解以下安全边界有助于你判断某个行为是否真的是漏洞。

### 在安全边界内（无需报告）

- 本地文件系统操作（应用运行在你自己的机器上，对本地文件有完全读写权限）
- SQLite 数据库内容（存储在本机的纯数据文件）
- 用户自行配置的 LLM API endpoint（由用户自己选择的服务）

### 在安全边界外（需要报告）

- 未经加密传输的 API Key 或敏感数据
- 日志或调试输出中泄露的密钥、Token、用户笔记内容
- 通过应用 UI 能够访问到系统其他位置的未授权文件
- 远程代码执行（通过导入文件、打开链接等触发）
- 依赖项引入的已知 CVE

## API Key 处理

- Iris 使用操作系统原生凭据管理器存储 API Key（Windows Credential Manager / macOS Keychain / Linux Secret Service）
- API Key 永不写入明文到磁盘文件、日志或数据库
- 应用启动时从凭据管理器读取，仅保存在内存中
- 向 LLM API 发送请求时，Key 仅出现在 HTTPS 请求的 Authorization Header 中

## 笔记静态存储

Iris **不提供** Vault 目录级加密。笔记以明文 `.md` 文件存储在用户选择的目录中，与「文件即数据、任意编辑器可打开」的产品原则一致。若需额外保护，请使用操作系统级全盘加密（Windows BitLocker / macOS FileVault / Linux LUKS）。详见 [ROADMAP § 产品原则与非目标](./ROADMAP.md#产品原则与非目标)。

## 依赖安全

- 推荐在本地定期运行 `cargo audit`（Rust）与 `npm audit`（Node.js）
- CI 当前运行 fmt / clippy / test 与 lint / typecheck / test；安全审计接入 CI 为后续改进项
- 发现高危漏洞的依赖应尽快升级或替换

## 已公开漏洞

暂无。此部分将在首次漏洞修复后更新。
