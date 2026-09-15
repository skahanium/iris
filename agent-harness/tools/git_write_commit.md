# git_write_commit

<!-- iris:object T27 kind=rules file=true -->

`git_write_commit` 是 Iris 的内置业务工具之一（扩展能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T27 -->

<!-- iris:object T27 kind=tool name=git_write_commit owner=M02 -->

## 用途与语义归属

**在显式任务与确认范围内执行一次 Git 提交**：对调用方列出的 Vault 相对路径执行 `git add`，再以固定作者身份创建一次 commit，并返回短哈希。语义归属为工具／Git 服务，责任模块 `M02`（权限与受控副作用）。目标合同原文是「显式任务与确认范围内执行」——它不是自动保存，也不把「已提交」当成任务成功的证明；提交回执只是副作用事实，任务成功仍由 `C25` 依据 `C23` 报告与回执发布。

与同类只读工具的分工：`T17` `git_read_status`、`T18` `git_read_diff`、`T19` `git_read_log` 读取状态、差异与历史；本工具是这一组里唯一的写动作。

## 参数与消费

目录 schema：`message`（string，**required**）、`paths`（string 数组，**required**）。两者都被处理器消费：

- `message`：`trim()` 后必须非空且长度 ≤ 500 字节；否则错误 `commit message must be 1..500 bytes`。长度按字节计，中文消息按 UTF-8 字节数计算。
- `paths`：必须是字符串数组（数组内含非字符串元素时报 `paths entries must be strings`）；空数组报 `paths are required`。逐项校验：拒绝含 `..` 的父目录组件（`Path traversal is not allowed`）、拒绝绝对路径（`absolute paths are not allowed`）、拒绝保留元数据路径（`.iris/` 等，`is_user_note_path` 为假时报「内部元数据路径不允许用于此工具」）。
- 无声明未消费的参数。

执行顺序：逐路径 `git add -- <path>`（每个路径一条命令，带 `--` 分隔），随后一次 `commit -m <message>`，最后 `rev-parse --short HEAD` 读取短哈希。

返回：`{ "type": "git_write_commit", "commit": <短哈希>, "paths": [<已提交路径>], "sandbox_profile": {…} }`。

## 授权

- 能力合同（`C16`）：`required_capability_ids() = ["git.write"]`；授权校验按精确能力 ID，访问等级（`WriteMarkdown`）不是授权来源。
- 权限原子（`C04`）：`Atom::GitWriteCommit`（`git.write_commit`），Risk = **High**，`supported = true`。
- 确认要求：目录 `requires_confirmation = true`，风险等级为 High，因此决策为 `RequiresConfirmation`：调用先冻结为变更计划，由用户确认后才执行（`C05`、`K05`）。High 风险不能以 Session 范围授权长期放行（`upsert_permission_grant` 拒绝 `AllowForSession` + High/Critical）。
- 不需要联网；不受联网开关影响。Git 子进程只作用于当前 Vault 工作目录。
- 子任务：本工具不在 `C15` 的 `child_tool_surface` 白名单内，子运行不获得它——与「一层委派、只读」的目标一致（架构 §3 M5 的 `C15` 合同不以本工具为例，本卡的结论来自源码白名单）。

## 副作用

**有真实副作用，且不可逆**：在 Vault 仓库内创建一次 commit（写入 Git 对象库与 `HEAD`/索引）。它不改写用户笔记文件内容，但改变仓库历史。

目标绑定与版本复验的当前事实：

- 冻结阶段：参数没有 `target_path`／`path`／`new_path`，因此 `frozen_relative_paths` 把目标记为 `application://tool/git_write_commit`；没有 `base_content_hash`，也不产生 `expected_post_content_hashes`（[run_tool_loop.rs](../../src-tauri/src/ai_runtime/run_tool_loop.rs)）。
- 执行阶段：逐操作执行，失败即停止后续操作（已完成前缀保留）；执行前复核冻结的基准 hash 对（本工具为空集，因此没有文件 hash 漂移检查门槛）。
- **事实边界**：确认范围绑定的是工具调用与参数（路径清单、消息），不是「这些路径当时的文件内容哈希」。因此参数范围内文件在确认后发生的改动会被一并提交——这与 `T15`／`T16` 的「基线 hash + 范围复验」不是同一强度，属**当前实现的绑定事实**，本卡不声称它满足内容保持类合同。
- 子进程边界：`git` 以 `core.hooksPath=/dev/null`、`filter.lfs.smudge=` 等配置运行并 `env_clear()`，作者固定为 `Iris Agent <iris-agent@example.invalid>`；沙箱画像是 L1 子进程级（`env_cleared`、`cwd_fixed`、`argument_allowlist`、`git_hooks_disabled`、`git_filters_disabled`），**不是 OS 沙箱**。
- 部分失败：`git add` 在任一路径失败时报 `git add failed` 并中止，不做部分提交；`commit` 失败时报 `git commit failed: <截断 stderr>`，不重放。
- 审计：派发结果经 `C17` 的统一派发审计记录（工具名、成功状态、耗时、安全摘要），不保存仓库正文。

