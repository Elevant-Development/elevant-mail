import { describe, expect, it } from "vitest";

import { de } from "@/locales/de";

import { languages, type MessageKey, resolveLanguage, tCount } from "./i18n";

describe("resolveLanguage", () => {
  it("follows the system when there is a translation", () => {
    expect(resolveLanguage("system", "de-AT")).toBe("de");
    expect(resolveLanguage("system", "en-GB")).toBe("en");
  });

  it("falls back to English for other system languages", () => {
    expect(resolveLanguage("system", "fr-FR")).toBe("en");
  });

  it("uses the chosen language", () => {
    expect(resolveLanguage("de", "en-US")).toBe("de");
  });
});

describe("translations", () => {
  it("keep the placeholders of the German text", () => {
    const placeholders = (text: string) => [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();
    for (const [language, { messages }] of Object.entries(languages)) {
      for (const key of Object.keys(de) as MessageKey[]) {
        expect(placeholders(messages[key]), `${language}: ${key}`).toEqual(placeholders(de[key]));
      }
    }
  });
});

describe("tCount", () => {
  it("picks one or other and fills in the count", () => {
    expect(tCount("undo.archived", 1)).toBe("Mail archiviert");
    expect(tCount("undo.archived", 3)).toBe("3 Mails archiviert");
    expect(tCount("undo.moved", 2, { folder: "Projekte" })).toBe(
      "2 Mails nach „Projekte“ verschoben",
    );
  });

  it("has both forms for every counted key", () => {
    for (const key of Object.keys(de)) {
      if (key.endsWith(".one")) expect(Object.keys(de)).toContain(key.replace(/one$/, "other"));
    }
  });
});
