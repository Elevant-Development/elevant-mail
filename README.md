# Elevant Mail

[![CI](https://github.com/Elevant-Development/elevant-mail/actions/workflows/ci.yml/badge.svg)](https://github.com/Elevant-Development/elevant-mail/actions/workflows/ci.yml)
[![License: GPL-3.0-or-later](https://img.shields.io/badge/license-GPL--3.0--or--later-blue)](LICENSE)

A fast, private mail client for Linux and Windows. All your IMAP accounts in one window, with a
unified inbox and a calm interface.

- **Free and open source.** No paywall, no subscription, no account with us.
- **Private.** No cloud, no telemetry. The app talks to your mail servers and nothing else.
- **Local first.** Your mail is cached on your computer, opens instantly and stays readable
  offline.

![Elevant Mail with two accounts in the unified inbox](docs/screenshots/main.jpg)

> **Status: v0.2, reading only.** Elevant Mail reads, searches and organizes mail from several
> accounts. Writing and sending mail comes in a later version (see [Roadmap](#roadmap)). It is
> early software: please keep another mail client around for now, and
> [tell us](https://github.com/Elevant-Development/elevant-mail/issues) what breaks.

## Features

- **Several accounts** with a unified inbox, a color per account and a collapsible folder tree
- **Works with** self-hosted mail servers, Gmail, Microsoft 365, Outlook.com, web.de, GMX and any
  other IMAP provider. Server settings are suggested from your address.
- **Sign in with Microsoft** for Microsoft 365 and Outlook.com, no password stored
- **Fast search** across all accounts and folders, right on your computer
- **Safe HTML mail:** scripts never run, external images stay blocked until you allow them, links
  open in your browser and the app warns when a link leads somewhere other than it shows
- **Attachments:** open or save them. Files that could run programs are only saved, never opened
  directly.
- **Read and starred** state syncs back to the server
- **Keyboard control** for the whole mail view (see [Keyboard shortcuts](#keyboard-shortcuts))
- **Dark and light mode,** six accent colors, two densities, several bundled fonts
- **Readable newsletters in dark mode:** designed mail sits on a light page, simple mail follows
  the dark theme, and you can switch per mail
- **English and German.** The app follows your system language; more languages are welcome.

| Designed newsletter with images allowed        | Light mode with attachments               |
| ---------------------------------------------- | ----------------------------------------- |
| ![Newsletter](docs/screenshots/newsletter.jpg) | ![Light mode](docs/screenshots/light.jpg) |

## Download and install

Get the latest version from the
[releases page](https://github.com/Elevant-Development/elevant-mail/releases/latest).

### Linux

**Ubuntu, Debian, Linux Mint and other Debian-based systems:** download the `.deb` file and
install it:

```bash
sudo apt install ./Elevant.Mail_*_amd64.deb
```

Elevant Mail then appears in your app menu. To remove it later: `sudo apt remove elevant-mail`.

**Other distributions:** use the `.AppImage`. Make it executable and start it:

```bash
chmod +x Elevant.Mail_*_amd64.AppImage
./Elevant.Mail_*_amd64.AppImage
```

If the AppImage does not start, your system lacks FUSE: install it with your package manager
(`fuse3` on most distributions).

Passwords are stored in the system keyring through the Secret Service. GNOME provides it out of
the box; on KDE, KWallet does it once its Secret Service interface is enabled.

### Windows

Windows 10 and 11, 64-bit. Download the `…_x64-setup.exe` file and run it.

The installer is not code-signed yet, so Windows SmartScreen may warn that it does not know the
app. Click **More info**, then **Run anyway**. Passwords go into the Windows Credential Manager.

### Updates

Elevant Mail does not check for updates by itself, because it does not contact anything but your
mail servers. Install a new version over the old one; your accounts and mail stay. To hear about
new versions, watch the repository on GitHub (**Watch → Custom → Releases**).

## Getting started

Click **Add account**, enter your email address and password, and Elevant Mail suggests the
server settings. It signs in once to check them before saving the account. For some providers
there is one extra step:

| Provider                    | What you need                                                                                                                                                                                    |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Gmail                       | An **app password** instead of your normal password: Google Account → Security → App passwords (needs 2-Step Verification). Gmail labels appear as folders.                                      |
| Microsoft 365               | Click **Sign in with Microsoft.** Your browser opens the Microsoft sign-in; no password is entered in the app. Some organizations require an admin to allow Elevant Mail once (see [FAQ](#faq)). |
| Outlook.com, Hotmail        | **Sign in with Microsoft**, as above.                                                                                                                                                            |
| web.de, GMX                 | Switch on IMAP in the web mail settings first: Settings → POP3/IMAP access.                                                                                                                      |
| iCloud                      | An **app-specific password** from account.apple.com.                                                                                                                                             |
| Yahoo                       | An **app password** from your account's security settings.                                                                                                                                       |
| Your own server (Mailcow …) | Usually nothing: the settings come from your domain's mail server. Check them if the app says they were guessed.                                                                                 |

The app shows these hints in the dialog as soon as it recognizes your provider, and every error
comes with a concrete next step.

The interface language follows your system. To change it, open **Appearance** (the palette icon
at the top right) and pick a language; the same menu has mode, colors, density and font.

## Keyboard shortcuts

| Key        | Action                                                          |
| ---------- | --------------------------------------------------------------- |
| `j` or `↓` | Next message                                                    |
| `k` or `↑` | Previous message                                                |
| `Enter`    | Open the first message, or move into the open message           |
| `Esc`      | Go back one level: message → list → no selection                |
| `u`        | Mark as read or unread                                          |
| `s`        | Star or unstar                                                  |
| `/`        | Search. `Enter` or `↓` jumps to the first result, `Esc` clears. |

Inside an open message the arrow keys scroll. Shortcuts pause while you type in a field or a
dialog is open.

## Privacy and security

- **No telemetry, no analytics, no cloud.** Elevant Mail has no server of its own. It connects to
  your mail servers, to Microsoft when you sign in with Microsoft, and to image hosts only when you
  load a mail's images.
- **Nothing from the web.** Fonts, icons and styles are part of the app; no CDNs, no avatar
  services. The build is checked for this on every change.
- **Passwords and sign-in tokens** live in your system keyring, never in files or logs.
- **Encrypted connections only,** with certificates checked against your system's trust store.
- **Mail is untrusted.** HTML is cleaned before display and shown in a sandbox where scripts
  never run. External images are blocked, so senders cannot see when you open a mail. Links open
  in your browser, with a warning when the shown address differs from the real one.
- **Your data on disk:** the mail cache is a SQLite database in `~/.local/share/dev.elevant.mail/`
  on Linux and `%APPDATA%\dev.elevant.mail\` on Windows. Removing an account in the app deletes
  its mail from that cache and its password from the keyring; the mail on the server stays.

Found a security problem? Please report it privately, see [SECURITY.md](SECURITY.md).

## Roadmap

**Done**

- v0.1: reading with one account, safe HTML, attachments, search, keyboard control
- v0.2: several accounts, unified inbox, folder tree, sign in with Microsoft, English and German

**Next: comfort**

- Archive, delete and move, with undo
- New mail right away (IMAP IDLE) and desktop notifications
- Conversation view (threads)
- Filters (unread, starred, with attachment) and server search beyond the local cache
- Account settings: rename, color, change password, sign in again

**Then**

- **Sending:** new mail, reply, forward, drafts, signatures, attachments
- **MCP server for AI agents:** optional and off by default, local only, read-only to start, so
  tools like Claude Code can search your mail without their own access to your accounts
- **Optional AI help:** summaries and reply drafts with a provider you choose, including local
  models; nothing active without setup
- **Calendar:** later, as its own area, after sending works

The [project brief](docs/project-brief.md) has the details, the architecture and the decisions
behind them.

## FAQ

**My organization says Elevant Mail needs admin approval.** Some Microsoft 365 organizations only
let admins allow new apps. An admin grants consent once in the Entra admin center under
Enterprise applications → Elevant Mail → Permissions. IMAP also has to be enabled for your
mailbox (Microsoft 365 admin center → Users → Mail → Manage email apps).

**Gmail says my password is wrong.** Gmail does not accept your normal password in mail apps. Use
an app password (see [Getting started](#getting-started)).

**Can I send mail?** Not yet. Sending is the next big step after the comfort features.

**Does it support POP3 or Exchange ActiveSync?** No, IMAP only. Microsoft 365 and Outlook.com work
over IMAP with sign-in via Microsoft.

**Is there a macOS version?** Not planned for now. Tauri runs on macOS, so it is possible later.

**The window stays blank or flickers on Linux.** WebKitGTK has rendering problems on some
graphics setups, especially hybrid Nvidia laptops. Start the app with:

```bash
WEBKIT_DISABLE_DMABUF_RENDERER=1 elevant-mail
```

**The app cannot save my password on Linux.** It needs a running Secret Service (GNOME Keyring,
or KWallet with Secret Service enabled). Unlock the keyring and try again.

## Contributing

Bug reports, translations and code are all welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md) to
get started; [docs/development.md](docs/development.md) explains how to build and run the app from
source.

## License

Elevant Mail is free software under the [GNU General Public License v3.0 or later](LICENSE).

The Elevant logo mark is a trademark of Elevant and is not covered by the code license.
