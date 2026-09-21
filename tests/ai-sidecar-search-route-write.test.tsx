import { act, renderHook, waitFor } from "@testing-library/react";
import { createRef } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { useAiSidecarBridge } from "@/hooks/useAiSidecarBridge";
import {
  settingsGet,
  webEvidenceProvidersList,
  webSearchRouteGet,
  webSearchRoutePromote,
  webSearchRouteSet,
  type WebEvidenceProviderSummary,
} from "@/lib/ipc";

vi.mock("@/lib/ipc", () => ({
  settingsGet: vi.fn(),
  settingsSet: vi.fn(async () => undefined),
  webEvidenceProvidersList: vi.fn(),
  webSearchRouteGet: vi.fn(),
  webSearchRouteSet: vi.fn(),
  webSearchRoutePromote: vi.fn(),
}));

function provider(id: string): WebEvidenceProviderSummary {
  return {
    id,
    name: `Provider ${id}`,
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
}

const A = provider("alpha");
const B = provider("beta");
const C = provider("gamma");

function installRoute(initial: string[]) {
  let stored = [...initial];
  vi.mocked(webSearchRouteGet).mockImplementation(async () => ({
    candidateProviderIds: [...stored],
  }));
  vi.mocked(webSearchRouteSet).mockImplementation(async (route) => {
    stored = [...new Set(route.candidateProviderIds)].slice(0, 3);
    return { candidateProviderIds: [...stored] };
  });
  vi.mocked(webSearchRoutePromote).mockImplementation(async (providerId) => {
    const trimmed = providerId.trim();
    stored = [trimmed, ...stored.filter((id) => id !== trimmed)];
    stored = [...new Set(stored)].slice(0, 3);
    return { candidateProviderIds: [...stored] };
  });
  return () => [...stored];
}

async function mountBridge() {
  const before = vi.mocked(webEvidenceProvidersList).mock.calls.length;
  const view = renderHook(() => useAiSidecarBridge({ editorRef: createRef() }));
  // The mount effect awaits `Promise.all([settingsGet, webEvidenceProvidersList,
  // webSearchRouteGet])` and only then pushes `webSearchProviders` /
  // `webSearchProviderId` into state. Waiting for the *call* therefore returns
  // while the availability projection is still empty, so the assertions raced
  // the state update. Wait for both state slices the projection derives from.
  await waitFor(() => {
    expect(
      vi.mocked(webEvidenceProvidersList).mock.calls.length,
    ).toBeGreaterThan(before);
    expect(view.result.current.webSearchProviders.length).toBeGreaterThan(0);
  });
  return view;
}

describe("Q02 两个设置入口的 MCP 搜索候选写入语义", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(settingsGet).mockResolvedValue(false);
    vi.mocked(webEvidenceProvidersList).mockResolvedValue([A, B, C]);
  });

  it("侧栏改主服务时走 promote，保留备用项并提到首位", async () => {
    const stored = installRoute([A.id, B.id, C.id]);
    const { result } = await mountBridge();
    const setCallsBefore = vi.mocked(webSearchRouteSet).mock.calls.length;

    await act(async () => {
      result.current.setWebSearchProviderId(B.id);
    });

    expect(vi.mocked(webSearchRoutePromote)).toHaveBeenCalledWith(B.id);
    expect(vi.mocked(webSearchRouteSet).mock.calls.length).toBe(setCallsBefore);
    expect(stored()).toEqual([B.id, A.id, C.id]);
    expect(result.current.webSearchProviderId).toBe(B.id);
  });

  it("管理中心保存过的备用项不因侧栏选择而丢失", async () => {
    const stored = installRoute([A.id, B.id]);
    const first = await mountBridge();

    await act(async () => {
      await webSearchRouteSet({
        candidateProviderIds: [C.id, A.id],
      });
    });
    expect(stored()).toEqual([C.id, A.id]);
    first.unmount();

    const { result } = await mountBridge();
    await act(async () => {
      result.current.setWebSearchProviderId(A.id);
    });

    expect(stored()).toEqual([A.id, C.id]);
  });

  it("挂载中的面板 set 之后侧栏 promote 读服务端路线，不把已删项写回", async () => {
    const stored = installRoute([A.id, B.id, C.id]);
    const { result } = await mountBridge();

    await act(async () => {
      await webSearchRouteSet({
        candidateProviderIds: [C.id, A.id],
      });
    });
    expect(stored()).toEqual([C.id, A.id]);
    const setCallsAfterPanel = vi.mocked(webSearchRouteSet).mock.calls.length;

    await act(async () => {
      result.current.setWebSearchProviderId(A.id);
    });

    await waitFor(() => expect(stored()).toEqual([A.id, C.id]));
    expect(vi.mocked(webSearchRoutePromote)).toHaveBeenCalledWith(A.id);
    expect(vi.mocked(webSearchRouteSet).mock.calls.length).toBe(
      setCallsAfterPanel,
    );
    expect(stored()).not.toEqual([A.id, B.id, C.id]);
    expect(stored()).not.toEqual([A.id, C.id, B.id]);
    expect(stored()).not.toContain(B.id);
  });

  it("promote 失败时从服务端回读，不把乐观主服务留在界面", async () => {
    const stored = installRoute([A.id, B.id]);
    const { result } = await mountBridge();
    vi.mocked(webSearchRoutePromote).mockRejectedValueOnce(
      new Error("promote failed"),
    );

    await act(async () => {
      result.current.setWebSearchProviderId(B.id);
    });

    await waitFor(() => expect(result.current.webSearchProviderId).toBe(A.id));
    expect(stored()).toEqual([A.id, B.id]);
  });

  it("两个入口交替保存后候选集合稳定，不产生重复项", async () => {
    const stored = installRoute([A.id, B.id, C.id]);
    const { result } = await mountBridge();

    await act(async () => {
      result.current.setWebSearchProviderId(B.id);
    });
    await act(async () => {
      result.current.setWebSearchProviderId(A.id);
    });
    await act(async () => {
      result.current.setWebSearchProviderId(B.id);
    });

    expect(stored()).toEqual([B.id, A.id, C.id]);
    expect(new Set(stored()).size).toBe(stored().length);
  });

  it("清空选择时写空候选而不是保留旧主服务", async () => {
    const stored = installRoute([A.id, B.id]);
    const { result } = await mountBridge();

    await act(async () => {
      result.current.setWebSearchProviderId(null);
    });

    expect(vi.mocked(webSearchRouteSet)).toHaveBeenCalledWith({
      candidateProviderIds: [],
    });
    expect(vi.mocked(webSearchRoutePromote)).not.toHaveBeenCalled();
    expect(stored()).toEqual([]);
    expect(result.current.webSearchProviderId).toBeNull();
  });

  it("所选主服务已禁用时，可用性回退到仍启用的备用项而不是报无提供方", async () => {
    vi.mocked(webEvidenceProvidersList).mockResolvedValue([
      { ...A, enabled: false },
      B,
    ]);
    installRoute([A.id, B.id]);
    const { result } = await mountBridge();

    expect(result.current.webSearchAvailability.canEnable).toBe(true);
    expect(
      result.current.webSearchAvailability.options.map((o) => o.id),
    ).toEqual([B.id]);
  });
});
