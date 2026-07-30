/** Preload the lazy assistant UI without moving it into the eager app chunk. */
export function preloadAssistantPanel(): void {
  void import("@/components/ai/UnifiedAssistantPanel");
}
