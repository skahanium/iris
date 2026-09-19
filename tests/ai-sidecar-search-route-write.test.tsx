import { act, renderHook, waitFor } from "@testing-library/react";
import { createRef } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { useAiSidecarBridge } from "@/hooks/useAiSidecarBridge";
import {
  settingsGet,
  webEvidenceProvidersList,
  webSearchRouteGet,
  webSearchRouteSet,
  type WebEvidenceProviderSummary,
} from "@/lib/ipc";

vi.mock("@/lib/ipc", () => ({
  settingsGet: vi.fn(),
  settingsSet: vi.fn(async () => undefined),
  webEvidenceProvidersList: vi.fn(),
  webSearchRouteGet: vi.fn(),
  webSearchRouteSet: vi.fn(),
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
  return () => [...stored];
}

async function mountBridge() {
  const before = vi.mocked(webEvidenceProvidersList).mock.calls.length;
  const view = renderHook(() => useAiSidecarBridge({ editorRef: createRef() }));
  await waitFor(() =>
    expect(
      vi.mocked(webEvidenceProvidersList).mock.calls.length,
    ).toBeGreaterThan(before),
  );
  return view;
}

describe("Q02 两个设置入口的 MCP 搜索候选写入语义", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(settingsGet).mockResolvedValue(false);
    vi.mocked(webEvidenceProvidersList).mockResolvedValue([A, B, C]);
  });

  it("侧栏改主服务时保留备用项，并把它提到首位", async () => {
    const stored = installRoute([A.id, B.id, C.id]);
    const { result } = await mountBridge();

    await act(async () => {
      result.current.setWebSearchProviderId(B.id);
    });

    // The sidecar offers the selection UI for the primary only; the route file
    // is what holds the failover order. Writing `[B]` would delete alpha and
    // gamma, which is the defect: the two settings entries then disagree.
    expect(stored()).toEqual([B.id, A.id, C.id]);
    expect(result.current.webSearchProviderId).toBe(B.id);
  });

  it("管理中心保存过的备用项不因侧栏选择而丢失", async () => {
    const stored = installRoute([A.id, B.id]);
    const first = await mountBridge();

    await act(async () => {
      // Simulates the management-centre order being persisted while the
      // sidecar is mounted: the primary is gamma, with alpha as the backup.
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

    // alpha becomes primary; gamma must survive as the failover entry.
    expect(stored()).toEqual([A.id, C.id]);
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

    expect(stored()).toEqual([]);
    expect(result.current.webSearchProviderId).toBeNull();
  });

  it("所选主服务已禁用时，可用性回退到仍启用的备用项而不是报无提供方", async () => {
    // `webSearchProviderId` 只是主服务投影；备用项仍启用时，整个联网开关不应被判成
    // 不可用——这正是把备用数组写没之后会出现的用户可见后果。
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
