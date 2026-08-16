//! Stable identities for assistant conversations and messages.
//!
//! The virtual list must distinguish "the same message kept streaming" from
//! "a different message landed in the same row slot". Index keys are not
//! identities: switching sessions or retracting a message would reuse DOM rows
//! and morph unrelated Markdown trees together.

export interface AssistantSessionIdentityFields {
  domain?: string;
  sessionKey?: string;
}

export interface AssistantMessageIdentityFields {
  role: string;
  runId?: string;
  clientRequestId?: string;
  turnId?: string;
  seq?: number;
}

export function assistantSessionIdentity(
  session: AssistantSessionIdentityFields | null | undefined,
): string {
  if (!session?.domain || !session.sessionKey) return "new-session";
  return `${session.domain}:${session.sessionKey}`;
}

export function assistantMessageIdentity(
  message: AssistantMessageIdentityFields,
  fallbackIndex: number,
): string {
  const durable = message.runId
    ? `run:${message.runId}`
    : message.clientRequestId
      ? `request:${message.clientRequestId}`
      : message.seq != null
        ? `seq:${message.seq}`
        : `index:${fallbackIndex}`;
  return [durable, message.role, message.turnId ?? ""].join("|");
}
