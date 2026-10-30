# Project brief: Elevant Mail

Status: 2026-10-29 · v0.2 (reading with several accounts)

## Goal

Elevant Mail is a free, open-source desktop mail client for **Linux (Ubuntu/Debian)** and
**Windows**: modern, fast and light on resources, for several IMAP accounts with a unified inbox.
It is not an enterprise client and not a full Thunderbird replacement. Mail comes first; a calendar
may follow later as its own area (see "Later: calendar").

Not negotiable:

- No paywall, no account requirement, no cloud backend, no telemetry
- No calls to third-party services the user did not trigger (no avatar services, no looking up
  server settings in third-party databases)
- No CDNs (GDPR): fonts, icons and styles are bundled, the app's CSP only allows its own content
- Open source

The first milestone is **reading**: fetching, reading, searching and organizing mail. Sending
comes in phase 4.

## Accounts it has to work with

| Account                           | Sign-in                  | IMAP                           | SMTP (phase 4)                 | Notes                                                                                                     |
| --------------------------------- | ------------------------ | ------------------------------ | ------------------------------ | --------------------------------------------------------------------------------------------------------- |
| Self-hosted (Mailcow)             | Password / app password  | 993 TLS                        | 465 TLS / 587 STARTTLS         | Full control over the server                                                                              |
| web.de                            | Password                 | `imap.web.de` 993 TLS          | `smtp.web.de` 587 STARTTLS     | IMAP access has to be switched on in the web.de settings                                                 |
| Gmail                             | App password             | `imap.gmail.com` 993 TLS       | `smtp.gmail.com` 465 / 587     | App passwords need 2-step verification; labels appear as folders                                         |
| Microsoft 365 (Entra work account) | **OAuth2 (XOAUTH2)**    | `outlook.office365.com` 993 TLS | `smtp.office365.com` 587 STARTTLS | Exchange Online accepts no password sign-in over IMAP; SMTP with passwords is off by default from late 2026 |

The app suggests server settings (`crates/core/src/mail/presets.rs`): first presets for known
providers (Gmail, web.de, GMX, Microsoft 365/Outlook.com, iCloud, Yahoo, Posteo, mailbox.org,
T-Online), then the domain's MX record. Hosted mailboxes (Google Workspace, Microsoft 365, IONOS,
Strato …) are recognised by their MX host; on self-hosted servers such as Mailcow the MX host is
usually the IMAP host as well. Otherwise `imap.<domain>` remains as a guess for the user to check.
Only the user's own DNS is asked, never a third-party database. Later: autoconfig on the
provider's own domain (`autoconfig.<domain>`, DNS SRV per RFC 6186).

### Microsoft 365: what OAuth2 needs

- App registration in Elevant's Entra tenant (Elevant publishes the app; organisations that use it
  only grant consent): public client without a secret, multi-tenant, authority `common` (also
  covers personal Outlook.com accounts), redirect `http://localhost` (loopback), PKCE
- Delegated permissions: `https://outlook.office.com/IMAP.AccessAsUser.All`,
  `https://outlook.office.com/SMTP.Send`, `offline_access`, `openid`, `email`
- Flow: open the system browser → redirect to a local port → fetch tokens → refresh token into
  the system keyring
- The client ID is in the code (usual for public clients). Later it can be overridden in the
  settings, for organisations that require their own registration.
- Without publisher verification Microsoft shows "unverified", and tenants that only allow
  consent to verified publishers need an admin to consent once.
- IMAP has to be enabled for the mailbox in the account's tenant; for phase 4 also SMTP AUTH.

**No Gmail OAuth on purpose.** Google treats the Gmail scope as "restricted" and requires a
security assessment for a public app. As long as app passwords work, that stays so.

## Tech stack

**Frontend**

- Tauri 2, React, TypeScript (`strict`), Vite
- Tailwind CSS v4 and shadcn/ui (Radix based)
- TanStack Query for everything that comes from the backend; Zustand for pure UI state
  (selection, focus, layout)
- `@tanstack/react-virtual` for the message list
- ESLint and Prettier, Vitest

**Backend (Rust)**

- `tokio` as the runtime
- `async-imap` (feature `runtime-tokio`) with `tokio-rustls`, no OpenSSL
- `mail-parser` for MIME
- `ammonia` for sanitizing mail HTML
- `rusqlite` with `bundled` (includes FTS5), own migrations
- `keyring` for the Secret Service (Linux) and the Windows Credential Manager
- `reqwest` with rustls for the OAuth token endpoint and external images
- `lettre` for SMTP (phase 4), with rustls
- `thiserror` for error types, `tracing` for logs
- `uuid` (v7, time-sortable) for IDs
- clippy, rustfmt, `cargo-deny` (licences and known vulnerabilities of dependencies)

No Electron, no cloud backend.

App identifier: `dev.elevant.mail`. Database and attachments live in the app data folder that
Tauri provides per platform.

## Architecture: local-first

Principle: **the IMAP sync writes to SQLite. The interface reads only from SQLite.**

```text
IMAP server ──▶ sync engine (Rust, background) ──▶ SQLite ◀── commands ── React
                        │                                          ▲
                        └──────── events (progress, changes) ──────┘
```

