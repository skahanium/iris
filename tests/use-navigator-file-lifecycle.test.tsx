import { act, createElement, type MutableRefObject } from "react";
import { createRoot, type Root } from "react-dom/client";
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  type Mock,
  vi,
} from "vitest";

import type { TabItem } from "@/components/layout/TabBar";
import { useNavigatorFileLifecycle } from "@/hooks/useNavigatorFileLifecycle";
import type { PersistBeforeLeave } from "@/hooks/useAppPersistenceLifecycle";

type HookApi = ReturnType<typeof useNavigatorFileLifecycle>;
type BeginPathMigration = (oldPath: string, newPath: string) => Promise<void>;
type CompletePathMigration = (oldPath: string, newPath: string) => string;

function Harness({
  onReady,
  beginPathMigration,
  completePathMigration,
}: {
  onReady: (api: HookApi) => void;
  beginPathMigration: BeginPathMigration;
  completePathMigration: CompletePathMigration;
}) {
  const api = useNavigatorFileLifecycle({
    activePathRef: { current: "old.md" },
    awaitSaveInFlight: vi.fn(async () => undefined),
    abortPathMigration: vi.fn(),
    beginPathMigration,
    bumpVaultIndex: vi.fn(),
    cancelPendingSave: vi.fn(),
    completePathMigration,
    discardOpenTab: vi.fn(async () => undefined),
    persistBeforeLeaveRef: {
      current: vi.fn(async () => "saved before move"),
    } as MutableRefObject<PersistBeforeLeave>,
    replaceOpenTabPath: vi.fn(),
    tabsRef: {
      current: [
        { dirty: false, locked: false, path: "old.md", title: "Old" },
      ] as TabItem[],
    },
  });
  onReady(api);
  return null;
}

describe("useNavigatorFileLifecycle", () => {
  let api!: HookApi;
  let beginPathMigration: Mock<BeginPathMigration>;
  let completePathMigration: Mock<CompletePathMigration>;
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(async () => {
    beginPathMigration = vi.fn<BeginPathMigration>(async () => undefined);
    completePathMigration = vi.fn<CompletePathMigration>(
      () => "edited while moving",
    );
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
    await act(async () => {
      root.render(
        createElement(Harness, {
          beginPathMigration,
          completePathMigration,
          onReady: (next) => {
            api = next;
          },
        }),
      );
    });
  });

  afterEach(() => {
    act(() => root.unmount());
    container.remove();
  });

  it("routes a navigator rename through migration and rebind before replacing the tab", async () => {
    await act(async () => {
      await api.handleBeforeFilePathChange("old.md", "new.md");
    });
    expect(beginPathMigration).toHaveBeenCalledWith("old.md", "new.md");

    act(() => {
      api.handleFilePathChanged("old.md", "new.md", "New");
    });

    expect(completePathMigration).toHaveBeenCalledWith("old.md", "new.md");
  });
});
