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

describe("useNavigatorFileLifecycle", () => {
  let api!: HookApi;
  let beginPathMigration: Mock<BeginPathMigration>;
  let completePathMigration: Mock<CompletePathMigration>;
  let discardOpenTab: Mock<(path: string) => Promise<void>>;
  let persistBeforeLeave: Mock<PersistBeforeLeave>;
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(async () => {
    beginPathMigration = vi.fn<BeginPathMigration>(async () => undefined);
    completePathMigration = vi.fn<CompletePathMigration>(
      () => "edited while moving",
    );
    discardOpenTab = vi.fn(async () => undefined);
    persistBeforeLeave = vi.fn(async () => "saved before delete");
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
    await act(async () => {
      root.render(
        createElement(
          function HarnessWithDelete({
            onReady,
          }: {
            onReady: (api: HookApi) => void;
          }) {
            const next = useNavigatorFileLifecycle({
              abortPathMigration: vi.fn(),
              beginPathMigration,
              bumpVaultIndex: vi.fn(),
              completePathMigration,
              discardOpenTab,
              persistBeforeLeaveRef: {
                current: persistBeforeLeave,
              } as MutableRefObject<PersistBeforeLeave>,
              replaceOpenTabPath: vi.fn(),
              tabsRef: {
                current: [
                  { dirty: true, locked: false, path: "note.md", title: "Note" },
                ] as TabItem[],
              },
            });
            onReady(next);
            return null;
          },
          {
            onReady: (next) => {
              api = next;
            },
          },
        ),
      );
    });
  });

  afterEach(() => {
    act(() => root.unmount());
    container.remove();
  });

  it("routes a navigator rename through migration and rebind before replacing the tab", async () => {
    await act(async () => {
      await api.handleBeforeFilePathChange("note.md", "renamed.md");
    });
    expect(beginPathMigration).toHaveBeenCalledWith("note.md", "renamed.md");

    act(() => {
      api.handleFilePathChanged("note.md", "renamed.md", "Renamed");
    });

    expect(completePathMigration).toHaveBeenCalledWith("note.md", "renamed.md");
  });

  it("flushes dirty content before deleting an open tab", async () => {
    await act(async () => {
      await api.handleBeforeFileDelete("note.md");
    });
    expect(persistBeforeLeave).toHaveBeenCalledWith("note.md");
    expect(discardOpenTab).toHaveBeenCalledWith("note.md");
  });
});