## 预算

- `requires_confirmation = true` ⇒ `ToolBudgetClass::ConfirmedChange`（[capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs)）。在确认批次判定里，**整批必须都是需确认调用**，混合批次直接以 `mixed_confirmation_batch` 拒绝。
- 当前预设 `max_confirmed_change_calls = 6`（三预设同值，[run_contract.rs](../../src-tauri/src/ai_runtime/run_contract.rs)）：这是主循环分类额度，不是全局统一总数，数值权威在 `K06`。
- 单次派发包裹在 30 秒超时内（`DEFAULT_TOOL_DISPATCH_TIMEOUT`）。Git 子进程本身没有独立的、按仓库大小自适应的时限声明；超时会让整个派发返回 `tool_dispatch_timeout`，**已产生的副作用不会因此回滚**——这一点必须按「结果未知的副作用」对待。
- 不在自动重试白名单内（白名单只含 `web_search`／`web_fetch`）。
- 目录 `max_results = None`；`is_discovery()` 为 false，不占发现调用配额。

## 幂等

**不幂等**：同一 `message` + `paths` 重复执行会创建第二个 commit（只要仍有可提交内容）。

当前可依赖的机械保护（不是本工具的幂等设计，而是循环与 Git 的既有行为）：

- 同 Run 内已成功执行的同指纹调用返回 `tool_call_already_succeeded`；同回合内同指纹超过 `MAX_REPEAT_CALLS` 返回 `tool_call_repeated`。
- 若暂存区无变化，`git commit` 以非零退出，报 `git commit failed: …`，不会静默产生空提交。
- 确认计划被消费后不能重放：执行以 checkpoint 推进（`Dispatching` → `Applied`），进程恢复只继续未执行后缀（[run_tool_loop.rs](../../src-tauri/src/ai_runtime/run_tool_loop.rs) 的 `execute_confirmed_frozen_change_set`）。

**去重键**：当前没有调用方可见的去重键。目标合同未提出提交去重语义；若需要「同一变更集只提交一次」，属于待明确的新合同，本卡不代替它作承诺。

## 取消

取消在派发入口生效：`dispatch_tool_inner` 首先执行 `ctx.ensure_run_active()`，已取消的 Run 不进入处理器。取消信号同时进入写盘前的提交检查（[ARCHITECTURE.md](../../ARCHITECTURE.md) 的 Agent Run 节）。

**已开始的 Git 子进程没有中断检查点**：`std::process::Command::output()` 同步等待，取消不能打断已在运行的 `git add`／`git commit`。因此可能出现「用户已取消、但 commit 已生成」的结果——必须按事实报告，不假装回滚；不得因重试造成重复提交（架构 §3 M5：结果未知的非幂等副作用不进入盲目自动重试）。

## 失败反馈

- **可恢复（参数类）**：消息为空或超长、路径为空、路径含 `..`、绝对路径、`paths` 元素非字符串，都返回稳定错误码或明确消息，模型可修正后重提。缺字段或类型不符在执行门返回 `{"error":"tool_arguments_invalid"}`（**不是字段级说明**，字段级反馈属 `G01` 目标）。
- **可恢复（环境类）**：`git` 不存在或工作目录非仓库时 `Command::output()` 失败／非零退出，反馈为 `git command failed: <截断 stderr>`，属外部环境问题；是否重试由 `C14` 依据「是否已有实质修正」判断，不由本工具自行重试。
- **需用户操作**：未确认前不执行；已有有效授权不反复索要（`K05`）。
- **不可恢复**：仓库损坏、路径校验与真实布局不一致等，不做自动修复，如实报告。
- **用户侧表达**：提交失败不得表达为「模型能力降级」（`N17`）；部分成功必须如实说明哪些路径已提交、哪些未提交，不能因为 `paths` 参数完整就声称全部完成。
- 归因：Git 错误字符串进入工具错误消息前会被截断（≤400 字符），并按 `C26`／`C27` 的脱敏要求处理，不把仓库正文写入日志或诊断。

