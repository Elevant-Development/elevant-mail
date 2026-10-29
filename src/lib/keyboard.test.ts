import { describe, expect, it } from "vitest";

import { adjacentMessageId } from "./keyboard";

const ids = ["a", "b", "c"];

describe("adjacentMessageId", () => {
  it("moves down and up", () => {
    expect(adjacentMessageId(ids, "a", 1)).toBe("b");
    expect(adjacentMessageId(ids, "c", -1)).toBe("b");
  });

  it("stays at the ends", () => {
    expect(adjacentMessageId(ids, "c", 1)).toBe("c");
    expect(adjacentMessageId(ids, "a", -1)).toBe("a");
  });

  it("starts at the first message without a selection", () => {
    expect(adjacentMessageId(ids, null, 1)).toBe("a");
    expect(adjacentMessageId(ids, null, -1)).toBe("a");
  });

  it("starts at the first message when the selection is gone", () => {
    expect(adjacentMessageId(ids, "x", 1)).toBe("a");
  });

  it("returns nothing for an empty list", () => {
    expect(adjacentMessageId([], null, 1)).toBeNull();
  });
});
