import { readFileSync } from "node:fs";

import { beforeEach, describe, expect, expectTypeOf, it, vi } from "vitest";

const invoke = vi.fn();
const listen = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (...args: unknown[]) => listen(...args),
}));

import {
  feedDiscover,
  feedDocumentCancel,
  feedDocumentPrepare,
  feedDocumentRelease,
  feedImagePrepare,
  feedImagesAuthorize,
  feedImagesRelease,
  listenFeedDocumentProgress,
  feedFulltextEnqueueItem,
  feedItemGet,
  feedItemList,
  feedItemsMarkRead,
  feedItemSetState,
  feedOpmlExport,
  feedOpmlImport,
  feedSourceAdd,
  feedSourceItemCount,
  feedSourceTrashMatch,
  feedSourceTrashPreview,
  feedSourceTrash,
  feedSourceTrashPurge,
  feedSourceTrashRestore,
  feedSourceUpdate,
  feedTrashClear,
  feedTrashList,
  feedTrashRestore,
  feedSyncAll,
  feedSyncBatch,
  feedSyncSource,
} from "@/lib/ipc";
import { IPC_EVENTS } from "@/lib/ipc-events";
import type {
  FeedChangedEvent,
  FeedItemDetail,
  FeedItemQuery,
  FeedItemStatePatch,
  FeedSourceSummary,
  OpmlImportResult,
} from "@/types/ipc";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

const FEED_COMMANDS = [
  "feed_discover",
  "feed_source_add",
  "feed_source_list",
  "feed_source_update",
  "feed_source_trash",
  "feed_source_trash_restore",
  "feed_source_trash_purge",
  "feed_source_item_count",
  "feed_source_trash_match",
  "feed_source_trash_preview",
  "feed_library_summary",
  "feed_trash_list",
  "feed_trash_restore",
  "feed_trash_clear",
  "feed_library_optimize",
  "feed_item_list",
  "feed_item_get",
  "feed_item_set_state",
  "feed_fulltext_enqueue_item",
  "feed_document_prepare",
  "feed_document_cancel",
  "feed_document_release",
  "feed_images_authorize",
  "feed_image_prepare",
  "feed_images_release",
  "feed_items_mark_read",
  "feed_sync_source",
  "feed_sync_all",
  "feed_sync_batch",
  "feed_opml_import",
  "feed_opml_export",
] as const;

const FIXED_QUERY: FeedItemQuery = {
  view: "inbox",
  sourceId: "src-1",
  receivedAfter: null,
  cursor: { sortAt: "2026-08-01T08:00:00Z", rowId: 7 },
  limit: 50,
};

