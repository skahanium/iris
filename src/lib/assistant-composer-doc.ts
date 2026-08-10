import type { Node as ProseMirrorNode } from "@tiptap/pm/model";

import type { DisplayMention } from "@/types/ai";
import type { JSONContent } from "@tiptap/core";

export interface AssistantComposerProjection {
  text: string;
  displayMentions: DisplayMention[];
}

export function assistantComposerDocFromText(text: string): JSONContent {
  const lines = text.split(/\r?\n/u);
  return {
    type: "doc",
    content: lines.map((line) => ({
      type: "paragraph",
      ...(line ? { content: [{ type: "text", text: line }] } : {}),
    })),
  };
}

function appendText(
  chunks: string[],
  mentions: DisplayMention[],
  text: string,
  mention?: DisplayMention,
): void {
  const from = chunks.join("").length;
  chunks.push(text);
  if (mention && text.length > 0) {
    mentions.push({
      ...mention,
      range: { from, to: from + text.length },
    });
  }
}

function visitComposerNode(
  node: ProseMirrorNode,
  chunks: string[],
  mentions: DisplayMention[],
  separateChildren: boolean,
): void {
  if (node.isText) {
    appendText(chunks, mentions, node.text ?? "");
    return;
  }
  if (node.type.name === "hardBreak") {
    appendText(chunks, mentions, "\n");
    return;
  }
  if (node.type.name === "assistantMention") {
    const label = String(node.attrs.label ?? "");
    const value = String(node.attrs.value ?? "");
    const kind = node.attrs.kind as DisplayMention["kind"];
    if (
      label &&
      value &&
      (kind === "file" || kind === "folder" || kind === "tag")
    ) {
      appendText(chunks, mentions, label, {
        kind,
        value,
        label,
        range: { from: 0, to: 0 },
      });
    }
    return;
  }
  node.forEach((child, index) => {
    if (separateChildren && index > 0) appendText(chunks, mentions, "\n");
    visitComposerNode(child, chunks, mentions, false);
  });
}

/** Project the private TipTap Composer document to the existing wire shape. */
export function projectAssistantComposerDoc(
  doc: ProseMirrorNode,
): AssistantComposerProjection {
  const chunks: string[] = [];
  const displayMentions: DisplayMention[] = [];

  doc.forEach((node, index) => {
    if (index > 0) appendText(chunks, displayMentions, "\n");
    visitComposerNode(node, chunks, displayMentions, false);
  });

  return { text: chunks.join(""), displayMentions };
}