## 暴露规则

- 目录 `default_enabled_without_skill = false`；不在核心无技能只读名单内。
- 目标工具面：扩展目录项，「仅在任务需要、能力启用且授权满足时进入工具面」（架构 §6.3）；同类只读工具的目标合同写作「仅在 Git 任务中暴露」，本工具属同一族。
- 实际门禁是 Run 冻结能力：需要 `git.write`（`C16` 的 `is_authorized_by`）。
- **暴露缺口（本卡核对所得，尚未登记为 `G*`）**：基线 `2670739f` 中 `git.write` 没有找到任何授予位置；`RunIntake` 加入的能力清单不含它，而 `T17`／`T18`／`T19` 需要的 `git.read` 同样如此（[run_intake.rs](../../src-tauri/src/ai_runtime/run_intake.rs)）。按当前路径，本工具虽为 `Dispatchable`，**可能无法进入任何 Run 的工具面**。该结论限于静态核对，**待核对**是否存在其他授予入口。
- 子任务不暴露（见「授权」）。Planned 项不暴露；本工具不是 Planned。

## 源码落点

- 目录定义：[tool_catalog/boundary.rs](../../src-tauri/src/ai_runtime/tool_catalog/boundary.rs)（`git_write_commit`：`WriteMarkdown`、`requires_confirmation = true`、`Dispatchable`、`max_results = None`）。
- 能力与预算分类：[tool_catalog/capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs)（`git.write`、`budget_class`）。
- 权限画像：[agent_permissions.rs](../../src-tauri/src/ai_runtime/agent_permissions.rs)（`git_write_commit` ⇒ Risk::High）。
- 处理器：[tool_dispatch/boundary.rs](../../src-tauri/src/ai_runtime/tool_dispatch/boundary.rs)（`git_write_commit_tool`、`validate_vault_relative_path`、`string_array_arg`、`run_git`）。
- 路径与沙箱：[storage/paths.rs](../../src-tauri/src/storage/paths.rs)（`is_user_note_path`）、[sandbox_profile.rs](../../src-tauri/src/ai_runtime/sandbox_profile.rs)（L1 画像）。
- 确认与冻结路径：[run_tool_loop.rs](../../src-tauri/src/ai_runtime/run_tool_loop.rs)（`request_change_set`、`freeze_change_operation`、`frozen_relative_paths`、`execute_confirmed_frozen_change_set`）。
- 现有测试：[src-tauri/tests/agent_permission_boundaries.rs](../../src-tauri/tests/agent_permission_boundaries.rs) 的 `git_write_commit_only_commits_explicit_vault_paths`（真实 `git init` + 提交成功 + `../outside.md` 被拒）；[src-tauri/tests/ai_agent_phase9_10_contracts.rs](../../src-tauri/tests/ai_agent_phase9_10_contracts.rs) 的 `high_risk_tools_have_honest_sandbox_profiles` 与 `subprocess_sources_apply_l1_constraints_and_run_audit_identity`（源码级约束存在性检查）。

## 当前状态

`implementation.state = present`（静态源码事实：目录项 `Dispatchable`，处理器、路径校验与 `git` 调用在位）。`verification.state = none`。

已知边界：确认绑定参数而非文件内容哈希（见「副作用」）；能力授予缺口见「暴露规则」；无去重键。**本卡不声明 Git 提交行为已通过真实用户任务验收**，也不把上面的既有测试当作合同正确性证明。

## 相关合同

- `K05` 授权范围与冻结确认：确认身份、范围与版本复验；本工具的确认路径来自该合同。
- `K06` 统一预算账本：`ConfirmedChange` 分类额度的唯一权威。
- `K17` 审计事件：派发结果的关联记录。
- 相邻工具：`T17` `git_read_status`、`T18` `git_read_diff`、`T19` `git_read_log`（只读同类）；`T15` `insert_text_at_cursor`／`T16` `replace_selection`（真正的笔记写入动作）。
- 需求依据：本工具由架构定义 §6.3 的扩展目录定位给出；用户侧要求「严格场景保留联网限制、授权覆盖子任务」等边界见 `N12`。
- 依据文档：[docs/agent-architecture.md](../../docs/agent-architecture.md) §3 M2、§6.3、§8.5（文件发生意外修改的定位路径）。

<!-- iris:end T27 -->
