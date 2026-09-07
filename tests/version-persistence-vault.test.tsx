import { act, useRef } from "react";
import type { Editor } from "@tiptap/react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  useAppPersistenceLifecycle,
  type PersistBeforeLeave,
} from "@/hooks/useAppPersistenceLifecycle";

const ipc = vi.hoisted(() => ({
  appExit: vi.fn(),
  fileDiscard: vi.fn(),
  fileSetLock: vi.fn(),
  fileWrite: vi.fn(),
  versionFinalizeCurrent: vi.fn(),
  versionSaveIdle: vi.fn(),
  versionSaveManual: vi.fn(),
  versionSavePreClose: vi.fn(),
}));
vi.mock("@/lib/ipc", () => ipc);
vi.mock("@/lib/tauri-runtime", () => ({ isTauriRuntime: () => false }));
const noop = () => {};
let api: ReturnType<typeof useAppPersistenceLifecycle>;
function Harness({ vaultPath }: { vaultPath: string }) {
  const path = useRef<string | null>("same.md");
  const body = useRef(() => "saved body");
  const tabs = useRef([
    { path: "same.md", title: "same", dirty: false, locked: false },
  ]);
  api = useAppPersistenceLifecycle({
    vaultPath,
    activeFileLocked: false,
    activePath: path.current,
    activePathRef: path,
    applySavedMarkdown: noop,
    autoSnapshotGenerationRef: useRef(0),
    autoVersionEnabled: false,
    autoVersionIdleMinutes: 5,
    dirtyRef: useRef(false),
    persistenceContentTick: 1,
    editorRef: useRef<Editor | null>(null),
    editorReadyRef: useRef(false),
    getLiveMarkdownRef: body,
    getTabMarkdownCached: () => "saved body",
    markClean: noop,
    markdown: "saved body",
    persistBeforeLeaveRef: useRef<PersistBeforeLeave>(async () => null),
    setAiStatus: noop,
    setFileLocked: noop,
    setMarkdown: noop,
    syncTabMarkdownCache: noop,
    tabsRef: tabs,
  });
  return null;
}
let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  vi.clearAllMocks();
  ipc.versionSaveManual.mockResolvedValue({ created: true, versionId: 1 });
  ipc.versionFinalizeCurrent.mockResolvedValue(null);
  ipc.versionSavePreClose.mockResolvedValue({ created: true, versionId: 2 });
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});
afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

it("retains the enqueue Vault for delayed version writes after switching Vault", async () => {
  let release!: (value: unknown) => void;
  ipc.versionSaveManual.mockReturnValueOnce(
    new Promise((resolve) => {
      release = resolve;
    }),
  );
  await act(async () => root.render(<Harness vaultPath="/vault-A" />));
  const oldScheduler = api.versionSnapshotScheduler;
  const first = oldScheduler.saveManual("same.md", "A original");
  const queued = oldScheduler.finalize("same.md", "A final", "A label");
  await act(async () => root.render(<Harness vaultPath="/vault-B" />));
  const current = api.versionSnapshotScheduler.savePreClose(
    "same.md",
    "B body",
  );
  await act(async () => {
    release({ created: true, versionId: 1 });
    await Promise.all([first, queued, current]);
  });
  expect(ipc.versionSaveManual).toHaveBeenCalledWith(
    "same.md",
    "A original",
    "/vault-A",
  );
  expect(ipc.versionFinalizeCurrent).toHaveBeenCalledWith(
    "same.md",
    "A final",
    "A label",
    "/vault-A",
  );
  expect(ipc.versionSavePreClose).toHaveBeenCalledWith(
    "same.md",
    "B body",
    "/vault-B",
  );
});
