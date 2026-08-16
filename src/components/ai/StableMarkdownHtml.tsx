//! Sanitized Markdown container that keeps DOM node identity stable across
//! streaming updates.
//!
//! React's `dangerouslySetInnerHTML` destroys and recreates the whole subtree
//! whenever the generated HTML changes. For streaming assistant answers this
//! invalidates layout/paint for already-final paragraphs on every flush and
//! interacts badly with the virtual list below the message. This component
//! renders the first frame and then morphs subsequent sanitized HTML in place,
//! so unchanged blocks keep their DOM nodes and selection.

import { useLayoutEffect, useRef, type MouseEvent as ReactMouseEvent } from "react";

import morphdom from "morphdom";

import { toTrustedHtml } from "@/lib/sanitize";

export function StableMarkdownHtml({
  html,
  contentIdentity,
  className,
  dataProseSurface,
  onClick,
}: {
  html: string;
  /** Changes when a different message owns this container. */
  contentIdentity?: string;
  className?: string;
  dataProseSurface?: string;
  onClick?: (event: ReactMouseEvent<HTMLDivElement>) => void;
}) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const lastHtmlRef = useRef<string | null>(null);
  const lastIdentityRef = useRef<string | null | undefined>(null);

  useLayoutEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    if (lastIdentityRef.current !== contentIdentity) {
      lastIdentityRef.current = contentIdentity;
      lastHtmlRef.current = null;
    }
    if (lastHtmlRef.current === html) return;

    if (lastHtmlRef.current === null) {
      container.innerHTML = toTrustedHtml(html) as unknown as string;
      lastHtmlRef.current = html;
      return;
    }

    const template = document.createElement("div");
    template.innerHTML = toTrustedHtml(html) as unknown as string;
    morphdom(container, template, {
      childrenOnly: true,
    });
    lastHtmlRef.current = html;
  }, [contentIdentity, html]);

  return (
    <div
      ref={containerRef}
      className={className}
      data-prose-surface={dataProseSurface}
      onClick={onClick}
      data-ai-stable-markdown
    />
  );
}
