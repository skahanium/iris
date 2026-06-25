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
});
