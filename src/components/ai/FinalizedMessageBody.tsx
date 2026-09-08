//! Finalized assistant message body.
//!
//! Compatibility entry sharing the continuous block renderer. Existing callers
//! with pre-rendered HTML can still provide it directly.

import { type MouseEvent as ReactMouseEvent } from "react";

import { StreamingMessageBody } from "./StreamingMessageBody";

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
  if (providedHtml === undefined)
    return (
      <StreamingMessageBody
        content={content}
        streaming={false}
        contentIdentity={contentIdentity}
        className={className}
        dataProseSurface={dataProseSurface}
        onClick={onClick}
      />
    );

  return (
    <div
      dangerouslySetInnerHTML={{ __html: toTrustedHtml(providedHtml) }}
      data-content-identity={contentIdentity}
      className={className}
      data-prose-surface={dataProseSurface}
      onClick={onClick}
    />
  );
}
