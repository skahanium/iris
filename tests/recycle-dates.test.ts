import { describe, expect, it } from "vitest";

import {
  recycleDaysRemaining,
  recycleRetentionLabel,
} from "@/lib/recycle-dates";

describe("recycle-dates", () => {
  it("computes whole days remaining", () => {
    const now = Date.parse("2026-05-27T12:00:00.000Z");
    const expires = "2026-05-29T12:00:00.000Z";
    expect(recycleDaysRemaining(expires, now)).toBe(2);
  });

  it("labels urgent expiry", () => {
    expect(recycleRetentionLabel(0)).toBe("即将永久删除");
    expect(recycleRetentionLabel(1)).toBe("剩余 1 天");
    expect(recycleRetentionLabel(5)).toBe("剩余 5 天");
  });
});
