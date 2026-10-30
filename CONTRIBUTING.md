# Contributing to Elevant Mail

Thanks for helping. Bug reports, translations, ideas and code are all welcome.

## Reporting bugs and ideas

Open an [issue](https://github.com/Elevant-Development/elevant-mail/issues). For a bug, include:

- the Elevant Mail version and your system (for example "0.3.0, Ubuntu 24.04, .deb")
- the kind of account (Gmail, Microsoft 365, web.de, self-hosted …)
- what you did, what you expected and what happened instead

Please never post passwords, tokens or the content of private mail. If a specific mail shows
wrongly, describe it or build a small sample mail without personal data.

**Security problems** do not belong in a public issue. Report them privately as described in
[SECURITY.md](SECURITY.md).

## Translations

The interface is in English and German, and more languages are welcome. A translation is one file
in `src/locales/`; [docs/development.md](docs/development.md#translations) explains the steps. You
can check your translation in the browser with `pnpm dev` without building the desktop app.

## Code

### Before you start

For anything bigger than a small fix, open an issue first and describe what you plan. Elevant
Mail has a clear scope (see the [project brief](docs/project-brief.md)), and a short discussion
up front saves you work that might not fit.

Set up the project as described in [docs/development.md](docs/development.md).

### Principles

These are not negotiable, and reviews check them:

- **Privacy:** no telemetry, no analytics, no calls to services the user did not trigger. Nothing
  loads from the web: fonts, icons and styles are bundled, no CDNs.
- **Local first:** the sync writes to SQLite, the interface reads only from SQLite.
- **Mail is untrusted:** HTML is sanitized and shown sandboxed, scripts never run, remote content
  only after consent.
- **Secrets** go into the system keyring, never into files, logs or error messages.
- **Mail logic stays in `crates/core`,** independent of Tauri.

### Conventions

- Code, comments, commit messages and docs are in **English**.
- Every text in the interface goes through an i18n key. Add new keys to every file in
  `src/locales/`; the typecheck fails until all languages have them. If you cannot translate a
  key, use the English text and say so in the pull request.
- Errors shown to the user come with a concrete next step, not just a message.
- Follow the style of the surrounding code. `pnpm format` runs Prettier and rustfmt.
- Commit messages: a short summary in the imperative ("Add attachment preview", "Fix unread count
  for Gmail labels"), details in the body if needed.

### Pull requests

1. Fork the repository and create a branch from `main`.
2. Keep a pull request to one topic. Add or update tests for what you change.
3. Run the checks locally before pushing:

   ```bash
   pnpm check
   cargo deny check
   ```

4. If you changed Rust types that the frontend uses, commit the regenerated files in
   `src/lib/bindings/` (see [docs/development.md](docs/development.md#generated-types)).
5. For interface changes, add a screenshot to the pull request, ideally in dark and light mode.
6. Open the pull request against `main` and describe what changes and why. CI runs on Linux and
   Windows and has to pass.

## License

Elevant Mail is licensed under the [GPL-3.0-or-later](LICENSE). By contributing, you agree that
your contribution is licensed under the same terms. There is no contributor license agreement.
