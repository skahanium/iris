import { useCallback, useState } from "react";

import { findPacketByCitationRef } from "@/lib/ai/citation-markdown";
import type { ContextPacket } from "@/types/ai";

export interface UseCitationClickReturn {
  handleCitationClick: (ref: string) => void;
  citationMiss: string | null;
  clearCitationMiss: () => void;
}

export function useCitationClick(
  packets: ContextPacket[],
  onOpenPackets: () => void,
  onSelectPacket: (ids: string[]) => void,
): UseCitationClickReturn {
  const [citationMiss, setCitationMiss] = useState<string | null>(null);

  const handleCitationClick = useCallback(
    (ref: string) => {
      const packet = findPacketByCitationRef(ref, packets);
      if (!packet) {
        setCitationMiss(ref);
        onOpenPackets();
        return;
      }
      setCitationMiss(null);
      onSelectPacket([packet.id]);
      onOpenPackets();
    },
    [packets, onOpenPackets, onSelectPacket],
  );

  const clearCitationMiss = useCallback(() => setCitationMiss(null), []);

  return { handleCitationClick, citationMiss, clearCitationMiss };
}
