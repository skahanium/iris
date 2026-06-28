import { describe, expect, it } from "vitest";

import {
  classifyWorkspacePath,
  parseWikiMediaReference,
  resolveAttachmentRole,
} from "@/lib/media-reference";

describe("media reference contract", () => {
  it("parses core Obsidian media embeds and aliases", () => {
    expect(parseWikiMediaReference("![[diagram.png]]")).toEqual({
      alias: null,
      embed: true,
      raw: "![[diagram.png]]",
      target: "diagram.png",
    });

    expect(parseWikiMediaReference("![[paper.pdf|证据材料]]")).toEqual({
      alias: "证据材料",
      embed: true,
      raw: "![[paper.pdf|证据材料]]",
      target: "paper.pdf",
    });
  });

  it("parses media links separately from embeds", () => {
    expect(parseWikiMediaReference("[[clip.mp4]]")).toEqual({
      alias: null,
      embed: false,
      raw: "[[clip.mp4]]",
      target: "clip.mp4",
    });
  });

  it("classifies supported workspace media paths without treating every file as a note", () => {
    expect(classifyWorkspacePath("notes/case.md")).toEqual({
      kind: "note",
      mediaKind: null,
    });
    expect(classifyWorkspacePath("evidence/paper.pdf")).toEqual({
      kind: "media",
      mediaKind: "pdf",
    });
    expect(classifyWorkspacePath("assets/clip.mp4")).toEqual({
      kind: "media",
      mediaKind: "video",
    });
    expect(classifyWorkspacePath("archive.zip")).toEqual({
      kind: "unsupported",
      mediaKind: null,
    });
  });

  it("uses attachment roots to distinguish attachment role from formal vault files", () => {
    expect(resolveAttachmentRole("assets/photo.png", ["assets"])).toBe(
      "attachment",
    );
    expect(resolveAttachmentRole("materials/photo.png", ["assets"])).toBe(
      "formal",
    );
  });
});
