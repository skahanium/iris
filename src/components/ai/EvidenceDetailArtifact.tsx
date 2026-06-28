import { FileText, Globe } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import type { SessionEvidenceRecord } from "@/types/ipc";

export interface EvidenceDetailArtifactPayload {
  sessionId: number;
  evidence: SessionEvidenceRecord[];
}

function isEvidencePayload(
  value: unknown,
): value is EvidenceDetailArtifactPayload {
  if (typeof value !== "object" || value === null) return false;
  const record = value as Record<string, unknown>;
  return typeof record.sessionId === "number" && Array.isArray(record.evidence);
}

function localStatus(evidence: SessionEvidenceRecord): string {
  if (evidence.detailStatus) return evidence.detailStatus;
  if (!evidence.sourcePath) return "source_missing";
  return evidence.contentHash ? "source_unchanged" : "span_missing";
}

function webTitle(evidence: SessionEvidenceRecord): string {
  return (
    evidence.title?.trim() ||
    evidence.domain?.trim() ||
    evidence.url ||
    evidence.citationLabel
  );
}

export function EvidenceDetailArtifactView({ payload }: { payload: unknown }) {
  if (!isEvidencePayload(payload)) {
    return (
      <p className="text-sm text-muted-foreground">
        No evidence detail available.
      </p>
    );
  }

  const evidence = payload.evidence;
  if (evidence.length === 0) {
    return (
      <p className="text-sm text-muted-foreground">
        No evidence in this session.
      </p>
    );
  }

  return (
    <div className="grid gap-6 lg:grid-cols-[180px_minmax(0,1fr)]">
      <aside className="hidden border-r border-border/60 pr-4 text-xs lg:block">
        <p className="mb-2 font-medium text-muted-foreground">Outline</p>
        <nav className="space-y-1">
          {evidence.map((item) => (
            <a
              key={item.citationLabel}
              href={`#evidence-${item.citationIndex}`}
              className="block truncate text-muted-foreground hover:text-foreground"
            >
              {item.citationLabel}{" "}
              {item.title || item.sourcePath || item.domain || item.url}
            </a>
          ))}
        </nav>
      </aside>
      <article className="min-w-0 space-y-6">
        <header>
          <p className="text-xs text-muted-foreground">
            Session #{payload.sessionId}
          </p>
          <h1 className="text-2xl font-semibold tracking-normal">
            Evidence Detail
          </h1>
        </header>
        {evidence.map((item) => (
          <section
            key={item.citationLabel}
            id={`evidence-${item.citationIndex}`}
            className="scroll-mt-6 border-b border-border/60 pb-5 last:border-b-0"
          >
            <div className="flex flex-wrap items-center gap-2">
              {item.sourceType === "web" ? (
                <Globe className="h-4 w-4 text-muted-foreground" />
              ) : (
                <FileText className="h-4 w-4 text-muted-foreground" />
              )}
              <h2 className="text-lg font-semibold tracking-normal">
                {item.citationLabel}{" "}
                {item.sourceType === "web" ? webTitle(item) : item.title}
              </h2>
              <Badge variant="outline">{item.sourceType}</Badge>
            </div>
            <dl className="mt-3 grid gap-2 text-sm sm:grid-cols-[140px_minmax(0,1fr)]">
              <dt className="text-muted-foreground">Status</dt>
              <dd>
                {item.sourceType === "web"
                  ? "external_metadata_only"
                  : localStatus(item)}
              </dd>
              <dt className="text-muted-foreground">Source</dt>
              <dd className="break-words">
                {item.sourceType === "web"
                  ? item.url
                  : item.sourcePath || "source missing"}
              </dd>
              {item.headingPath ? (
                <>
                  <dt className="text-muted-foreground">Heading</dt>
                  <dd>{item.headingPath}</dd>
                </>
              ) : null}
              {item.retrievalReason ? (
                <>
                  <dt className="text-muted-foreground">Reason</dt>
                  <dd>{item.retrievalReason}</dd>
                </>
              ) : null}
            </dl>
            {item.sourceType === "web" ? (
              <p className="mt-3 text-sm text-muted-foreground">
                External webpage; page body and excerpt were not saved.
              </p>
            ) : null}
            {item.sourceType === "local" && item.liveExcerpt ? (
              <pre className="mt-3 whitespace-pre-wrap rounded-md bg-surface-inset p-3 text-sm text-foreground">
                {item.liveExcerpt}
              </pre>
            ) : null}
          </section>
        ))}
      </article>
    </div>
  );
}
