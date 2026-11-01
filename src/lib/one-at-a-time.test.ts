import { describe, expect, it } from "vitest";

import { oneAtATime } from "./one-at-a-time";

describe("oneAtATime", () => {
  it("runs once more after a run that was asked for again, not once per asking", async () => {
    const start = oneAtATime();
    const runs: string[] = [];
    let release = () => {};
    const job = (name: string) => async () => {
      runs.push(name);
      if (name === "first") await new Promise<void>((resolve) => (release = resolve));
    };
    const first = start("a", job("first"));
    const second = start("a", job("second"));
    const third = start("a", job("third"));
    // Another key does not wait.
    await start("b", job("other"));
    expect(runs).toEqual(["first", "other"]);
    release();
    await Promise.all([first, second, third]);
    expect(runs).toEqual(["first", "other", "second"]);
    // Free again afterwards.
    await start("a", job("fourth"));
    expect(runs.at(-1)).toBe("fourth");
  });
});
