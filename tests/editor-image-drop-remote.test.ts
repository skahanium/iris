import { describe, expect, it, vi } from "vitest";

import { localizeRemoteImagesInHtml } from "@/components/editor/extensions/EditorImageDropExtension";
import * as ipc from "@/lib/ipc";

vi.mock("@/lib/ipc", () => ({
  vaultAssetImportUrl: vi.fn(),
  vaultAssetWrite: vi.fn(),
}));

const mockedImport = vi.mocked(ipc.vaultAssetImportUrl);

describe("localizeRemoteImagesInHtml", () => {
  it("replaces remote https images with local vault asset paths", async () => {
    mockedImport.mockResolvedValue("assets/abc.png");

    const result = await localizeRemoteImagesInHtml(
      '<p>before<img src="https://example.com/a.png" alt="a">after</p>',
    );

    expect(result).toContain('src="assets/abc.png"');
    expect(result).toContain("before");
    expect(result).toContain("after");
    expect(mockedImport).toHaveBeenCalledWith("https://example.com/a.png");
  });

  it("removes images that fail to download instead of leaving broken links", async () => {
    mockedImport.mockRejectedValue(new Error("download failed"));

    const result = await localizeRemoteImagesInHtml(
      '<p><img src="https://example.com/b.png" alt="b"></p>',
    );

    expect(result).not.toContain("example.com");
    expect(result).not.toContain("<img");
  });
});
