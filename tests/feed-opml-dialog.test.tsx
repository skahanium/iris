//! FeedOpmlDialog 契约测试：OPML 导入预览→确认、同步选项、导出流程、
//! 稳定错误码安全文案（不展示 URL/正文/OPML 内容）。

import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { feedOpmlImport, feedOpmlExport, feedSyncBatch } = vi.hoisted(() => ({
  feedOpmlImport: vi.fn(),
  feedOpmlExport: vi.fn(),
  feedSyncBatch: vi.fn(),
}));

const { dialogOpen, dialogSave, fsReadTextFile, fsWriteTextFile } = vi.hoisted(
  () => ({
    dialogOpen: vi.fn(),
    dialogSave: vi.fn(),
    fsReadTextFile: vi.fn(),
    fsWriteTextFile: vi.fn(),
  }),
);

vi.mock("@/lib/ipc", () => ({
  feedOpmlImport,
  feedOpmlExport,
  feedSyncBatch,
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: dialogOpen,
  save: dialogSave,
}));

vi.mock("@tauri-apps/plugin-fs", () => ({
  readTextFile: fsReadTextFile,
  writeTextFile: fsWriteTextFile,
}));

import { FeedOpmlDialog } from "@/components/feed/FeedOpmlDialog";

const SAMPLE_XML = `<?xml version="1.0" encoding="UTF-8"?>
<opml version="2.0"><body>
  <outline text="技术">
    <outline type="rss" text="Rust 源" xmlUrl="https://example.com/rust.xml"/>
  </outline>
</body></opml>`;

async function flushReactUpdates() {
  await act(async () => {
    await Promise.resolve();
  });
}

beforeEach(() => {
  vi.clearAllMocks();
  dialogOpen.mockResolvedValue("/tmp/subscriptions.opml");
  dialogSave.mockResolvedValue("/tmp/export.opml");
  fsReadTextFile.mockResolvedValue(SAMPLE_XML);
  fsWriteTextFile.mockResolvedValue(undefined);
  feedOpmlImport.mockResolvedValue({
    added: 1,
    updated: 0,
    skipped: 0,
    addedIds: ["src-1"],
  });
  feedOpmlExport.mockResolvedValue('<opml version="2.0"><body></body></opml>');
  feedSyncBatch.mockResolvedValue({
    total: 1,
    succeeded: 1,
    notModified: 0,
    failed: 0,
    newItems: 2,
    outcomes: [],
  });
});

afterEach(() => {
  cleanup();
});

