import {
  act,
  fireEvent,
  render,
  renderHook,
  screen,
  waitFor,
} from "@testing-library/react";
import { createRef } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { useAiSidecarBridge } from "@/hooks/useAiSidecarBridge";
import { ConnectivityIndicators } from "@/components/layout/ConnectivityIndicators";
import {
  settingsGet,
  settingsSet,
  webEvidenceProvidersList,
  webSearchRouteGet,
  type WebEvidenceProviderSummary,
} from "@/lib/ipc";

vi.mock("@/lib/ipc", () => ({
  settingsGet: vi.fn(),
  settingsSet: vi.fn(async () => undefined),
  webEvidenceProvidersList: vi.fn(),
  webSearchRouteGet: vi.fn(),
  webSearchRouteSet: vi.fn(async () => undefined),
}));

const provider: WebEvidenceProviderSummary = {
  id: "search",
  name: "Search",
  providerKind: "mcp",
  enabled: true,
  hasSearchMapping: true,
  hasFetchMapping: true,
  transportKind: "https",
  transportConfigJson: "{}",
  credentialRefsJson: "{}",
  mappingStatus: "complete",
  diagnosticStatus: "ready",
  isNative: false,
  editable: true,
};

describe("AI sidecar web authorization preference", () => {
  it("shows granted authorization during an outage and allows revoking it", () => {
    const onChange = vi.fn();
    render(
      <ConnectivityIndicators
        status={null}
        webSearch
        onWebSearchChange={onChange}
        webSearchAvailability={{
          canEnable: false,
          reason: "provider_unavailable",
          detail: "服务暂不可用",
          selectedProviderId: null,
          effectiveProvider: null,
          options: [],
        }}
      />,
    );
    const toggle = screen.getByRole("switch", { name: "关闭联网搜索" });
    expect(toggle).toHaveAttribute("aria-checked", "true");
    expect(toggle).not.toBeDisabled();
    fireEvent.click(toggle);
    expect(onChange).toHaveBeenCalledWith(false);
  });
  beforeEach(() => {
    vi.mocked(settingsGet).mockReset();
    vi.mocked(settingsSet).mockClear();
    vi.mocked(webEvidenceProvidersList).mockReset();
    vi.mocked(webSearchRouteGet).mockReset();
  });

  it("restores persisted authorization even when provider availability fails", async () => {
    vi.mocked(settingsGet).mockResolvedValue(true);
    vi.mocked(webEvidenceProvidersList).mockRejectedValue(new Error("offline"));
    vi.mocked(webSearchRouteGet).mockRejectedValue(new Error("offline"));

    const { result } = renderHook(() =>
      useAiSidecarBridge({ editorRef: createRef() }),
    );

    await waitFor(() => expect(result.current.webSearchEnabled).toBe(true));
    expect(settingsSet).not.toHaveBeenCalledWith("web_search_enabled", false);
    expect(result.current.webSearchAvailability.canEnable).toBe(false);
  });

  it("does not let delayed startup state overwrite a newer explicit choice", async () => {
    let resolveEnabled: (value: boolean) => void = () => undefined;
    vi.mocked(settingsGet).mockImplementation(
      () => new Promise<boolean>((resolve) => (resolveEnabled = resolve)),
    );
    vi.mocked(webEvidenceProvidersList).mockResolvedValue([provider]);
    vi.mocked(webSearchRouteGet).mockResolvedValue({
      candidateProviderIds: [provider.id],
    });

    const { result } = renderHook(() =>
      useAiSidecarBridge({ editorRef: createRef() }),
    );
    await waitFor(() =>
      expect(webEvidenceProvidersList).toHaveBeenCalledTimes(1),
    );
    act(() => result.current.setWebSearch(false));
    await act(async () => resolveEnabled(true));

    await waitFor(() => expect(result.current.webSearchEnabled).toBe(false));
    expect(settingsSet).toHaveBeenLastCalledWith("web_search_enabled", false);
  });
});
