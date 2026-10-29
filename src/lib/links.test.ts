import { describe, expect, it } from "vitest";

import { isDeceptiveLink, splitUrl } from "./links";

describe("isDeceptiveLink", () => {
  it("flags a shown address on another site", () => {
    expect(isDeceptiveLink("https://evil.example/login", "www.paypal.com")).toBe(true);
    expect(isDeceptiveLink("https://evil.example/", "https://bank.example.org/konto")).toBe(true);
  });

  it("accepts matching addresses and subdomains", () => {
    expect(isDeceptiveLink("https://www.example.org/a", "example.org")).toBe(false);
    expect(isDeceptiveLink("https://news.example.org/a", "www.example.org")).toBe(false);
    expect(isDeceptiveLink("https://example.org/a?x=1", "https://example.org/a")).toBe(false);
  });

  it("ignores link texts that are not addresses", () => {
    expect(isDeceptiveLink("https://tracking.example/abc", "Jetzt ansehen")).toBe(false);
    expect(isDeceptiveLink("https://example.org", null)).toBe(false);
    expect(isDeceptiveLink("https://example.org", "Version 2.0")).toBe(false);
  });

  it("does not trust lookalikes of the shown domain", () => {
    expect(isDeceptiveLink("https://example.org.evil.example/", "example.org")).toBe(true);
  });
});

describe("splitUrl", () => {
  it("separates the host", () => {
    expect(splitUrl("https://news.example.org/a?b=1")).toEqual({
      before: "https://",
      host: "news.example.org",
      after: "/a?b=1",
    });
  });
});
