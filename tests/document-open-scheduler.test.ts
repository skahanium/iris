import { describe, expect, it, vi } from "vitest";

import {
  DocumentOpenScheduler,
  type DocumentOpenJob,
} from "../src/lib/document-open-scheduler";

function job(
  key: string,
  priority: DocumentOpenJob<string>["priority"],
  order: string[],
): DocumentOpenJob<string> {
  return {
    key,
    namespace: "vault-a",
    path: `${key}.md`,
    source: "test",
    priority,
    run: async () => {
      order.push(key);
      return key;
    },
  };
}

describe("DocumentOpenScheduler", () => {
  it("runs foreground work before queued warm work", async () => {
    const order: string[] = [];
    const scheduler = new DocumentOpenScheduler({ maxConcurrent: 1 });

    const warm = scheduler.enqueue(job("warm", "warm", order));
    const foreground = scheduler.enqueue(
      job("foreground", "foreground", order),
    );

    await expect(foreground.promise).resolves.toBe("foreground");
    await expect(warm.promise).resolves.toBe("warm");
    expect(order).toEqual(["foreground", "warm"]);
  });

  it("coalesces queued jobs with the same key", async () => {
    const run = vi.fn(async () => "ready");
    const scheduler = new DocumentOpenScheduler({ maxConcurrent: 1 });

    const first = scheduler.enqueue({
      key: "same",
      namespace: "vault-a",
      path: "same.md",
      source: "quick-open",
      priority: "warm",
      run,
    });
    const second = scheduler.enqueue({
      key: "same",
      namespace: "vault-a",
      path: "same.md",
      source: "file-tree",
      priority: "foreground",
      run,
    });

    await expect(second.promise).resolves.toBe("ready");
    await expect(first.promise).resolves.toBe("ready");
    expect(first.promise).toBe(second.promise);
    expect(run).toHaveBeenCalledTimes(1);
  });

  it("cancels queued speculative work without cancelling foreground work", async () => {
    const order: string[] = [];
    const scheduler = new DocumentOpenScheduler({ maxConcurrent: 1 });
    const background = scheduler.enqueue(
      job("background", "background", order),
    );
    const foreground = scheduler.enqueue(
      job("foreground", "foreground", order),
    );

    background.cancel();

    await expect(background.promise).rejects.toMatchObject({
      name: "AbortError",
    });
    await expect(foreground.promise).resolves.toBe("foreground");
    expect(order).toEqual(["foreground"]);
  });
  it("does not let a stale same-key warm cancel handle cancel an upgraded foreground job", async () => {
    const run = vi.fn(async () => "ready");
    const scheduler = new DocumentOpenScheduler({ maxConcurrent: 1 });

    const warm = scheduler.enqueue({
      key: "same",
      namespace: "vault-a",
      path: "same.md",
      source: "startup",
      priority: "background",
      run,
    });
    const foreground = scheduler.enqueue({
      key: "same",
      namespace: "vault-a",
      path: "same.md",
      source: "quick-open",
      priority: "foreground",
      run,
    });

    warm.cancel();

    await expect(foreground.promise).resolves.toBe("ready");
    await expect(warm.promise).resolves.toBe("ready");
    expect(run).toHaveBeenCalledTimes(1);
  });

  it("starts foreground work even when background work already occupies the concurrency slot", async () => {
    const order: string[] = [];
    const scheduler = new DocumentOpenScheduler({ maxConcurrent: 1 });
    let releaseBackground!: () => void;

    const background = scheduler.enqueue({
      key: "background",
      namespace: "vault-a",
      path: "background.md",
      source: "startup",
      priority: "background",
      run: async () => {
        order.push("background");
        await new Promise<void>((resolve) => {
          releaseBackground = resolve;
        });
        return "background";
      },
    });
    await Promise.resolve();
    expect(order).toEqual(["background"]);

    const foreground = scheduler.enqueue(
      job("foreground", "foreground", order),
    );
    await Promise.resolve();

    await expect(foreground.promise).resolves.toBe("foreground");
    expect(order).toEqual(["background", "foreground"]);

    releaseBackground();
    await expect(background.promise).resolves.toBe("background");
  });

  it("starts hot tab work even when background work already occupies the concurrency slot", async () => {
    const order: string[] = [];
    const scheduler = new DocumentOpenScheduler({ maxConcurrent: 1 });
    let releaseBackground!: () => void;

    const background = scheduler.enqueue({
      key: "background",
      namespace: "vault-a",
      path: "background.md",
      source: "startup",
      priority: "background",
      run: async () => {
        order.push("background");
        await new Promise<void>((resolve) => {
          releaseBackground = resolve;
        });
        return "background";
      },
    });
    await Promise.resolve();
    expect(order).toEqual(["background"]);

    const hot = scheduler.enqueue(job("hot", "hot", order));
    await Promise.resolve();

    await expect(hot.promise).resolves.toBe("hot");
    expect(order).toEqual(["background", "hot"]);

    releaseBackground();
    await expect(background.promise).resolves.toBe("background");
  });

  it("does not start every queued interactive job while the scheduler is over the concurrency limit", async () => {
    const order: string[] = [];
    const scheduler = new DocumentOpenScheduler({ maxConcurrent: 1 });
    let releaseBackground!: () => void;
    let releaseFirstForeground!: () => void;

    const background = scheduler.enqueue({
      key: "background",
      namespace: "vault-a",
      path: "background.md",
      source: "startup",
      priority: "background",
      run: async () => {
        order.push("background");
        await new Promise<void>((resolve) => {
          releaseBackground = resolve;
        });
        return "background";
      },
    });
    await Promise.resolve();

    const firstForeground = scheduler.enqueue({
      key: "foreground-a",
      namespace: "vault-a",
      path: "foreground-a.md",
      source: "test",
      priority: "foreground",
      run: async () => {
        order.push("foreground-a");
        await new Promise<void>((resolve) => {
          releaseFirstForeground = resolve;
        });
        return "foreground-a";
      },
    });
    const secondForeground = scheduler.enqueue(
      job("foreground-b", "foreground", order),
    );
    await Promise.resolve();

    expect(order).toEqual(["background", "foreground-a"]);

    releaseFirstForeground();
    await expect(firstForeground.promise).resolves.toBe("foreground-a");
    await expect(secondForeground.promise).resolves.toBe("foreground-b");
    expect(order).toEqual(["background", "foreground-a", "foreground-b"]);

    releaseBackground();
    await expect(background.promise).resolves.toBe("background");
  });
});
