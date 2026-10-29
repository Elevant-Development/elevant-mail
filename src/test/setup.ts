// i18n picks the interface language from the system when it loads. Tests pin German, which
// defines the keys, so they pass the same on every machine.
Object.defineProperty(navigator, "language", { value: "de-DE", configurable: true });
