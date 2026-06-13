import { useState } from "react";

import {
  ArrowRight,
  Bot,
  ClipboardCheck,
  ClipboardList,
  FileCog,
  FileText,
  FolderOpen,
  Globe2,
  KeyRound,
  LockKeyhole,
  Puzzle,
  ShieldCheck,
  Sparkles,
  TerminalSquare,
  Wrench,
} from "lucide-react";

import { AiRulesPanel } from "@/components/ai/AiRulesPanel";
import { SkillsPanel } from "@/components/ai/SkillsPanel";
import { LlmRoutingSection } from "@/components/settings/LlmRoutingSection";
import { MinimaxSearchSection } from "@/components/settings/MinimaxSearchSection";
import { PersonaSettingsPanel } from "@/components/settings/PersonaSettingsPanel";
import { IrisOverlay } from "@/components/ui/iris-overlay";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";

interface AiSystemCenterPanelProps {
  open: boolean;
  onClose: () => void;
}

type AiSystemSection =
  | "models"
  | "search"
  | "persona"
  | "permissions"
  | "memory";

const AI_SYSTEM_SECTIONS: {
  id: AiSystemSection;
  label: string;
  detail: string;
}[] = [
  { id: "models", label: "模型路由", detail: "对话、写作、研究、Embedding" },
  { id: "search", label: "联网搜索", detail: "搜索后端、模型与可用性" },
  { id: "persona", label: "Persona", detail: "PromptProfile、表达风格与边界" },
  {
    id: "permissions",
    label: "Markdown Agent 权限",
    detail: "读写、联网、Shell 与凭据边界",
  },
  { id: "memory", label: "记忆与规则", detail: "用户规则、AI 记忆与确认" },
];

const PERMISSION_GROUPS: {
  title: string;
  detail: string;
  icon: typeof Sparkles;
  atoms: string[];
  policy: string;
  risk: "低" | "中" | "高" | "关键";
}[] = [
  {
    title: "Vault",
    detail: "面向当前 Markdown 库的读取、检索、补丁写入和版本快照。",
    icon: FolderOpen,
    atoms: [
      "vault.read",
      "vault.search",
      "vault.write.patch",
      "vault.create_note",
      "vault.rename_move",
      "vault.delete_to_trash",
      "vault.assets.read",
      "vault.assets.write",
      "vault.versioning",
    ],
    policy: "AI 写入必须走 patch，并在写入前生成可回滚快照。",
    risk: "中",
  },
  {
    title: "外部文件",
    detail: "只处理用户选择的文件、授权目录或明确导出的目标位置。",
    icon: FileText,
    atoms: [
      "fs.pick_file",
      "fs.pick_folder",
      "fs.import_to_vault",
      "fs.export",
      "fs.read_authorized_folder",
      "fs.write_authorized_export",
    ],
    policy: "不默认开放任意 external delete/move。",
    risk: "高",
  },
  {
    title: "文档处理",
    detail: "PDF 提取、OCR、表格抽取、Markdown 规范化和链接修复。",
    icon: FileCog,
    atoms: [
      "doc.convert",
      "doc.ocr",
      "doc.extract_pdf",
      "doc.extract_table",
      "doc.normalize_markdown",
      "doc.fix_links",
      "doc.extract_citations",
    ],
    policy: "转换结果进入临时区、assets 或用户确认的目标路径。",
    risk: "中",
  },
  {
    title: "Web",
    detail: "联网搜索、HTTPS 抓取、网页转 Markdown 和引用抽取。",
    icon: Globe2,
    atoms: [
      "web.search",
      "web.fetch",
      "web.to_markdown",
      "web.download_to_assets",
      "web.citation_extract",
      "net.localhost",
    ],
    policy: "登录态网页读取需要明确提示，下载只能到临时区或 assets。",
    risk: "中",
  },
  {
    title: "Skills",
    detail: "Skill 资源读取、本地存储、能力请求和受限脚本执行。",
    icon: Puzzle,
    atoms: [
      "skill.read_resource",
      "skill.write_storage",
      "skill.request_capabilities",
      "skill.execute_script_sandboxed",
      "skill.install_dependency",
      "skill.mcp_bridge",
    ],
    policy: "脚本执行默认关闭，依赖安装单独高风险确认。",
    risk: "高",
  },
  {
    title: "Shell/Git",
    detail: "受控命令、只读状态、diff/log 查看和 git commit。",
    icon: TerminalSquare,
    atoms: [
      "process.run_markdown_tool",
      "process.run_readonly",
      "process.run_mutating",
      "process.run_network",
      "process.long_running",
      "process.kill_owned",
      "git.read_status",
      "git.read_diff",
      "git.read_log",
      "git.write_commit",
    ],
    policy: "cwd 限制在 vault 或授权 workspace，env 最小化并脱敏。",
    risk: "关键",
  },
  {
    title: "Clipboard/Browser",
    detail: "剪贴板读写、本地预览网页读取、截图和受控页面操作。",
    icon: ClipboardList,
    atoms: [
      "clipboard.write",
      "clipboard.read",
      "browser.read_page",
      "browser.screenshot",
      "browser.control_page",
    ],
    policy: "clipboard read 每次确认或显式会话授权。",
    risk: "高",
  },
  {
    title: "Secrets",
    detail: "仅允许检查、代用或更新 named credential。",
    icon: KeyRound,
    atoms: [
      "secret.exists",
      "secret.use_named",
      "secret.create_update",
      "secret.read_plaintext",
    ],
    policy: "secret.read_plaintext 不支持；模型不能拿到明文 API Key。",
    risk: "关键",
  },
];

