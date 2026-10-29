# Elevant Mail

A fast, private desktop mail client for Linux and Windows. Several IMAP accounts, one unified
inbox, a calm dark interface.

- No paywall, no account, no cloud backend, no telemetry
- Mail is cached locally and stays readable offline
- Passwords live in the system keyring, never in files

> **Status:** v0.2. Reading works with several IMAP accounts (self-hosted, web.de, Gmail with
> app passwords, Microsoft 365 with sign-in via Microsoft): local cache, search, read and flagged
> state, safe HTML, attachments. Sending comes later. See the
> [project brief](docs/project-brief.md) for scope, architecture and roadmap.

Built with [Tauri 2](https://tauri.app), React, TypeScript, Tailwind CSS and Rust.

## Privacy

The app talks to your mail servers and nothing else. Fonts, icons and styles are bundled, there
are no CDNs, no analytics and no telemetry, and the content security policy only allows the
app's own files.

## Development

### Requirements

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

### Run

```bash
pnpm install
pnpm tauri dev    # desktop window with hot reload
pnpm dev          # frontend only, in the browser at http://localhost:1420
```

`pnpm dev` runs the interface in a normal browser with sample data. That is handy for working on
layout and styling; everything that needs the backend is checked in the Tauri window.

### Checks

```bash
pnpm check        # typecheck, ESLint, Prettier, rustfmt, clippy, tests
pnpm format       # Prettier and rustfmt
```

## Build installers

```bash
pnpm tauri build
```

The bundles end up in `src-tauri/target/release/bundle/`:

- Linux: `deb/*.deb`, `appimage/*.AppImage` (and `rpm/*.rpm`)
- Windows: `nsis/*-setup.exe`

Releases are not code-signed yet, so Windows SmartScreen shows a warning on first start
("More info" → "Run anyway").

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
  locales/           UI strings (German first)
docs/                project brief: scope, architecture, decisions, roadmap
```

## License

[GPL-3.0-or-later](LICENSE)

The Elevant logo mark is a trademark of Elevant and is not covered by the code license.
