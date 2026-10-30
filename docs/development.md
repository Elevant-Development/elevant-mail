# Development

How to build, run and check Elevant Mail from source. For how to propose changes, see
[CONTRIBUTING.md](../CONTRIBUTING.md); for scope, architecture and decisions, the
[project brief](project-brief.md).

Built with [Tauri 2](https://tauri.app), React, TypeScript, Tailwind CSS and Rust.

## Requirements

|         | Linux (Ubuntu/Debian)                                  | Windows 10/11                                                                                                                                                  |
| ------- | ------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Rust    | stable via [rustup](https://rustup.rs) (1.98 or newer) | stable via rustup, MSVC toolchain                                                                                                                              |
| Node.js | 22 or newer                                            | 22 or newer                                                                                                                                                    |
| pnpm    | 12 (`npm install -g pnpm`)                             | 12 (`npm install -g pnpm`)                                                                                                                                     |
| System  | packages below                                         | [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with "Desktop development with C++"; WebView2 is part of Windows 10/11 |

System packages on Ubuntu/Debian:

```bash
sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file pkg-config \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

## Run

```bash
pnpm install
pnpm tauri dev    # desktop window with hot reload
pnpm dev          # frontend only, in the browser at http://localhost:1420
```

`pnpm dev` runs the interface in a normal browser with sample data (`src/lib/mock.ts`). That is
handy for layout, styling and translations; everything that needs the backend is checked in the
Tauri window.

## Checks

```bash
pnpm check        # typecheck, ESLint, Prettier, rustfmt, clippy, tests
pnpm format       # Prettier and rustfmt
cargo deny check  # licences and known vulnerabilities (cargo install cargo-deny)
```

CI (`.github/workflows/ci.yml`) runs the same on Linux and Windows for every push and pull
request. It also checks that the generated TypeScript types are current and that the build loads
nothing from the web (`scripts/check-no-external-urls.mjs`).

## Generated types

Data types shared with the frontend are defined in Rust and exported with
[ts-rs](https://github.com/Aleph-Alpha/ts-rs) into `src/lib/bindings/`. The export runs with the
Rust tests:

```bash
pnpm test:rust
```

Commit the changed files in `src/lib/bindings/` together with the Rust change.

## Translations

UI strings live in `src/locales/`, one file per language. German (`de.ts`) defines the keys; every
other language is a `Record<MessageKey, string>`, so the typecheck fails when a key is missing or
unknown. A test checks that each translation keeps the placeholders (`{count}`, `{name}` …) of the
German text.

To add a language:

1. Copy `src/locales/en.ts` to `src/locales/<code>.ts` (for example `fr.ts`) and translate the
   values. Keep the keys and placeholders.
2. Register it in `languages` in `src/lib/i18n.ts` with its native name and a default locale.
3. Check it with `pnpm dev`: Appearance → Language.

The sample data in `src/lib/mock.ts` exists in English and German only; other languages show the
English samples.

## Build installers

```bash
pnpm tauri build
```

The bundles end up in `target/release/bundle/`:

- Linux: `deb/*.deb`, `appimage/*.AppImage` (and `rpm/*.rpm`)
- Windows: `nsis/*-setup.exe`

## Releases

1. Raise the version in `package.json`, `Cargo.toml` and `src-tauri/tauri.conf.json`.
2. Add the version's section to `CHANGELOG.md` (`## 0.3.0 – date`, below it `###` headings).
3. Commit, then tag: `git tag -a v0.3.0 -m "Elevant Mail 0.3.0" && git push origin v0.3.0`.
4. `.github/workflows/release.yml` builds the Linux and Windows installers into a draft release
   whose text is that section, and fails without one. Test the installers and publish the draft.

## Troubleshooting

**Blank or flickering window on Linux.** WebKitGTK has known rendering problems on some GPUs,
especially hybrid Nvidia setups. Start with:

```bash
WEBKIT_DISABLE_DMABUF_RENDERER=1 pnpm tauri dev
```

**Keyring.** On Linux, saving passwords needs a running Secret Service, which GNOME Keyring
provides. On KDE, use KWallet with its Secret Service interface enabled.

## Project layout

```text
crates/core/         mail, cache, credentials and sign-in, independent of the desktop shell
src-tauri/           desktop shell (Tauri): window, commands, mail-asset protocol
src/                 React frontend
  components/        layout, mail, accounts, common, ui (shadcn/ui)
  lib/tauri.ts       the only bridge to the backend; data types come from Rust (lib/bindings)
  lib/mock.ts        sample data for working on the interface in a browser
  lib/i18n.ts        language selection and t()
  locales/           UI strings, one file per language
docs/                project brief and this guide
```