It follows that:

- The app starts without a network and shows the cache.
- Switching folders and the unified inbox are database queries, no waiting for the network.
- Actions (read, flagged, archiving, deleting, moving) take effect locally at once and go into a queue
  (`pending_actions`) that is sent to the server (`crates/core/src/actions.rs`,
  `mail/push.rs`). Per message and flag only the latest wish counts. The queue is sent right after
  the action and before every sync, so changes made offline go out with the next connection; for
  that the app opens the folder with `SELECT` and sends `UID STORE … FLAGS.SILENT`, one command
  per folder and flag. Connection problems leave the queue in place. If the server refuses
  (`NO`/`BAD`), the cache gets the server's flags back and the error shows on the account. The
  sync's flag reconciliation never overwrites a message with a pending change.
- Every account has its own sync with its own connection. A hanging account does not block the
  others.

Sync rules (implemented in `crates/core/src/mail/sync.rs`):

- Check **UIDVALIDITY** per folder. If it changes, the folder's cache is dropped and fetched again.
- What the cache keeps per folder: the newest 100, everything from the account's period
  (`accounts.keep_days`: 1 month, 3 months (default), 1 year or everything, set per account;
  `UID SEARCH SINCE`), and everything that arrived since the last sync whatever its date (mail
  moved there keeps its old one). A longer period is fetched at the next sync, newest first; a
  shorter one keeps what is cached. Older mail comes in batches of 100 via "load older mail" at the
  end of the list.
- Mail a search on the server fetched (`messages.by_search`) stays out of its folder's list until
  the sync reaches it; then it is adopted. "Load older" counts from the oldest synced mail.
- Mail deleted on the server: the server's UID list (`UID SEARCH ALL`) is compared with the cache.
- Read and flagged state from other devices: the FLAGS of cached mail are reconciled. CONDSTORE
  (`highest_modseq`) and QRESYNC will make this cheaper later.
- Fetching never marks mail as read: the folder is opened with `EXAMINE` (read-only) and content
  fetched with `BODY.PEEK`. A test checks that neither `SELECT` nor `STORE` is sent unless a
  change by the user is waiting.
- Messages up to 512 KB are fetched completely. Larger ones (mostly because of attachments) get
  their headers and the first 64 KB for the preview, the rest when opened (`load_message_body`,
  also with `EXAMINE` and `BODY.PEEK`).
- Unread and total counts come from the server (`UID SEARCH UNSEEN`, `EXISTS`); for the other
  folders of an account every sync asks `STATUS (MESSAGES UNSEEN)` without opening them. The
  sidebar shows unread counts only for the inbox, the archive and the user's own folders; for spam,
  trash, sent, drafts and "All Mail" they say nothing.
- Every sync of an account also refreshes its folder list (new, renamed and deleted folders).
- Syncs run at start, for a new account (right after adding it), every 5 minutes and on
  "refresh". Folders other than the inbox sync when opened (at most once a minute).
- New mail arrives at once where the server offers IMAP IDLE (`crates/core/src/mail/idle.rs`,
  `src-tauri/src/watch.rs`): per account one more connection waits read-only in the inbox,
  renews IDLE every 25 minutes and reconnects after errors with growing pauses (30 seconds to
  10 minutes). A change there makes the frontend sync the account (`inbox-changed`). The
  connection starts with the app and a new account, restarts after a new login and stops when the
  account is removed. Servers without IDLE stay with the 5-minute poll.
- Conversations (`crates/core/src/threads.rs`): each message gets a `thread_id` when it is stored,
  oldest first. A reply finds its conversation by the Message-IDs it names (In-Reply-To,
  References), which `thread_keys` keeps for every conversation, so it also works when the reply
  arrives before the mail it answers; a message naming two conversations joins them. A reply
  without these headers ("Re:", "AW:" and the like) joins the latest conversation with the same
  subject from the 60 days before it. Conversations belong to one account and span its folders.
  The cache from before was placed once at the first start after the update.
- Only one operation per account runs at a time; further ones wait. Accounts do not block each
  other, and an error shows only on its own account.
