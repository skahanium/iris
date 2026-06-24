import { describe, expect, it, vi } from "vitest";

import {
  beginHomeOpenLoading,
  cancelHomeOpenTransitions,
  clearHomeOpenLoading,
  failHomeOpenLoading,
  type HomePendingOpen,
} from "@/lib/home-open-transition";

describe("home open transition", () => {
  it("immediately leaves Home into a target loading view", () => {
    const sequenceRef = { current: 0 };
    const setHomeActive = vi.fn();
    const setPendingOpen = vi.fn();

    const sequence = beginHomeOpenLoading({
      path: "new.md",
      title: "New",
      sequenceRef,
      setHomeActive,
      setPendingOpen,
    });

    expect(sequence).toBe(1);
    expect(setHomeActive).toHaveBeenCalledWith(false);
    expect(setPendingOpen).toHaveBeenCalledWith({
      kind: "note",
      path: "new.md",
      sequence: 1,
      title: "New",
    });
  });

  it("only allows the latest target request to clear loading", () => {
    const sequenceRef = { current: 0 };
    const setHomeActive = vi.fn();
    const setPendingOpen = vi.fn();
    const first = beginHomeOpenLoading({
      path: "b.md",
      title: "B",
      sequenceRef,
      setHomeActive,
      setPendingOpen,
    });
    const second = beginHomeOpenLoading({
      path: "c.md",
      title: "C",
      sequenceRef,
      setHomeActive,
      setPendingOpen,
    });

    expect(
      clearHomeOpenLoading({
        activePath: "b.md",
        path: "b.md",
        sequence: first,
        sequenceRef,
        setPendingOpen,
      }),
    ).toBe(false);
    expect(setPendingOpen).toHaveBeenLastCalledWith({
      kind: "note",
      path: "c.md",
      sequence: second,
      title: "C",
    });

    expect(
      clearHomeOpenLoading({
        activePath: "c.md",
        path: "c.md",
        sequence: second,
        sequenceRef,
        setPendingOpen,
      }),
    ).toBe(true);
    expect(setPendingOpen).toHaveBeenLastCalledWith(null);
  });

  it("ignores pending opens after the user explicitly returns Home", () => {
    const sequenceRef = { current: 0 };
    const setHomeActive = vi.fn();
    const setPendingOpen = vi.fn();
    const sequence = beginHomeOpenLoading({
      path: "new.md",
      title: "New",
      sequenceRef,
      setHomeActive,
      setPendingOpen,
    });

    cancelHomeOpenTransitions(sequenceRef, setPendingOpen);

    expect(
      clearHomeOpenLoading({
        activePath: "new.md",
        path: "new.md",
        sequence,
        sequenceRef,
        setPendingOpen,
      }),
    ).toBe(false);
    expect(setPendingOpen).toHaveBeenLastCalledWith(null);
  });

  it("marks the latest loading request as failed without restoring the old document", () => {
    const sequenceRef = { current: 0 };
    const setHomeActive = vi.fn();
    const setPendingOpen = vi.fn();
    const pending: HomePendingOpen = {
      kind: "note",
      path: "missing.md",
      sequence: 1,
      title: "Missing",
    };
    const sequence = beginHomeOpenLoading({
      path: pending.path,
      title: pending.title,
      sequenceRef,
      setHomeActive,
      setPendingOpen,
    });

    expect(
      failHomeOpenLoading({
        message: "无法打开笔记",
        pending,
        sequence,
        sequenceRef,
        setPendingOpen,
      }),
    ).toBe(true);
    expect(setHomeActive).toHaveBeenCalledWith(false);
    expect(setPendingOpen).toHaveBeenLastCalledWith({
      ...pending,
      error: "无法打开笔记",
    });
  });

  it("represents new-note creation as a loading target without an old active path", () => {
    const sequenceRef = { current: 0 };
    const setHomeActive = vi.fn();
    const setPendingOpen = vi.fn();

    const sequence = beginHomeOpenLoading({
      kind: "new-note",
      path: null,
      title: "新建笔记",
      sequenceRef,
      setHomeActive,
      setPendingOpen,
    });

    expect(sequence).toBe(1);
    expect(setHomeActive).toHaveBeenCalledWith(false);
    expect(setPendingOpen).toHaveBeenCalledWith({
      kind: "new-note",
      path: null,
      sequence: 1,
      title: "新建笔记",
    });
  });
});
