import { describe, expect, it } from "vitest";

import { contentHash64 } from "@/lib/content-hash";
import { sha256Utf8 } from "@/lib/sha256-utf8";

describe("sha256Utf8", () => {
  it("matches FIPS 180-4 empty and abc vectors", () => {
    expect(sha256Utf8("")).toBe(
      "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    );
    expect(sha256Utf8("abc")).toBe(
      "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    );
  });

  it("hashes UTF-8 bytes as SHA-256, not FNV-1a", () => {
    const value = "opened body\n中文";
    expect(sha256Utf8(value)).toBe(
      "ad0e3278d0ef81807fb97652cbc3380850caf9ed7e9ba57a045be2ca2a39ce19",
    );
    expect(sha256Utf8(value)).not.toBe(contentHash64(value));
  });
});
