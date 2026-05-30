import type { ContextPacket } from "@/types/ai";

/** 按 id 合并多组证据包，保持首次出现顺序。 */
export function mergeContextPackets(
  ...lists: ReadonlyArray<ContextPacket[] | undefined>
): ContextPacket[] {
  const seen = new Set<string>();
  const out: ContextPacket[] = [];
  for (const list of lists) {
    for (const packet of list ?? []) {
      if (seen.has(packet.id)) continue;
      seen.add(packet.id);
      out.push(packet);
    }
  }
  return out;
}
