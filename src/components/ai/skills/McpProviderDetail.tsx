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
  onBack: () => void;
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

export function McpProviderDetail(props: McpProviderDetailProps) {
  const { onBack, ...rest } = props;
  return <McpProfileCard {...rest} surface="detail" onBack={onBack} />;
}