describe("FeedOpmlDialog 导入流程", () => {
  it("预览计数后确认导入并可选同步新订阅", async () => {
    const onSourcesChanged = vi.fn();
    const onOpenChange = vi.fn();
    render(
      <FeedOpmlDialog
        open
        onOpenChange={onOpenChange}
        onSourcesChanged={onSourcesChanged}
      />,
    );

    fireEvent.click(screen.getByTestId("feed-opml-pick"));
    await waitFor(() =>
      expect(screen.getByTestId("feed-opml-preview-added").textContent).toBe(
        "1",
      ),
    );
    expect(dialogOpen).toHaveBeenCalledTimes(1);
    expect(fsReadTextFile).toHaveBeenCalledWith("/tmp/subscriptions.opml");
    expect(feedOpmlImport).toHaveBeenCalledWith(SAMPLE_XML, true);

    // 默认勾选同步新订阅；历史默认已读。
    fireEvent.click(screen.getByTestId("feed-opml-import-confirm"));
    await waitFor(() => expect(onSourcesChanged).toHaveBeenCalledTimes(1));
    expect(feedOpmlImport).toHaveBeenLastCalledWith(SAMPLE_XML, false);
    expect(feedSyncBatch).toHaveBeenCalledWith(["src-1"], true);
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("勾选历史未读时以未读方式同步", async () => {
    const onSourcesChanged = vi.fn();
    render(
      <FeedOpmlDialog
        open
        onOpenChange={vi.fn()}
        onSourcesChanged={onSourcesChanged}
      />,
    );
    fireEvent.click(screen.getByTestId("feed-opml-pick"));
    await waitFor(() => screen.getByTestId("feed-opml-preview-added"));

    fireEvent.click(screen.getByTestId("feed-opml-history-unread"));
    fireEvent.click(screen.getByTestId("feed-opml-import-confirm"));
    await waitFor(() => expect(onSourcesChanged).toHaveBeenCalledTimes(1));
    expect(feedSyncBatch).toHaveBeenCalledWith(["src-1"], false);
  });

  it("取消同步选项时不发起首次同步", async () => {
    const onSourcesChanged = vi.fn();
    render(
      <FeedOpmlDialog
        open
        onOpenChange={vi.fn()}
        onSourcesChanged={onSourcesChanged}
      />,
    );
    fireEvent.click(screen.getByTestId("feed-opml-pick"));
    await waitFor(() => screen.getByTestId("feed-opml-preview-added"));

    fireEvent.click(screen.getByTestId("feed-opml-sync-new"));
    fireEvent.click(screen.getByTestId("feed-opml-import-confirm"));
    await waitFor(() => expect(onSourcesChanged).toHaveBeenCalledTimes(1));
    expect(feedSyncBatch).not.toHaveBeenCalled();
  });

  it("预览取消不执行导入", async () => {
    const onSourcesChanged = vi.fn();
    render(
      <FeedOpmlDialog
        open
        onOpenChange={vi.fn()}
        onSourcesChanged={onSourcesChanged}
      />,
    );
    fireEvent.click(screen.getByTestId("feed-opml-pick"));
    await waitFor(() => screen.getByTestId("feed-opml-preview-added"));

    fireEvent.click(screen.getByTestId("feed-opml-import-cancel"));
    expect(feedOpmlImport).toHaveBeenCalledTimes(1); // 只有 dryRun
    expect(feedOpmlImport).toHaveBeenCalledWith(SAMPLE_XML, true);
    expect(onSourcesChanged).not.toHaveBeenCalled();
  });

  it("文件选择取消不报错", async () => {
    dialogOpen.mockResolvedValue(null);
    render(
      <FeedOpmlDialog open onOpenChange={vi.fn()} onSourcesChanged={vi.fn()} />,
    );
    fireEvent.click(screen.getByTestId("feed-opml-pick"));
    await flushReactUpdates();
    expect(fsReadTextFile).not.toHaveBeenCalled();
    expect(feedOpmlImport).not.toHaveBeenCalled();
  });

  it("XXE/超限等稳定错误只展示安全文案", async () => {
    feedOpmlImport.mockRejectedValue({ code: "feed_xml_unsafe_declaration" });
    render(
      <FeedOpmlDialog open onOpenChange={vi.fn()} onSourcesChanged={vi.fn()} />,
    );
    fireEvent.click(screen.getByTestId("feed-opml-pick"));
    await waitFor(() =>
      expect(
        screen.getByTestId("feed-opml-import-error").textContent,
      ).toContain("DTD/ENTITY"),
    );
    // 错误文案不得包含文件内容或 URL。
    const text = screen.getByTestId("feed-opml-import-error").textContent ?? "";
    expect(text).not.toContain("example.com");
    expect(text).not.toContain("<opml");
  });

  it("超大文件错误码映射为安全文案", async () => {
    feedOpmlImport.mockRejectedValue({ code: "feed_opml_too_large" });
    render(
      <FeedOpmlDialog open onOpenChange={vi.fn()} onSourcesChanged={vi.fn()} />,
    );
    fireEvent.click(screen.getByTestId("feed-opml-pick"));
    await waitFor(() =>
      expect(
        screen.getByTestId("feed-opml-import-error").textContent,
      ).toContain("5 MiB"),
    );
  });

  it("执行阶段失败停留在预览并允许重试", async () => {
    const onSourcesChanged = vi.fn();
    render(
      <FeedOpmlDialog
        open
        onOpenChange={vi.fn()}
        onSourcesChanged={onSourcesChanged}
      />,
    );
    fireEvent.click(screen.getByTestId("feed-opml-pick"));
    await waitFor(() => screen.getByTestId("feed-opml-preview-added"));

    feedOpmlImport.mockRejectedValueOnce({ code: "feed_opml_parse_failed" });
    fireEvent.click(screen.getByTestId("feed-opml-import-confirm"));
    await waitFor(() =>
      expect(
        screen.getByTestId("feed-opml-import-error").textContent,
      ).toContain("无法解析"),
    );
    expect(onSourcesChanged).not.toHaveBeenCalled();
    // 预览仍可见，可再次确认。
    expect(screen.getByTestId("feed-opml-preview-added")).toBeTruthy();
  });
});

describe("FeedOpmlDialog 导出流程", () => {
  it("导出内容经保存对话框写入所选位置", async () => {
    const onSourcesChanged = vi.fn();
    render(
      <FeedOpmlDialog
        open
        onOpenChange={vi.fn()}
        onSourcesChanged={onSourcesChanged}
      />,
    );
    fireEvent.click(screen.getByTestId("feed-opml-export"));
    await waitFor(() => screen.getByTestId("feed-opml-export-done"));
    expect(feedOpmlExport).toHaveBeenCalledTimes(1);
    expect(dialogSave).toHaveBeenCalledTimes(1);
    expect(fsWriteTextFile).toHaveBeenCalledWith(
      "/tmp/export.opml",
      '<opml version="2.0"><body></body></opml>',
    );
    expect(onSourcesChanged).not.toHaveBeenCalled();
  });

  it("保存取消不写文件", async () => {
    dialogSave.mockResolvedValue(null);
    render(
      <FeedOpmlDialog open onOpenChange={vi.fn()} onSourcesChanged={vi.fn()} />,
    );
    fireEvent.click(screen.getByTestId("feed-opml-export"));
    await flushReactUpdates();
    expect(fsWriteTextFile).not.toHaveBeenCalled();
  });

  it("导出失败展示安全文案", async () => {
    feedOpmlExport.mockRejectedValue(new Error("boom"));
    render(
      <FeedOpmlDialog open onOpenChange={vi.fn()} onSourcesChanged={vi.fn()} />,
    );
    fireEvent.click(screen.getByTestId("feed-opml-export"));
    await waitFor(() =>
      expect(
        screen.getByTestId("feed-opml-export-error").textContent,
      ).toContain("导出失败"),
    );
  });
});
