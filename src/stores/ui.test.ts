import { beforeEach, describe, expect, it } from "vitest";

import { type Draft, useUiStore } from "./ui";

const draft = (text: string): Draft => ({
  key: "agent:1",
  mode: "reply",
  sourceId: null,
  accountId: "acc",
  to: [],
  cc: [],
  bcc: [],
  subject: "Re: Termin",
  html: `<p>${text}</p>`,
  text,
});

describe("opening a draft", () => {
  beforeEach(() => {
    useUiStore.setState({ drafts: { "agent:1": draft("alt") }, composeKey: null });
  });

  it("keeps what is written here, unless it comes fresh from the backend", () => {
    useUiStore.getState().openDraft(draft("neu"));
    expect(useUiStore.getState().drafts["agent:1"]?.text).toBe("alt");

    useUiStore.getState().closeCompose();
    // An agent changed it since the app started: the backend's version counts.
    useUiStore.getState().openDraft(draft("neu"), true);
    expect(useUiStore.getState().drafts["agent:1"]?.text).toBe("neu");
    expect(useUiStore.getState().composeKey).toBe("agent:1");
  });

  it("leaves a draft that is open right now as it is being written", () => {
    useUiStore.setState({ composeKey: "agent:1" });
    useUiStore.getState().openDraft(draft("neu"), true);
    expect(useUiStore.getState().drafts["agent:1"]?.text).toBe("alt");
  });
});