function SummaryTile({
  icon: Icon,
  label,
  value,
  detail,
}: {
  icon: typeof Sparkles;
  label: string;
  value: string;
  detail: string;
}) {
  return (
    <div className="rounded-md border border-border/60 bg-surface-inset/25 px-3 py-3">
      <div className="flex items-center gap-2 text-muted-foreground">
        <Icon className="h-3.5 w-3.5" />
        <span className="text-[11px] font-medium">{label}</span>
      </div>
      <p className="mt-2 text-sm font-semibold text-foreground">{value}</p>
      <p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">
        {detail}
      </p>
    </div>
  );
}

function ActionCard({
  icon: Icon,
  title,
  detail,
  action,
  onClick,
}: {
  icon: typeof Sparkles;
  title: string;
  detail: string;
  action: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      data-testid="ai-system-action-card"
      className="group flex min-h-[132px] w-full flex-col justify-between rounded-md border border-border/70 bg-background px-4 py-3 text-left transition-colors hover:border-primary/35 hover:bg-surface-inset/35"
      onClick={onClick}
    >
      <span className="flex items-start gap-3">
        <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md border border-border/60 bg-surface-inset text-muted-foreground group-hover:text-primary">
          <Icon className="h-4 w-4" />
        </span>
        <span className="min-w-0">
          <span className="block text-sm font-semibold text-foreground">
            {title}
          </span>
          <span className="mt-1 block text-xs leading-relaxed text-muted-foreground">
            {detail}
          </span>
        </span>
      </span>
      <span className="mt-4 inline-flex items-center gap-1 text-xs font-medium text-primary">
        {action}
        <ArrowRight className="h-3.5 w-3.5 transition-transform group-hover:translate-x-0.5" />
      </span>
    </button>
  );
}

function PermissionGroupCard({
  title,
  detail,
  icon: Icon,
  atoms,
  policy,
  risk,
}: (typeof PERMISSION_GROUPS)[number]) {
  return (
    <div className="rounded-md border border-border/60 bg-background/70 px-3 py-3">
      <div className="flex items-start gap-2.5">
        <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-border/60 bg-surface-inset text-muted-foreground">
          <Icon className="h-4 w-4" />
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <h4 className="text-xs font-semibold text-foreground">{title}</h4>
            <span className="rounded border border-border/60 bg-surface-inset px-1.5 py-0.5 text-[10px] text-muted-foreground">
              {risk}风险
            </span>
          </div>
          <p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">
            {detail}
          </p>
        </div>
      </div>
      <div className="mt-3 flex flex-wrap gap-1.5">
        {atoms.map((atom) => (
          <span
            key={atom}
            className="rounded border border-border/60 bg-surface-inset px-1.5 py-0.5 font-mono text-[10px] text-foreground"
          >
            {atom}
          </span>
        ))}
      </div>
      <p className="mt-2 text-[11px] leading-relaxed text-muted-foreground">
        {policy}
      </p>
    </div>
  );
}

