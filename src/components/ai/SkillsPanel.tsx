import { Search } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";

import { SkillCard } from "@/components/ai/skills/SkillCard";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { IrisOverlay } from "@/components/ui/iris-overlay";
import { ScrollArea } from "@/components/ui/scroll-area";
import { invokeErrorMessage } from "@/lib/credentials";
import {
  listenSkillsChanged,
  skillsConfirm,
  skillsCreateDraft,
  skillsList,
  type SkillDraft,
  type SkillListEntryDto,
} from "@/lib/ipc";

interface SkillsPanelProps {
  open: boolean;
  onClose: () => void;
}

type SkillScope = "global" | "vault";

function scopeLabel(scope: string): SkillScope {
  return scope === "vault" ? "vault" : "global";
}

function sourceSummary(skill: SkillListEntryDto): string {
  return `${scopeLabel(skill.scope)} · ${skill.file_path}`;
}

function confirmationState(skill: SkillListEntryDto): {
  label: string;
  detail: string;
} {
  if (skill.confirmation_status === "confirmed") {
    return {
      label: "Confirmed",
      detail: `confirmed hash: ${skill.confirmed_hash ?? "none"}`,
    };
  }
  return {
    label: "Needs confirmation",
    detail: "This prompt text must be confirmed before activation.",
  };
}

