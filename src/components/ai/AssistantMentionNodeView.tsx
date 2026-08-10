import { FileText, Folder, Hash } from "lucide-react";
import { NodeViewWrapper, type NodeViewProps } from "@tiptap/react";

import { cn } from "@/lib/utils";

export function AssistantMentionNodeView({ node, selected }: NodeViewProps) {
  const kind = String(node.attrs.kind ?? "file");
  const Icon = kind === "folder" ? Folder : kind === "tag" ? Hash : FileText;
  const label = String(node.attrs.label ?? "");

  return (
    <NodeViewWrapper
      as="span"
      contentEditable={false}
      data-assistant-mention="true"
      data-mention-kind={kind}
      className={cn("ai-composer-mention-node", selected && "is-selected")}
      title={label}
    >
      <Icon aria-hidden="true" className="ai-composer-mention-icon" />
      <span>{label}</span>
    </NodeViewWrapper>
  );
}