export function AiSystemCenterPanel({
  open,
  onClose,
}: AiSystemCenterPanelProps) {
  const [activeSection, setActiveSection] = useState<AiSystemSection>("models");
  const [personaOpen, setPersonaOpen] = useState(false);
  const [skillsOpen, setSkillsOpen] = useState(false);
  const activeMeta =
    AI_SYSTEM_SECTIONS.find((section) => section.id === activeSection) ??
    AI_SYSTEM_SECTIONS[0]!;

  return (
    <>
      <IrisOverlay
        open={open}
        onClose={onClose}
        title="AI 系统中心"
        size="wide"
      >
        <div data-testid="ai-system-center" className="flex min-h-0 flex-1">
          <aside
            data-testid="ai-system-center-nav"
            className="w-56 shrink-0 border-r border-border/60 bg-surface-inset/20 p-2"
          >
            <nav className="space-y-1" aria-label="AI 系统中心">
              {AI_SYSTEM_SECTIONS.map((section) => {
                const active = section.id === activeSection;
                return (
                  <button
                    key={section.id}
                    type="button"
                    className={cn(
                      "w-full rounded-md px-3 py-2 text-left transition-colors duration-base ease-iris-out",
                      active
                        ? "bg-task-selected text-foreground"
                        : "text-muted-foreground hover:bg-surface-inset/70 hover:text-foreground",
                    )}
                    aria-current={active ? "page" : undefined}
                    onClick={() => setActiveSection(section.id)}
                  >
                    <span className="block text-xs font-medium">
                      {section.label}
                    </span>
                    <span className="mt-0.5 block truncate text-[11px] opacity-75">
                      {section.detail}
                    </span>
                  </button>
                );
              })}
            </nav>
          </aside>
          <ScrollArea className="min-h-0 flex-1">
            <div className="px-5 py-4">
              <header className="mb-4 border-b border-border/60 pb-3">
                <h3 className="text-sm font-medium text-foreground">
                  {activeMeta.label}
                </h3>
                <p className="mt-1 text-xs text-muted-foreground">
                  {activeMeta.detail}
                </p>
              </header>

              {activeSection === "models" ? (
                <section>
                  <LlmRoutingSection open={open} />
                </section>
              ) : null}

              {activeSection === "search" ? (
                <section>
                  <MinimaxSearchSection open={open} />
                </section>
              ) : null}

              {activeSection === "persona" ? (
                <section
                  className="max-w-5xl space-y-4"
                  data-testid="ai-system-persona-dashboard"
                >
                  <div
                    className="grid gap-3 lg:grid-cols-3"
                    data-testid="ai-system-summary-grid"
                  >
                    <SummaryTile
                      icon={Bot}
                      label="人格"
                      value="对话身份与写作偏好"
                      detail="昵称、头像、语气和自定义规则会进入 AI 协作上下文。"
                    />
                    <SummaryTile
                      icon={Puzzle}
                      label="Skills"
                      value="场景化工具能力"
                      detail="按全局或当前库安装，匹配场景后再注入给助手。"
                    />
                    <SummaryTile
                      icon={ShieldCheck}
                      label="边界"
                      value="不授予权限"
                      detail="Persona 影响表达与写作偏好，不改变工具权限。"
                    />
                  </div>

                  <div className="grid gap-3 xl:grid-cols-[minmax(0,1fr)_minmax(280px,0.78fr)]">
                    <div className="grid gap-3 md:grid-cols-2">
                      <ActionCard
                        icon={Sparkles}
                        title="人格配置"
                        detail="调整 PromptProfile 的称呼、头像、协作风格、回答语言和常驻规则。"
                        action="打开人格配置"
                        onClick={() => setPersonaOpen(true)}
                      />
                      <ActionCard
                        icon={Wrench}
                        title="Skills 管理"
                        detail="安装、启停、编辑和迁移 SKILL.md；它们不能修改 Persona 配置。"
                        action="管理 Skills"
                        onClick={() => setSkillsOpen(true)}
                      />
                    </div>

                    <div className="rounded-md border border-border/60 bg-surface-inset/25 px-4 py-3">
                      <div className="flex items-center gap-2">
                        <ClipboardCheck className="h-4 w-4 text-muted-foreground" />
                        <h4 className="text-sm font-semibold text-foreground">
                          注入链路
                        </h4>
                      </div>
                      <div className="mt-3 space-y-2.5">
                        {[
                          [
                            "1",
                            "Safety overlay",
                            "最高优先级边界，不被配置覆盖",
                          ],
                          [
                            "2",
                            "PromptProfile",
                            "身份、风格、语言与用户自定义规则",
                          ],
                          [
                            "3",
                            "Task / Skill overlay",
                            "仅追加任务指导，不授予权限",
                          ],
                        ].map(([step, title, detail]) => (
                          <div key={step} className="flex gap-2.5">
                            <span className="flex h-5 w-5 shrink-0 items-center justify-center rounded bg-background text-[10px] font-semibold text-muted-foreground">
                              {step}
                            </span>
                            <span className="min-w-0">
                              <span className="block text-xs font-medium text-foreground">
                                {title}
                              </span>
                              <span className="mt-0.5 block text-[11px] leading-relaxed text-muted-foreground">
                                {detail}
                              </span>
                            </span>
                          </div>
                        ))}
                      </div>
                    </div>
                  </div>

                  <div className="grid gap-3 md:grid-cols-3">
                    {[
                      ["全局", "适合常用写作风格和通用工具。"],
                      ["当前库", "适合项目专属流程、术语和脚本。"],
                      ["确认", "高风险工具执行前会展示参数和影响。"],
                    ].map(([title, detail]) => (
                      <div
                        key={title}
                        className="rounded-md border border-border/50 bg-background/60 px-3 py-2.5"
                      >
                        <p className="text-xs font-semibold text-foreground">
                          {title}
                        </p>
                        <p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">
                          {detail}
                        </p>
                      </div>
                    ))}
                  </div>
                </section>
              ) : null}

              {activeSection === "permissions" ? (
                <section
                  className="max-w-6xl space-y-4"
                  data-testid="agent-permission-settings"
                >
                  <div className="grid gap-3 lg:grid-cols-3">
                    <SummaryTile
                      icon={ShieldCheck}
                      label="授权模型"
                      value="本轮确认优先"
                      detail="工具执行前展示权限、作用域、风险和可撤销方式。"
                    />
                    <SummaryTile
                      icon={LockKeyhole}
                      label="凭据边界"
                      value="不读明文密钥"
                      detail="secret.read_plaintext 被阻断，模型只能请求 named credential 代用。"
                    />
                    <SummaryTile
                      icon={ClipboardCheck}
                      label="审计摘要"
                      value="只记安全摘要"
                      detail="审计记录 request、工具、权限和作用域，不保存正文与敏感输出。"
                    />
                  </div>

                  <div className="grid gap-3 md:grid-cols-2">
                    {PERMISSION_GROUPS.map((group) => (
                      <PermissionGroupCard key={group.title} {...group} />
                    ))}
                  </div>
                </section>
              ) : null}

              {activeSection === "memory" ? (
                <section>
                  <AiRulesPanel compact />
                </section>
              ) : null}
            </div>
          </ScrollArea>
        </div>
      </IrisOverlay>
      <PersonaSettingsPanel
        open={personaOpen}
        onClose={() => setPersonaOpen(false)}
      />
      <SkillsPanel open={skillsOpen} onClose={() => setSkillsOpen(false)} />
    </>
  );
}