describe("feed IPC contract", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("registers all feed commands in Tauri lib.rs", () => {
    const lib = read("src-tauri/src/lib.rs");
    for (const cmd of FEED_COMMANDS) {
      expect(lib).toContain(`commands::feed_commands::${cmd}`);
    }
  });

  it("removes superseded whole-article image and per-document cache commands", () => {
    const lib = read("src-tauri/src/lib.rs");
    const commands = read("src-tauri/src/commands/feed_commands.rs");
    const ipc = read("src/lib/ipc.ts");
    const types = read("src/types/ipc.ts");

    for (const obsolete of [
      "feed_images_prepare",
      "feed_document_cache_clear",
      "feed_images_cancel",
      "feed_source_remove",
    ]) {
      expect(lib).not.toContain(obsolete);
      expect(commands).not.toContain(obsolete);
      expect(ipc).not.toContain(obsolete);
      expect(types).not.toContain(obsolete);
    }
  });

  it("allows only opaque local media schemes in PDF object-src", () => {
    const config = read("src-tauri/tauri.conf.json");
    expect(config).toContain("object-src iris-media: iris-feed-document:");
    expect(config).not.toContain("object-src https:");
    expect(config).not.toContain("object-src file:");
  });

  it("allows the opaque RSS image protocol without permitting remote image hotlinks", () => {
    const config = read("src-tauri/tauri.conf.json");
    expect(config).toContain("img-src 'self' data: iris-feed-image:");
    expect(config).not.toContain("img-src 'self' data: https:");
  });

  it("defines the feed contract in types/ipc.ts with camelCase fields", () => {
    const types = read("src/types/ipc.ts");
    expect(types).toContain(
      'export type FeedView = "inbox" | "today" | "all" | "starred" | "archived"',
    );
    expect(types).toContain("export interface FeedItemQuery");
    expect(types).toContain("receivedAfter");
    expect(types).toContain("export interface FeedItemStatePatch");
    expect(types).toContain("isRead?: boolean");
    expect(types).toContain("export interface FeedChangedEvent");
    expect(types).toContain(
      'kind: "sync_succeeded" | "sync_failed" | "items_changed"',
    );
    expect(types).toContain("errorCode: string | null");
    // 不得出现 snake_case 或 raw payload 字段。
    expect(types).not.toContain("received_after");
    expect(types).not.toContain("sourcePayload");
  });

  it("defines the event name in ipc-events.ts", () => {
    expect(IPC_EVENTS.FEED_CHANGED).toBe("feed:changed");
    expect(IPC_EVENTS.FEED_DOCUMENT_PROGRESS).toBe("feed:document-progress");
  });

  it("subscribes to PDF progress without exposing a URL or local path", async () => {
    const unlisten = vi.fn();
    listen.mockResolvedValue(unlisten);
    const handler = vi.fn();

    await expect(listenFeedDocumentProgress(handler)).resolves.toBe(unlisten);
    expect(listen).toHaveBeenCalledWith(
      "feed:document-progress",
      expect.any(Function),
    );
  });

  it("feedDiscover invokes with bounded url", async () => {
    invoke.mockResolvedValue([]);
    await feedDiscover("https://example.com/feed.xml");
    expect(invoke).toHaveBeenCalledWith("feed_discover", {
      url: "https://example.com/feed.xml",
    });
  });

  it("feedSourceAdd invokes with camelCase input", async () => {
    invoke.mockResolvedValue({
      id: "src-1",
      title: "Example",
      feedUrl: "https://example.com/feed.xml",
      siteUrl: null,
      folderPath: "tech",
      isEnabled: true,
      fetchIntervalMinutes: 60,
      fulltextEnabled: true,
      unreadCount: 0,
      lastCheckedAt: null,
      lastSuccessAt: null,
      nextFetchAt: null,
      consecutiveFailures: 0,
      lastErrorCode: null,
    } satisfies FeedSourceSummary);
    await feedSourceAdd({
      url: "https://example.com/feed.xml",
      title: "Example",
      titleOverride: null,
      folderPath: "tech",
      fetchIntervalMinutes: 60,
    });
    expect(invoke).toHaveBeenCalledWith("feed_source_add", {
      input: {
        url: "https://example.com/feed.xml",
        title: "Example",
        titleOverride: null,
        folderPath: "tech",
        fetchIntervalMinutes: 60,
      },
    });
  });

  it("feedSourceUpdate invokes with camelCase sourceId and patch", async () => {
    invoke.mockResolvedValue(undefined);
    await feedSourceUpdate("src-1", {
      titleOverride: "Renamed",
      fetchIntervalMinutes: 120,
      isEnabled: false,
    });
    expect(invoke).toHaveBeenCalledWith("feed_source_update", {
      sourceId: "src-1",
      patch: {
        titleOverride: "Renamed",
        fetchIntervalMinutes: 120,
        isEnabled: false,
      },
    });
  });

  it("feedSourceItemCount invokes with sourceId", async () => {
    invoke.mockResolvedValue(42);
    await expect(feedSourceItemCount("src-1")).resolves.toBe(42);
    expect(invoke).toHaveBeenCalledWith("feed_source_item_count", {
      sourceId: "src-1",
    });
  });

  it("uses read-only source trash preflight commands", async () => {
    invoke
      .mockResolvedValueOnce({
        itemCount: 5,
        starredCount: 2,
        purgeAfter: "2026-09-12T00:00:00Z",
      })
      .mockResolvedValueOnce(null);
    await expect(feedSourceTrashPreview("src-1")).resolves.toMatchObject({
      starredCount: 2,
    });
    await expect(
      feedSourceTrashMatch("https://example.com/feed.xml"),
    ).resolves.toBeNull();
    expect(invoke).toHaveBeenNthCalledWith(1, "feed_source_trash_preview", {
      sourceId: "src-1",
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "feed_source_trash_match", {
      url: "https://example.com/feed.xml",
    });
  });

  it("keeps RSS recycle bin and on-open fulltext operation explicit", async () => {
    invoke
      .mockResolvedValueOnce({ sources: [], items: [] })
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce(3)
      .mockResolvedValueOnce("queued");

    await expect(feedTrashList()).resolves.toEqual({ sources: [], items: [] });
    await expect(feedTrashRestore("item-1")).resolves.toBeUndefined();
    await expect(feedTrashClear()).resolves.toBe(3);
    await expect(feedFulltextEnqueueItem("item-1")).resolves.toBe("queued");

    expect(invoke).toHaveBeenNthCalledWith(1, "feed_trash_list");
    expect(invoke).toHaveBeenNthCalledWith(2, "feed_trash_restore", {
      itemId: "item-1",
    });
    expect(invoke).toHaveBeenNthCalledWith(3, "feed_trash_clear");
    expect(invoke).toHaveBeenNthCalledWith(4, "feed_fulltext_enqueue_item", {
      itemId: "item-1",
    });
  });

  it("exposes recoverable source removal and opaque media leases", async () => {
    invoke
      .mockResolvedValueOnce(12)
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce(12)
      .mockResolvedValueOnce({
        handle: "lease-1",
        url: "iris-feed-document://localhost/lease-1",
        mimeType: "application/pdf",
        sizeBytes: 4096,
      })
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce(undefined);

    await expect(feedSourceTrash("src-1")).resolves.toBe(12);
    await expect(feedSourceTrashRestore("src-1")).resolves.toBeUndefined();
    await expect(feedSourceTrashPurge("src-1")).resolves.toBe(12);
    await expect(feedDocumentPrepare("item-1")).resolves.toMatchObject({
      handle: "lease-1",
    });
    await expect(feedDocumentCancel("item-1")).resolves.toBeUndefined();
    await expect(feedDocumentRelease("lease-1")).resolves.toBeUndefined();
    await expect(feedImagesAuthorize("item-1")).resolves.toEqual(undefined);
    await expect(feedImagePrepare("item-1", 2, true)).resolves.toEqual(
      undefined,
    );
    await expect(feedImagesRelease(["lease-image-1"])).resolves.toBeUndefined();

    expect(invoke).toHaveBeenNthCalledWith(1, "feed_source_trash", {
      sourceId: "src-1",
    });
    expect(invoke).toHaveBeenNthCalledWith(4, "feed_document_prepare", {
      itemId: "item-1",
    });
    expect(invoke).toHaveBeenNthCalledWith(7, "feed_images_authorize", {
      itemId: "item-1",
    });
    expect(invoke).toHaveBeenNthCalledWith(8, "feed_image_prepare", {
      itemId: "item-1",
      index: 2,
      forceRetry: true,
    });
    expect(invoke).toHaveBeenNthCalledWith(9, "feed_images_release", {
      handles: ["lease-image-1"],
    });
  });

  it("feedItemList invokes with frozen query", async () => {
    invoke.mockResolvedValue([]);
    await feedItemList(FIXED_QUERY);
    expect(invoke).toHaveBeenCalledWith("feed_item_list", {
      query: FIXED_QUERY,
    });
  });

  it("feedItemGet detail type never exposes sourcePayload", async () => {
    invoke.mockResolvedValue({
      summary: {
        rowId: 1,
        id: "item-1",
        sourceId: "src-1",
        sourceTitle: "Example",
        title: "T",
        authorName: null,
        canonicalUrl: "https://example.com/a",
        publishedAt: null,
        receivedAt: "2026-08-01T08:00:00Z",
        sortAt: "2026-08-01T08:00:00Z",
        excerpt: "…",
        isRead: false,
        isStarred: false,
        isArchived: false,
        conversionStatus: "ok",
      },
      contentMarkdown: "# T",
      summaryMarkdown: "",
      siteUrl: "https://example.com/site",
      contentOrigin: "feed",
      fulltextStatus: "not_requested",
      primaryDocument: null,
      fulltextNeedsRefresh: false,
      imagesAuthorized: false,
    } satisfies FeedItemDetail);
    const detail = await feedItemGet("item-1");
    expect(invoke).toHaveBeenCalledWith("feed_item_get", { itemId: "item-1" });
    expect(detail.contentMarkdown).toBe("# T");
    // 类型层保证：详情 DTO 不含 sourcePayload / source_payload。
    expectTypeOf<FeedItemDetail>().not.toMatchTypeOf<{
      sourcePayload: string;
    }>();
    expectTypeOf<FeedItemDetail["summary"]>().not.toMatchTypeOf<{
      sourcePayload: string;
    }>();
  });

  it("feedItemSetState forwards the patch with camelCase fields", async () => {
    invoke.mockResolvedValue(undefined);
    const patch: FeedItemStatePatch = { isRead: true, isArchived: false };
    await feedItemSetState("item-1", patch);
    expect(invoke).toHaveBeenCalledWith("feed_item_set_state", {
      itemId: "item-1",
      patch,
    });
  });

  it("feedItemsMarkRead returns affected count", async () => {
    invoke.mockResolvedValue(12);
    await expect(feedItemsMarkRead(FIXED_QUERY)).resolves.toBe(12);
    expect(invoke).toHaveBeenCalledWith("feed_items_mark_read", {
      query: FIXED_QUERY,
    });
  });

  it("feedSyncSource waits for completion and returns counts", async () => {
    invoke.mockResolvedValue({
      status: "succeeded",
      newItems: 3,
      errorCode: null,
    });
    await expect(feedSyncSource("src-1", true)).resolves.toEqual({
      status: "succeeded",
      newItems: 3,
      errorCode: null,
    });
    expect(invoke).toHaveBeenCalledWith("feed_sync_source", {
      sourceId: "src-1",
      markHistoryRead: true,
    });
  });

  it("feedSyncAll invokes without args", async () => {
    invoke.mockResolvedValue({ total: 0, succeeded: 0, failed: 0 });
    await feedSyncAll();
    expect(invoke).toHaveBeenCalledWith("feed_sync_all");
  });

  it("feedSyncBatch invokes with bounded source IDs", async () => {
    invoke.mockResolvedValue({ total: 2, succeeded: 2, failed: 0 });
    await feedSyncBatch(["src-1", "src-2"], false);
    expect(invoke).toHaveBeenCalledWith("feed_sync_batch", {
      sourceIds: ["src-1", "src-2"],
      markHistoryRead: false,
    });
  });

  it("feedOpmlImport invokes with bounded xml string and dryRun flag", async () => {
    invoke.mockResolvedValue({
      added: 2,
      updated: 1,
      skipped: 0,
      addedIds: ["src-1", "src-2"],
    });
    await expect(feedOpmlImport("<opml/>", true)).resolves.toEqual({
      added: 2,
      updated: 1,
      skipped: 0,
      addedIds: ["src-1", "src-2"],
    });
    expect(invoke).toHaveBeenCalledWith("feed_opml_import", {
      xml: "<opml/>",
      dryRun: true,
    });
  });

  it("feedOpmlExport returns the opml document string", async () => {
    invoke.mockResolvedValue('<?xml version="1.0"?><opml version="2.0"/>');
    await expect(feedOpmlExport()).resolves.toContain("<opml");
    expect(invoke).toHaveBeenCalledWith("feed_opml_export");
  });

  it("OpmlImportResult type is camelCase without internal fields", () => {
    expectTypeOf<{
      added: number;
      updated: number;
      skipped: number;
      addedIds: string[];
    }>().toEqualTypeOf<OpmlImportResult>();
    expectTypeOf<OpmlImportResult>().not.toMatchTypeOf<{
      etag?: string;
    }>();
    expectTypeOf<OpmlImportResult>().not.toMatchTypeOf<{
      readAt?: string;
    }>();
  });

  it("FeedChangedEvent shape is documented without url/body", () => {
    const event: FeedChangedEvent = {
      sourceId: "src-1",
      kind: "sync_failed",
      newItems: 0,
      errorCode: "feed_http_error_500",
    };
    expect(event.errorCode).toBe("feed_http_error_500");
    expectTypeOf<FeedChangedEvent["kind"]>().toEqualTypeOf<
      "sync_succeeded" | "sync_failed" | "items_changed"
    >();
    expectTypeOf<FeedChangedEvent>().not.toMatchTypeOf<{
      url: string;
    }>();
    expectTypeOf<FeedChangedEvent>().not.toMatchTypeOf<{
      body: string;
    }>();
  });

  it("FeedView accepts only the five frozen values", () => {
    const view: FeedChangedEvent["kind"] = "items_changed";
    expect(["inbox", "today", "all", "starred", "archived"]).toContain("inbox");
    expect(view).toBe("items_changed");
  });
});
