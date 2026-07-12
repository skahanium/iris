import type { ContextPacket } from "@/types/ai";

function isMissingPacketValue(value: unknown): boolean {
  return value === null || value === undefined || value === "";
}

function mergePacketFields(
  existing: ContextPacket,
  candidate: ContextPacket,
): ContextPacket {
  const merged: Record<string, unknown> = { ...existing };
  const candidateRecord = candidate as unknown as Record<string, unknown>;

  for (const key of Object.keys(candidateRecord)) {
    if (
      isMissingPacketValue(merged[key]) &&
      !isMissingPacketValue(candidateRecord[key])
    ) {
      merged[key] = candidateRecord[key];
    }
  }

  return merged as unknown as ContextPacket;
}

/** 按 id 合并多组证据包，保持首次出现顺序。 */
export function mergeContextPackets(
  ...lists: ReadonlyArray<ContextPacket[] | undefined>
): ContextPacket[] {
  const indexById = new Map<string, number>();
  const out: ContextPacket[] = [];
  for (const list of lists) {
    for (const packet of list ?? []) {
      const existingIndex = indexById.get(packet.id);
      if (existingIndex !== undefined) {
        const existing = out[existingIndex];
        if (existing) {
          out[existingIndex] = mergePacketFields(existing, packet);
        }
        continue;
      }
      indexById.set(packet.id, out.length);
      out.push(packet);
    }
  }
  return out;
}