- Progress goes to the interface as the event `sync-progress` ("connecting…", "checking for new
  mail…", "loading mail… 40 of 100").

## Security

### Mail HTML

Mail HTML **never goes into the app's DOM**. Two layers:

1. **Rust sanitizes** with `ammonia` (`crates/core/src/mail/sanitize.rs`): no scripts, forms,
   objects, frames, event attributes, `<meta>`, `<base>`, `<link>`, no `javascript:` or `data:`
   links. Links only `http`, `https` and `mailto`; images only `cid:`, `data:image/…` and external
   addresses (see below). The cache keeps the original HTML and sanitizes on every open (1 to
   3 ms), so a stricter sanitizer also covers mail that is already cached.
2. **Display in `<iframe sandbox srcdoc>`** with `allow-same-origin allow-popups`, without
   `allow-scripts`, `allow-forms` and `allow-top-navigation`.

In addition:

- Without `allow-scripts` nothing runs in the frame, even with `allow-same-origin`; only the
  combination of both is dangerous. The app needs `allow-same-origin` to read the frame (height,
  the link under the mouse pointer, link text for the phishing check).
- The frame document gets its own CSP: `default-src 'none'; style-src 'unsafe-inline'; img-src
  mail-asset: http://mail-asset.localhost data:; font-src 'self'` (the app's bundled fonts). A
  mail's style sheets stay but load nothing.
- A `srcdoc` iframe inherits the app's CSP. That is why "load images" **never** opens the app's
  CSP for `https:`. Instead Rust fetches a mail's images on click and serves them through its own
  Tauri protocol (`mail-asset://`, on Windows `http://mail-asset.localhost`). The same protocol
  serves inline images (`cid:`) from the cache; they are stored with the body, up to 1 MB each.
- External images are blocked by default. They are allowed per mail for the running session, or
  always for a sender ("Always from …", table `image_senders`; the backend checks the sender when
  a mail is opened). The list is under settings → Reading, where addresses can be removed. Fetching runs without cookies or referrer, follows at most 5
  redirects, accepts only `image/*` up to 10 MB and refuses `localhost`, private and carrier-grade
  NAT addresses, so a mail cannot send the app into the local network. CSS background images stay
  blocked even when images are allowed.
- Links open in the **system browser**, never in the app. How: the frame sets
  `<base target="_blank">`, so every click becomes a new-window request. Tauri refuses it
  (`on_new_window`) and passes the address to the interface (`mail-link`), which hands it to
  `open_link`. Reason: WebKitGTK does not run the app's listeners in a frame without scripts
  (checked with WebKitGTK 2.52); Chromium (WebView2 on Windows) does, so there the click listener
  handles it directly. On top of that the main window allows no navigation away from the app.
- The real target is visible before opening: under the mouse pointer it shows at the bottom left,
  host emphasized. If the link text shows a different address than the target (phishing), the app
  asks first.
- Limitation: while the focus is in the frame (after a click into the mail text), keyboard
  shortcuts do not reach the app under WebKitGTK until the user clicks outside.

### App

- Keep Tauri 2 capabilities tight: the frontend may only call the app's own commands. No fs,
  shell or http plugins for the frontend.
- Strict CSP for the app itself.
- Passwords and tokens only in the system keyring, never in SQLite, JSON or logs. If no keyring
  is available, saving stops with a clear message instead of falling back to a file.
- Protocol logs redact `LOGIN` and `AUTHENTICATE`.
- Error messages contain neither credentials nor mail content.
- TLS certificate checks can never be switched off, neither globally nor per account.
- Opening attachments: they are saved to the app's cache folder (`attachments/`, readable only by
  the user, emptied at start) and handed to the system's program. Executable files (`.exe`,
  `.bat`, `.cmd`, `.ps1`, `.sh`, `.desktop`, `.AppImage`, Office files with macros …) are only
  saved, never opened; checked on file extension and declared type, in the backend and in the
  interface (`crates/core/src/attachments.rs`).
- File names from mail are cleaned before writing: no folders (`../`), no control characters or
  characters Windows forbids, no reserved names like `CON`, no leading dot.

## Data model

The schema in `crates/core/src/db/migrations/` is authoritative. Changes come as further
migrations; a shipped migration is never changed. The schema version is SQLite's `user_version`.
The app does not open a database from a newer app version, so an older version never changes data
it does not understand.

| Table             | Content                                                                                                                                                   |
| ----------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `accounts`        | Accounts with IMAP and SMTP settings (SMTP NULL until changed: then the one that goes with the IMAP host), sign-in method, colour, `sender_name`, `keep_days` (how far back the cache keeps mail), `in_unified_inbox`. No passwords. |
| `folders`         | Folders per account with SPECIAL-USE role, `uid_validity`, `uid_next`, `highest_modseq`, counts                                                           |
| `messages`        | Headers, flags, preview, content (text and original HTML, sanitized when opened), `in_reply_to`, `references_header` and `reply_to`, `thread_id` and `thread_subject` (subject without "Re:") for conversations, `leaving` while a move is on its way, `by_search` for mail only a server search fetched; unique per folder and UID |
| `thread_keys`     | Every Message-ID a conversation has seen, per account, so replies find it                                                                                  |
| `image_senders`   | Senders (address in lower case) whose mail shows its external images when opened                                                                           |
| `attachments`     | Attachments per message with the MIME part to fetch them; small inline images with their bytes                                                           |
| `pending_actions` | Local changes that still have to reach the server; `due_at` holds a move back while it can be undone                                                       |
| `messages_fts`    | Full-text index (FTS5) over subject, sender and text                                                                                                      |

Rules:

- IDs are UUIDv7, timestamps Unix milliseconds.
- `messages` also has a fixed row number `seq` (INTEGER PRIMARY KEY) that the full-text index
  refers to. Without it `VACUUM` could renumber rows.
- The full-text index does not store the text twice (external content). Triggers keep it current
  on insert, update and delete; flag changes do not touch it. `remove_diacritics` makes "muller"
  find "Müller".
- Foreign keys cascade: removing an account removes its folders, messages, attachments, queue and
  index entries.