export function SkillsPanelBody({ open }: { open: boolean }) {
  const [skills, setSkills] = useState<SkillListEntryDto[]>([]);
  const [query, setQuery] = useState("");
  const [draftName, setDraftName] = useState("");
  const [draftDescription, setDraftDescription] = useState("");
  const [draftBody, setDraftBody] = useState("");
  const [draftScopePattern, setDraftScopePattern] = useState("**/*.md");
  const [draft, setDraft] = useState<SkillDraft | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const nextSkills = await skillsList();
      setSkills(nextSkills);
    } catch (nextError) {
      setError(invokeErrorMessage(nextError));
    }
  }, []);

  useEffect(() => {
    if (!open) return;
    void refresh();
  }, [open, refresh]);

  useEffect(() => {
    if (!open) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listenSkillsChanged(() => {
      if (disposed) return;
      void refresh();
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [open, refresh]);

  const filtered = useMemo(
    () =>
      skills.filter(
        (skill) =>
          !query.trim() ||
          skill.name.toLowerCase().includes(query.toLowerCase()) ||
          skill.description.toLowerCase().includes(query.toLowerCase()),
      ),
    [query, skills],
  );

  const global = filtered.filter(
    (skill) => scopeLabel(skill.scope) === "global",
  );
  const vault = filtered.filter((skill) => scopeLabel(skill.scope) === "vault");

  const createDraft = async () => {
    if (!draftName.trim()) return;
    setBusy(true);
    setError(null);
    try {
      const nextDraft = await skillsCreateDraft({
        name: draftName.trim(),
        description: draftDescription.trim() || null,
        body: draftBody.trim() || null,
        scopeRules: [
          {
            kind: "path_glob",
            pattern: draftScopePattern.trim() || "**/*.md",
          },
        ],
      });
      setDraft(nextDraft);
    } catch (nextError) {
      setError(invokeErrorMessage(nextError));
    } finally {
      setBusy(false);
    }
  };

  const confirmDraft = async () => {
    if (!draft) return;
    setBusy(true);
    setError(null);
    try {
      await skillsConfirm(draft);
      setDraft(null);
      setDraftName("");
      setDraftDescription("");
      setDraftBody("");
      await refresh();
    } catch (nextError) {
      setError(invokeErrorMessage(nextError));
    } finally {
      setBusy(false);
    }
  };

  const renderSkillCard = (skill: SkillListEntryDto) => (
    <SkillCard
      key={`${skill.scope}-${skill.name}`}
      skill={skill}
      sourceSummary={sourceSummary(skill)}
      confirmation={confirmationState(skill)}
      onUpdate={() => void refresh()}
    />
  );

  const renderGroup = (title: string, items: SkillListEntryDto[]) => (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <p className="text-xs font-medium text-muted-foreground">{title}</p>
        <span className="text-[10px] text-muted-foreground">
          {items.length}
        </span>
      </div>
      {items.length === 0 ? (
        <p className="rounded-md border border-dashed border-border/70 px-3 py-4 text-center text-xs text-muted-foreground">
          No prompt-only Skills.
        </p>
      ) : (
        items.map(renderSkillCard)
      )}
    </div>
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col" data-testid="skills-panel">
      <div className="task-overlay-filter flex shrink-0 items-center justify-between border-b border-border/60 px-3 py-2">
        <p className="text-xs font-medium text-muted-foreground">Skills</p>
      </div>

      <ScrollArea className="task-overlay-results flex-1">
        <div className="space-y-3 p-3">
          <div className="relative">
            <Search className="absolute left-2 top-2 h-3.5 w-3.5 text-muted-foreground" />
            <Input
              className="h-8 pl-8 text-xs"
              placeholder="Search Skills"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
          </div>

          <section className="space-y-2 rounded-lg border border-border/70 bg-background px-3 py-3">
            <div className="flex items-center justify-between gap-2">
              <div>
                <p className="text-xs font-medium text-foreground">
                  Create prompt-only Skill draft
                </p>
                <p className="mt-1 text-[11px] text-muted-foreground">
                  Drafts are written only after user confirmation.
                </p>
              </div>
              <Button
                type="button"
                size="sm"
                data-testid="skill-create-draft"
                disabled={busy || !draftName.trim()}
                onClick={() => void createDraft()}
              >
                Draft
              </Button>
            </div>
            <div className="grid gap-2 md:grid-cols-2">
              <Input
                value={draftName}
                disabled={busy}
                placeholder="Skill name"
                onChange={(event) => setDraftName(event.target.value)}
              />
              <Input
                value={draftScopePattern}
                disabled={busy}
                placeholder="Scope pattern"
                onChange={(event) => setDraftScopePattern(event.target.value)}
              />
            </div>
            <Input
              value={draftDescription}
              disabled={busy}
              placeholder="Description"
              onChange={(event) => setDraftDescription(event.target.value)}
            />
            <textarea
              value={draftBody}
              disabled={busy}
              rows={4}
              placeholder="Prompt instructions"
              className="flex min-h-24 w-full rounded-md border border-input bg-background px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50"
              onChange={(event) => setDraftBody(event.target.value)}
            />
          </section>

          {draft ? (
            <section className="space-y-2 rounded-lg border border-border/70 bg-muted/35 px-3 py-3">
              <div className="flex items-center justify-between gap-2">
                <div>
                  <p className="text-xs font-medium text-foreground">
                    Draft confirmation
                  </p>
                  <p className="mt-1 text-[11px] text-muted-foreground">
                    {draft.targetPath} · {draft.contentHash}
                  </p>
                </div>
                <Button
                  type="button"
                  size="sm"
                  data-testid="skill-confirm-draft"
                  disabled={busy}
                  onClick={() => void confirmDraft()}
                >
                  Confirm
                </Button>
              </div>
              <pre className="max-h-64 overflow-auto rounded-md border border-border/60 bg-background p-2 text-[11px] leading-5 text-foreground">
                {draft.markdown}
              </pre>
            </section>
          ) : null}

          {error ? <p className="text-xs text-destructive">{error}</p> : null}

          {renderGroup("Current vault", vault)}
          {renderGroup("Global", global)}
        </div>
      </ScrollArea>
    </div>
  );
}

export function SkillsPanel({ open, onClose }: SkillsPanelProps) {
  return (
    <IrisOverlay open={open} onClose={onClose} title="AI Skills" size="command">
      <SkillsPanelBody open={open} />
    </IrisOverlay>
  );
}
