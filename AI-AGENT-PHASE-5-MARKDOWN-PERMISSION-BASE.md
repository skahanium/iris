# Phase 5: Markdown Permission Base

> 状态：规划草案  
> 目标：建设 Iris 最终版 Markdown Agent 权限底座，而不是通用电脑控制 Agent。

## 1. 定位

Iris 的 Agent 服务对象是本地 Markdown vault。权限底座应围绕 Markdown 工作台建设：

- vault 读写
- Markdown patch
- assets 管理
- 外部资料选择式导入
- 网页资料采集
- 文档转换/OCR/PDF 提取
- 受控 shell/git 高级能力
- skill sandbox

不默认建设：

- 全局桌面控制
- 任意全盘文件写入
- 任意 shell
- 系统环境变量读取
- 明文凭据读取
- 全局键鼠模拟

## 2. 权限原子

### Vault

- `vault.read`
- `vault.search`
- `vault.write.patch`
- `vault.create_note`
- `vault.rename_move`
- `vault.delete_to_trash`
- `vault.assets.read`
- `vault.assets.write`
- `vault.versioning`

要求：

- AI 写入 Markdown 必须走 patch。
- rename/move 必须预览 backlinks/wikilinks 影响。
- delete 默认进回收站。
- 写入前生成快照，可回滚。

### 外部文件

- `fs.pick_file`
- `fs.pick_folder`
- `fs.import_to_vault`
- `fs.export`
- `fs.read_authorized_folder`
- `fs.write_authorized_export`

要求：

- 外部文件访问来自用户选择或授权目录。
- 外部写入默认仅 export。
- 不默认开放任意 external delete/move。

### 文档处理

- `doc.convert`
- `doc.ocr`
- `doc.extract_pdf`
- `doc.extract_table`
- `doc.normalize_markdown`
- `doc.fix_links`
- `doc.extract_citations`

要求：

- 优先提供专用工具，不先开放裸 shell。
- 转换结果写入临时区、assets 或用户确认的目标路径。

### Web

- `web.search`
- `web.fetch`
- `web.to_markdown`
- `web.download_to_assets`
- `web.citation_extract`
- `net.localhost`

要求：

- HTTPS 优先。
- 下载只能到临时区或 vault assets。
- 登录态网页读取需明确提示。
- localhost 主要用于预览、开发和本地服务集成。

### Skill Runtime

- `skill.read_resource`
- `skill.write_storage`
- `skill.request_capabilities`
- `skill.execute_script_sandboxed`
- `skill.install_dependency`
- `skill.mcp_bridge`

要求：

- script execution 默认关闭。
- sandbox 必须限制 cwd、env、timeout、stdout/stderr 大小。
- dependency install 单独高风险确认。

### Process / Git

- `process.run_markdown_tool`
- `process.run_readonly`
- `process.run_mutating`
- `process.run_network`
- `process.long_running`
- `process.kill_owned`
- `git.read_status`
- `git.read_diff`
- `git.read_log`
- `git.write_commit`

要求：

- 命令风险分类器先判断 readonly/mutating/network/package-manager。
- cwd 限制在 vault 或用户授权 workspace。
- env 默认最小化并脱敏。
- 长期进程必须可见、可停止、不会跨会话静默常驻。

### Clipboard / Browser

- `clipboard.write`
- `clipboard.read`
- `browser.read_page`
- `browser.screenshot`
- `browser.control_page`

要求：

- clipboard read 每次确认或显式会话授权。
- browser control 默认关闭，仅在网页采集或本地预览测试中启用。
- 不默认使用用户登录态 cookies/session。

### Secrets

- `secret.exists`
- `secret.use_named`
- `secret.create_update`

禁止默认支持：

- `secret.read_plaintext`

要求：

- skill 和模型不能拿到明文 API Key。
- 只允许通过 Iris 后端代用 named credential。

## 3. 授权模型

授权作用域：

- per request
- per session
- per vault
- per folder
- per skill
- global default

授权决策：

- allow
- allow once
- allow for session
- deny once
- deny always for this skill
- open settings

风险等级：

- low：vault read、search、status。
- medium：vault patch、web fetch、assets write。
- high：external write、delete/move、script execution、git commit、clipboard read。
- critical：package manager、network command、persistent process、secret update。

## 4. UI 与用户体验

设置页新增“Markdown Agent 权限”：

- Vault
- 外部文件
- 文档处理
- Web
- Skills
- Shell/Git
- Clipboard/Browser
- Secrets

AI 执行时展示：

- 本轮需要哪些权限。
- 哪些自动允许。
- 哪些需要确认。
- 哪些被阻断。
- 是否有替代方案。

确认弹窗必须显示：

- 工具名
- 权限名
- 作用域
- 风险等级
- 将读取/修改的路径或域名摘要
- 可撤销方式

## 5. 安全与审计

审计记录：

- request id
- skill id
- tool name
- permission name
- decision
- scope summary
- timestamp
- result status

不记录：

- API Key
- note body
- external file body
- image base64
- clipboard body
- screenshot content
- full shell output containing sensitive data

## 6. 与前面阶段的关系

- Phase 1 提供 PermissionPreflight 和 Audit 框架。
- Phase 2 通过单一入口触发权限请求。
- Phase 3 确保模型和人格不能越权。
- Phase 4 让 skill manifest 映射到这些权限原子。
- Phase 5 扩展真实工具实现和授权 UI。

## 7. 测试计划

- vault patch 写入必须生成 diff、确认、快照。
- rename/move 展示 link impact。
- delete 进入回收站。
- external file 必须通过 picker 或授权目录。
- web download 只能进入临时区或 assets。
- script sandbox 限制 cwd/env/timeout/stdout。
- git commit 必须确认。
- clipboard read 必须确认。
- secret plaintext read 不可用。
- audit 不泄露敏感正文。

## 8. 验收标准

- Iris 能运行面向 Markdown 工作台的高级 skills。
- Hermes skill 缺少通用电脑控制能力时能明确解释，而不是沉默失败。
- 用户能理解并控制 Agent 本轮要读什么、写什么、运行什么。
- 权限体系不背离 Iris 的本地 Markdown 产品定位。