- SQLite runs in WAL mode with `foreign_keys = ON`.
- File: `elevant-mail.sqlite3` in the app data folder (Linux `~/.local/share/dev.elevant.mail/`,
  Windows `%APPDATA%\dev.elevant.mail\`). Attachments are not in the database (except small
  inline images up to 1 MB): the cache only knows the list, the app fetches a file from the
  server when it is opened or saved and keeps it in the cache folder for the running session. On
  Linux folder and database are readable only by the user (`0700`/`0600`); on Windows the user
  profile protects them.
- Credentials live in the system keyring under the service `dev.elevant.mail`, one entry per
  account and kind (`<account-id>:imap-password`, `:smtp-password`, `:oauth-refresh-token`).
- Logging through `tracing`, level via `ELEVANT_MAIL_LOG` (e.g. `debug`). Protocol lines go
  through `redact`, which blanks `LOGIN` and `AUTHENTICATE`.

## Tauri commands and events

The frontend only calls these commands; there is no direct IMAP access from JavaScript. All
calls are small wrappers in `src/lib/tauri.ts`. `ts-rs` generates the data types from the Rust
types: `cargo test` writes them to `src/lib/bindings/` (checked in, never edited by hand); CI
checks that they are current.

| Area        | Commands                                                                                                                                 |
| ----------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| App         | `app_status` (version, schema version, keyring available)                                                                                 |
| Accounts    | `detect_settings` (presets, MX record), `add_account` (checks the login before saving), `add_microsoft_account`, `cancel_sign_in`, `list_accounts`, `get_account_settings`, `update_account` (name, colour), `set_keep_days`, `set_in_unified_inbox`, `set_sender_name`, `change_login` (server settings, password; checks the login first), `change_smtp` (outgoing server, its own password; checks the login first), `sign_in_again` (Microsoft), `remove_account` |
| Folders     | `list_folders`                                                                                                                            |
| Mail        | `list_messages` (a page of a folder or the unified inbox; with `threads` one row per conversation, `filters` from the filter bar, `after` for the next page), `get_conversation` (what the reader shows for a row), `get_message` (with `remote_images` to allow a mail's external images; they also show for senders in `image_senders`), `load_message_body`, `search_messages`, `list_image_senders`, `allow_images_from`, `forbid_images_from` |
| Flags       | `set_read`, `set_flagged`, `push_actions`                                                                                                 |
| Moving      | `move_messages`, `archive_messages` (makes an archive folder where there is none), `delete_messages` (to the trash, from there for good), `undo_moves` |
| Writing     | `send_message` (returns once the SMTP server took it; filing in Sent follows, then the `mail-sent` event), `suggest_addresses` |
| Links       | `open_link`                                                                                                                               |
| Attachments | `save_attachment` (the system's save dialog via `tauri-plugin-dialog`, called from Rust only; the frontend gets no dialog or file permissions), `open_attachment` |
| Sync        | `sync_account`, `sync_folder`, `load_older`, `search_server` (one account's folders on the server)                                        |

Planned: `test_connection` for existing accounts.

`set_read` and `set_flagged` set a target value instead of toggling, which stays correct on
double clicks and repeats. They only change the cache and return the affected accounts;
`push_actions` then sends the queue in the background.

`search_messages` searches the full-text index over subject, sender and text of all cached mail,
newest first, at most 300 results. Every word has to match, as a word or the start of one ("rech"
finds "Rechnung"); accents and umlauts do not matter ("koln" finds "Köln"). The input is quoted
word by word, so FTS5 syntax such as `OR` or `-` is plain text for the user. A message that sits
in several folders of an account (Gmail labels, "All Mail", copies) is listed once, preferably
from the inbox. Large mail is searchable by its text only after it was opened once.

Filters in the search field (`crates/core/src/query.rs`), in English and German: `from:`/`von:`
(name or address, as a phrase in the sender columns), `is:unread`/`ist:ungelesen`,
`is:read`/`ist:gelesen`, `is:flagged`/`ist:markiert`, `has:attachment`/`hat:anhang`. They work
alone too ("ist:ungelesen" lists all unread mail). Unknown ones ("Re:", "10:30") are words. A
button in the reader shows all mail from a sender (`von:address`). The filter bar over the list
(unread, flagged, attachment) narrows whatever the list shows; a selected row stays while it is
selected, and the bar resets when another mailbox opens.

`search_server` searches on request ("search on the server too", below the results): per account
all folders except trash, spam and drafts, on Gmail only "All Mail". The same query goes out as
IMAP `SEARCH` (`TEXT`, `FROM`, `UNSEEN`, `FLAGGED`; text that is not ASCII as a LITERAL+ literal
with `CHARSET UTF-8`). The newest 50 matches per folder are fetched like the sync does it and
returned; `search_messages` lists them too (`also`), so matches deep in large mail show as well.

With conversations on, a row of `list_messages` and `search_messages` is a conversation within
one folder, shown with its newest message: unread while one of its messages is, flagged while one
is, with everyone who wrote and the number of messages, the user's own replies from "Sent"
included. Its `messageIds` (the conversation's messages in that folder) are what marking,
archiving, deleting and moving act on; replies in "Sent" stay there. A search hit in "Sent" shows
as the conversation in the folder where the rest of it is. `get_conversation` returns those
messages plus the replies from "Sent", newest first.

`list_messages` returns a page of 300 rows and where the next one starts (`next`: date, UID and
id of the last row's newest message); the list loads it while scrolling towards the end. Each
folder is read along an index in list order (`idx_messages_folder_list`), the unified inbox
merges its inboxes; a row is a conversation's newest message in the folder, which brings along
the rest of it, however old. The filter bar's filters (`unread`, `flagged`, `attachment`) run in
the database and match a conversation while one of its messages does; the selected rows come
along as `keep`, so an unread mail does not vanish while it is open. Measured with 50,000
messages in one folder: about 5 ms a page with conversations, under 1 ms without.

Events from the backend:

- `sync-progress`: account, folder, stage, progress
- `mail-link`: the address of a link clicked in a mail frame

Errors are types with a kind and a suggested next step, not free text. Examples: login refused at
Gmail → "use an app password"; at web.de → "switch IMAP on in the web.de settings"; connection
refused → "check port and encryption".

## Project structure

```text
Cargo.toml                 # workspace
crates/core/src/           # no Tauri: used by the desktop app, the planned MCP server and tests
  accounts.rs  actions.rs  attachments.rs  mailbox.rs  model.rs  oauth.rs
  error.rs  ids.rs  redact.rs  secrets.rs
  db/        mod.rs  migrations.rs  migrations/*.sql
  mail/      imap.rs  sync.rs  push.rs  download.rs  parse.rs  sanitize.rs  remote.rs
             folders.rs  presets.rs  settings.rs  test_server.rs (tests only)
src-tauri/src/             # window, start-up and commands only
  main.rs  lib.rs  state.rs  error.rs  window.rs
  protocol.rs              # mail-asset:// for images
  commands/  app.rs  accounts.rs  mailbox.rs  actions.rs  sync.rs  links.rs  attachments.rs

src/
  components/
    layout/    AppShell, Sidebar, TitleBar, AppearanceMenu, AppearanceSettings, WindowControls
    mail/      MessageList, MessageListItem, MessageReader, SafeHtmlMail, AttachmentList, …
    accounts/  AccountDialog, AccountList, ServerFields, RemoveAccountDialog, WelcomeScreen
    settings/  SettingsDialog, AccountSettingsPanel
    common/    Icon, IconButton, SearchInput, Notice, Badge, Avatar, …
    ui/        # shadcn/ui
  design/      themes and fonts
  hooks/       sync, keyboard shortcuts, message actions, …
  queries/     accounts, folders, messages   # TanStack Query
  stores/      ui, sync                      # Zustand
  lib/         tauri.ts  mock.ts  folders.ts  links.ts  keyboard.ts  formatting.ts  i18n.ts
  locales/     de.ts  en.ts
```

`lib/mock.ts` provides sample data when the frontend runs in a browser without Tauri. That is for
design iteration and quick visual checks. An interface counts as done only once it has been
clicked through in the running Tauri app.

## Design

Elevant Mail should feel modern and calm, not like Thunderbird, Outlook 2010 or a Windows 95 tool.

- Dark mode by default, light mode optional
- Three columns: accounts/folders on the left, message list in the middle, reader on the right
- Narrow windows: list and reader as two views with navigation
- Compact rows, clear typography, subdued colours
- Unread mail clearly highlighted. An opened message counts as read after one second, so moving
  through the list with `j`/`k` does not mark everything; once per opening, so `u` keeps it unread
  afterwards. Settings → Reading: at once, after 1 s (default), after 3 s, or never by itself.
- Avatars from initials, no external avatar services
- Keyboard: `j`/`k` move (arrow keys too in the list; in the reader they scroll), `Enter` opens
  the first message or moves into the reader, `Esc` goes back one level (reader → list → no
  selection), `u` read/unread, `s` star, `/` search (in the search field `Enter` or `↓` goes to the
  first result, `Esc` clears, a second `Esc` leaves the field; in the list `Esc` without a
  selection clears the search). With `Shift` the moving keys widen the selection, `Ctrl+A` selects
  the list; on the selection `e` archives, `#` and `Del` delete, `v` moves (folder picker), and
  `z` takes the last archiving, deleting or moving back. Shortcuts pause while a dialog or menu
  is open or while typing in a field. `Home`/`End` in the list go to its first and last loaded
  message; `Ctrl+A` selects what is loaded.
- Focus: Tab reaches each part once: search field, folder tree (at the open folder), list (at the
  open message), reader. In the folder tree the arrow keys move, `→` opens a folder or account
  and `←` closes it or goes to its parent; `F6`/`Shift+F6` jump between the parts. A dialog gives
  the focus back where it was, or to the next mail when the one it came from was moved.
- Several messages: `Ctrl`-click adds or removes one, `Shift`-click selects a range. The reader
  then shows what can be done with all of them.
- No animations that slow things down; only small, meaningful transitions
- Performance targets: the list stays smooth with 50,000 messages in a folder, switching folders
  from the cache shows no waiting, cold start to a visible list under one second.

Decisions:

- **"Floating panels":** list and reader are rounded panels on a darker canvas, the sidebar sits
  directly on the canvas. Messages appear as cards, avatars as rounded squares with a dot for
  unread.
- **Appearance from six settings**, all design tokens derive from them (`src/design/themes.ts`);
  the layout stays the same. Chosen via "appearance" in the title bar.
  - Mode: dark, light or system
  - Style: **neutral** (default) with black, grey and white surfaces and colour only as an accent,
    or **tinted** with tinted surfaces and selection in the accent colour
  - Contrast: soft (layered, default), clear (dark grey), hard (near black or clear white)
  - Accent: teal (the Elevant teal, default), coral, saffron, sky, raspberry, mono
  - Density: comfortable (default) or compact (smaller avatars, no preview line)
  - Font: Wix Madefor Text (default), Inter, Onest, Figtree, Mona Sans or Geist, used throughout.
    All fonts are bundled; nothing loads from the network.
- **Icons:** Lucide, all through `src/components/common/Icon.tsx`.
- **Brand:** the Elevant mark sits in the title bar, aligned with the sidebar icons; the search
  field is aligned with the message list. App icon: the mark in white on teal.
- **Title bar:** a custom one instead of the system's. The window is frameless, the title bar is
  the drag area (double click maximizes), with own buttons to minimize, maximize and close.
- **Account colour:** a narrow stripe on the card's left edge in the unified inbox.
- **Folder tree:** folders as a tree by their path; accounts and folders collapse (remembered).
  Special folders always sit on top, even where the server nests them (Gmail's `[Gmail]/…`);
  groups without mail of their own start collapsed. A role belongs to one folder only, SPECIAL-USE
  wins over a guess from the name; where another folder carries the translated name (web.de: "Sent"
  as the sent folder plus an old "Gesendet"), the special folder shows its server name too.
- **Conversations:** like Gmail, Apple Mail, Outlook and Fastmail, on by default, switchable in
  the appearance ("List: conversations / single messages"). A row names everyone who wrote by
  first name ("Lena, me, Jonas") and the number of messages. The reader shows the newest message
  on top; the newest and the unread ones are open, older ones folded to a line (sender, start,
  date) and open with a click. The user's own replies from "Sent" are part of it, marked "Sent".
  Opening marks the whole conversation read; the star goes on its newest message.
- **Writing:** in the reader column, like a reply in Gmail or Mimestream, with the list beside it.
  "New mail" sits on top of the sidebar (`c`); the reader has reply, reply all and forward
  (`r`, `a`, `f`). Opening another mail puts the draft aside; its conversation then offers
  "Continue writing". Recipients as chips with suggestions from the cache, Cc/Bcc on request, a
  plain toolbar (bold, italic, underline, lists, quote, link), `Ctrl+Enter` sends.
- **HTML mail in dark mode:** hybrid. Simple mail without its own design follows dark mode (the
  app's colours and reading font); designed mail (own background or text colours) sits on a light
  page. Switchable per mail: simple mail onto the light page, designed mail into a dark view with
  inverted colours where images keep theirs. Wide newsletters shrink to the column width (down to
  60 %).

## UX rules

- First start: welcome screen with "add account"
- Adding an account: enter the address, server settings are suggested, provider tips (app
  password, switch on IMAP) show once the address is entered, login check before saving
- After adding: progress with a clear status
- Errors never only as a red toast, but with a concrete next step ("check port", "use an app
  password")
- Click on a message: immediate selection, content loads asynchronously
- Switching folders: at once from the cache, the sync runs in the background
- No zoo of modals
- The app starts without internet and shows the cache

## Scope

### Phase 1: reading with one account (v0.1) — done

Account with suggested settings and login check, inbox sync (newest 100, older on demand),
virtualized list, reader with safely rendered text and HTML (images on click, links in the
system browser), attachments (view, save, open), read/unread and star (local at once, then to the
server), search in the cache (FTS5), start without network, dark interface, keyboard control.

### Phase 2: several accounts (v0.2) — done

Several accounts with a folder sidebar (roles via SPECIAL-USE), unified inbox, a colour per
account, errors isolated per account, folders from the cache, **OAuth2 for Microsoft 365**.
Gmail's "All Mail" is not synced in the background, and search lists a message once even when it
is in several folders. Gmail app passwords are used without the spaces Google shows them with.

Still open from these phases: publisher verification for Elevant so users of other organisations
can consent without an admin. Paging beyond 5,000 cached messages per folder came in step 18.

### Phase 3: comfort

- Settings window (gear in the title bar): per account name, colour (eight to choose from), server
  settings and password, which only change after a login with them worked, and "sign in again"
  for Microsoft accounts (same address only); removing; the appearance. Sync errors that the
  account settings fix get a button to them. — done
- IMAP IDLE for the inbox, polling for other folders — done
- Desktop notifications for new mail (`tauri-plugin-notification`) — done. Only for mail that
  arrived since the last sync and is unread, only while the window is not in front; one message
  by sender and subject, several by count and senders. Settings: on/off, and without sender and
  subject (only the account). Keeping the app running in the tray after the window is closed
  comes later.
- Archive, delete, move, also for several messages at once — done. Local at once, the server
  after a 10-second undo window (`crates/core/src/moves.rs`): `UID MOVE`, without MOVE `UID COPY`
  plus `\Deleted` and `UID EXPUNGE` (plain `EXPUNGE` without UIDPLUS). Archiving goes to the
  archive folder, Gmail's "All Mail", or a folder "Archiv" made on the server; deleting goes to
  the trash and asks before deleting from there for good. A refused move shows the mail again.
- Server-side search (IMAP `SEARCH`) for mail outside the cache, on request — done
- Filters: unread, flagged, sender, with attachment — done (filter bar and search syntax)
- How far back the cache keeps mail, per account (1 month to everything, default 3 months) — done
- Keyboard focus management — done. Tab reaches each part once (search, folders, list, reader),
  the arrow keys move within; F6 jumps between the parts; dialogs give the focus back
- Settings → Reading — done: which accounts the unified inbox brings together (all by default;
  with some left out it is called "Inboxes"), which inbox opens at start (the unified one or one
  account's; from phase 4 also the default sender), the mark-as-read delay, the senders whose
  images always load. Theme and density are in the appearance.
- Conversations, switchable, on by default — done. Grouped by Message-ID headers, replies
  without them by subject; per folder, with the user's own replies from "Sent".

### Phase 4: sending

- SMTP per account, with password and XOAUTH2 — done (`lettre`, TLS like IMAP, the client greets
  as `[127.0.0.1]`). The outgoing server is suggested from the incoming one (known providers,
  otherwise `smtp.` for `imap.` or the same host, port 465); a separate SMTP password is possible.
- New message, reply, reply all, forward — done. Written in the reader column with a formatted
  editor (TipTap, bundled); sent as HTML plus a plain-text version made from the editor's
  document. Replies get In-Reply-To and References, follow Reply-To and quote the text below an
  empty line; Bcc goes to the server but only into the copy for Sent.
- Sent mail in Sent where the provider does not do it — done: a few seconds after sending the app
  looks for the Message-ID in Sent and only then files the copy (`APPEND`, read).
- Drafts kept while the app runs — done; saved locally and in the server's drafts folder: next
- Text and HTML signatures
- Sending attachments

### Phase 5: MCP server for AI agents (planned)

Elevant Mail offers the local mail store over the Model Context Protocol, so AI agents (Claude
Code and other MCP clients) can search and read mail without their own IMAP access.

- Local only: as a stdio process or on `127.0.0.1`, never reachable from the network
- Off by default. The user switches it on and decides per account what agents may do.
- Reading first: list accounts and folders, search mail (FTS5), read mail, list attachments
- Writing tools later, each enabled separately: set read/flagged, move, create a draft
- Sending stays with the user; an agent can at most prepare a draft.
- Mail content is untrusted data. The server marks it so agents do not treat instructions in it
  as orders (prompt injection).
- Every access goes into a local log that can be viewed in the app
- No credentials over MCP: the server reads from the cache and needs no password

So this works without rework later, `db/` and `mail/` stay free of Tauri. A separate program
(`elevant-mail-mcp`) can use the same code and database; SQLite runs in WAL mode so app and MCP
server can read at the same time.

### Phase 6: AI features (planned)

Optional AI help in the app, with a freely chosen provider. Nothing is active without setup.

- **Providers:** OpenRouter with the user's own API key (in the system keyring); local models
  through an OpenAI-compatible server (Ollama, LM Studio, llama.cpp server); optionally a small
  built-in model that runs on any laptop's CPU
- **Built-in model:** not in the installer but downloadable on click (one to two GB). Runs in
  Rust, candidates `llama-cpp-2` (llama.cpp) or `candle`/`mistral.rs`; the model is chosen when
  this is implemented.
- **Possible features:** summarize a message or thread, suggest a reply draft (sending stays with
  the user), extract dates and tasks, important first, semantic search via local embeddings
- **Privacy:** local by default; a cloud provider is clearly marked as such; before the first
  request to a cloud provider the app says what goes where; no telemetry, no Elevant server in
  between
- **Mail content is untrusted:** instructions in a mail must not steer the AI (prompt
  injection); actions only after the user confirms.

### Later: calendar

A calendar as its own area next to mail, after sending works.

- Earlier, in phase 3 or 4: show invitations (`.ics`) in the reader; accepting and declining needs
  sending (phase 4)
- Sources: CalDAV (self-hosted, for example Mailcow with SOGo), Microsoft 365 and Outlook.com via
  Microsoft Graph (`Calendars.ReadWrite`), Google via OAuth. Google's calendar scope is
  "sensitive" and needs Google's app verification; unlike Gmail there is no app password route.
- Local first like mail: events are cached in SQLite, the interface reads only from the cache

## Tests

- Rust unit tests for parser, sanitizer, sync logic (UIDVALIDITY change, deleted mail, flag
  reconciliation), the send queue and OAuth, against an in-process IMAP test server
  (`crates/core/src/mail/test_server.rs`) and a local stand-in for the token endpoint
- Malicious HTML (scripts, event attributes, CSS tracking, `<meta refresh>`, forms, `javascript:`
  links) has to be defused by the sanitizer
- Vitest for frontend logic (formatting, keyboard, links, folder tree)
- Later: integration tests against a real IMAP server in Docker (Dovecot or GreenMail)
- Before every release a smoke test with real accounts on Linux and Windows

## Build and distribution

- CI on every push and pull request (`.github/workflows/ci.yml`): lint, format, typecheck, tests
  and build on Linux and Windows, a check that generated bindings are current, a check that the
  build loads no external addresses (GDPR), and `cargo-deny`
- Release on tags like `v0.1.0` (`.github/workflows/release.yml`, `tauri-action`), artefacts to a
  draft GitHub release:
  - Linux on the oldest available Ubuntu LTS runner (older glibc, so the AppImage also runs on
    older systems): `.deb` and `.AppImage`
  - Windows: NSIS `.exe`
- No code signing in 0.x; Windows SmartScreen warns, the README explains it.
- No auto-update in 0.x

### Platform quirks

- Tauri uses WebKitGTK on Linux and WebView2 (Chromium) on Windows. Rendering and speed differ;
  test both regularly.
- On machines with hybrid GPUs WebKitGTK has known rendering issues. Workaround:
  `WEBKIT_DISABLE_DMABUF_RENDERER=1`.
- The keyring needs a running Secret Service on Linux (GNOME Keyring, or KWallet with the Secret
  Service interface on KDE).
- `docs/development.md` lists the system packages for Ubuntu and the requirements on Windows.

## Open source

- Licence GPL-3.0-or-later: forks stay free, which fits "no paywall". The Elevant mark is a
  trademark and not covered by the code licence.
- Code, comments, commits, README and docs in English. The interface is in English and German,
  more languages can follow: all texts go through i18n keys (`src/locales/`, German defines the
  keys). The language follows the system (English where there is no translation) and can be
  changed under Appearance.
- Public GitHub repository `Elevant-Development/elevant-mail` with `LICENSE`, `README.md` for
  users, `CONTRIBUTING.md`, `SECURITY.md` and `docs/development.md` for building from source
- No credentials, real account addresses or server names in the repository, tests or fixtures.
- The OAuth client ID may be public (public client without a secret).

## Roadmap

1. **Scaffold:** Tauri 2, React, TypeScript, Vite, Tailwind v4, shadcn/ui, dark mode, three-column
   layout with sample data, ESLint/Prettier/clippy/rustfmt, README, licence — done
2. **CI:** lint, tests and builds on Linux and Windows on every push; release workflow for tags —
   done
3. **Design:** mock-ups and the open design decisions — done
4. **Rust foundation:** error types, SQLite with migrations (FTS5, WAL), keyring, tracing with
   redaction, all in `crates/core` without Tauri — done
5. **Accounts:** model, dialog, settings suggestion, login check, folder list, removal — done
6. **Sync engine:** inbox and opened folders into SQLite, UIDVALIDITY, new, changed and deleted
   mail, older mail on demand, progress events — done
7. **Message list and reader:** virtualized, both only from the cache, large mail loads its body
   when opened, keyboard control — done
8. **Safe HTML rendering:** sanitizer, sandboxed iframe, `mail-asset://`, images on click, links
   in the system browser, hybrid dark mode — done
9. **Read, flagged, search:** queue to the server, FTS5 over all accounts and folders — done
10. **Attachments:** view, save, open → **v0.1** — done
11. **Several accounts:** folders as a tree, unified inbox, web.de and Gmail — done
12. **OAuth2 for Microsoft 365** → **v0.2** — done. `crates/core/src/oauth.rs`: authorization
    code with PKCE, redirect to a one-time loopback port (IPv4 and IPv6), access tokens only in
    memory, refresh token in the keyring (rotated on every renewal), IMAP sign-in with `XOAUTH2`
    as the address from the ID token. Registration: app "Elevant Mail", client ID
    `430b3404-3b08-41df-a2f6-23e5375d9bd7`, Elevant's tenant.

Phase 3:

13. **Account settings:** settings window, rename, colour, server settings and password with a login
    check, sign in again with Microsoft — done

14. **Archive, delete, move:** with undo, several messages at once, `e` / `#` / `Del` / `v` / `z`
    — done

15. **New mail at once and notifications:** IMAP IDLE per account, desktop notifications with
    their settings — done

16. **Conversations:** grouped list, conversation in the reader with the replies from "Sent",
    switchable — done

17. **Filters and search on the server:** filter bar, `von:`/`ist:`/`hat:` in the search, all mail
    from a sender, search on the server on request, period kept per account — done

18. **Polish:** the list in pages of 300, filters in the database, one tab stop per part with the
    arrow keys in the folder tree, F6, focus back after dialogs — done

19. **Settings → Reading:** accounts in the unified inbox, inbox at start, mark-as-read delay,
    images always from a sender — done

Phase 4:

20. **Sending:** SMTP with password and Microsoft sign-in, sender name, writing in the reader
    column with formatting, reply, reply all, forward, Sent filled where the server does not, address
    suggestions from the cache — done

Next: drafts that survive a restart, then attachments and signatures.
