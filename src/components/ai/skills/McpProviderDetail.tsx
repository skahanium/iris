import type { ReactNode } from "react";

import { McpProfileCard, type McpCredentialSave } from "./McpProfileCard";
import type {
  WebEvidenceProviderDiagnostics,
  WebEvidenceProviderInput,
  WebEvidenceProviderSummary,
} from "@/lib/ipc";

export type { McpCredentialSave };

interface McpProviderDetailProps {
  provider: WebEvidenceProviderSummary;
  diagnostics?: WebEvidenceProviderDiagnostics | null;
  credentialConfiguredByService?: Record<string, boolean>;
  saving?: boolean;
  persisted?: boolean;
  children?: ReactNode;
  onSave: (
    input: WebEvidenceProviderInput,
    credentialSaves: McpCredentialSave[],
  ) => void | Promise<void>;
  onToggle: (enabled: boolean) => void | Promise<void>;
  onDelete: () => void | Promise<void>;
  onClearCredential: (service: string) => void | Promise<void>;
  onDiagnostics: () => void | Promise<void>;
  onConfigurationChanged: () => void;
}

export function McpProviderDetail({
  children,
  ...props
}: McpProviderDetailProps) {
  return (
    <>
      <McpProfileCard {...props} surface="detail" />
      {children}
    </>
  );
}
