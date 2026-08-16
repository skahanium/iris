//! Finalized assistant message body.
//!
//! Finalized messages never use the streaming Markdown worker. They render
//! synchronously through the contract renderer, which already has a bounded
//! finalized render cache. History messages therefore appear immediately on
//! session switch instead of cycling through a worker placeholder.

import { useMemo, type MouseEvent as ReactMouseEvent } from "react";

import { renderMarkdownWithProfile } from "@/lib/markdown-contract";

import { toTrustedHtml } from "@/lib/sanitize";

export function FinalizedMessageBody({
  content,
  html: providedHtml,
  contentIdentity,
  className,
  dataProseSurface,
  onClick,
}: {
  content: string;
  /** Optional pre-rendered HTML; used when the caller already rendered it. */
  html?: string;
  contentIdentity?: string;
  className?: string;
  dataProseSurface?: string;
  onClick?: (event: ReactMouseEvent<HTMLDivElement>) => void;
}) {
  const html = useMemo(() => {
    if (providedHtml) return providedHtml;
    return renderMarkdownWithProfile(content, "chat_assistant", {
      streaming: false,
    }).output;
  }, [content, providedHtml]);

  return (
    <div
      dangerouslySetInnerHTML={{ __html: toTrustedHtml(html) }}
      data-content-identity={contentIdentity}
      className={className}
      data-prose-surface={dataProseSurface}
      onClick={onClick}
    />
  );
}
