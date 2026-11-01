# Changelog

What changed in each version of Elevant Mail. The release workflow takes a version's section as
the text of its GitHub release, so every version gets its entry before it is tagged.

## Unreleased

### New

- **Keeps running in the tray.** Closing the window hides it behind a tray icon, and new mail is still fetched and announced; the tray menu opens the window, starts a new mail or quits. The first close says where the app went. Settings → Notifications switches it off, and can start the app at login, in the tray without a window.
- **One app at a time.** Starting it again brings the running app's window back.
- **Mail for AI agents (MCP).** Agents such as Claude Code can search and read the mail of the accounts you share with them, from the copy on your computer and without passwords: `elevant-mail mcp` is the server, Settings → AI agents switches it on, shows the command to connect Claude Code and logs every access. Off until you switch it on, and per account you choose what agents may do besides reading: mark mail, move and archive it, put it in the trash, write drafts. They never send mail or delete it for good; their changes reach the server through the app.
- **Calendar.** A column of icons at the left edge switches between mail and a calendar with day, week and month view (`Ctrl+1`, `Ctrl+2`). Create, change and delete events, also all-day and repeating ones; a click on an empty slot starts one there.
- **Your calendars over CalDAV.** For accounts with a password the app finds their calendars on the mail server (Mailcow/SOGo, Nextcloud, Posteo, mailbox.org, iCloud and others) and keeps them in step both ways; where it does not find them, enter the address in the account settings under Calendar.
- **Calendars of Microsoft 365 and Outlook.com** over Microsoft Graph, both ways, series included. Accounts added before ask once to "Allow calendars": a new sign-in in the browser. Invitations in these accounts are answered over Microsoft, which tells the organizer.
- **Contacts.** A third area (`Ctrl+3`) lists your contacts by name, all address books together or one at a time, with a search that ignores accents. Add and change contacts with mail addresses, phone numbers, addresses, birthday and a note; a click on an address starts a mail.
- **Your address books over CardDAV.** Where the app finds your calendars on the mail server (Mailcow/SOGo, Nextcloud, Posteo, mailbox.org, iCloud), it finds the address books too and keeps them in step both ways. Photos and anything else the app does not edit stay in the contact as they were. The account settings call the switch "Calendar and contacts".
- **Contacts of Microsoft 365 and Outlook.com** over Microsoft Graph, both ways, with their folders. Accounts added before ask once to "Allow contacts": a new sign-in in the browser.
- **Reminders.** An event can remind you, from the moment it starts to two days before; the reminder shows as a notification, and those of Microsoft 365 and CalDAV calendars come along. Settings → Notifications switches reminders off and sets the one for new events.
- **One date of a series.** Changing or deleting a date of a repeating event asks whether only this date or the whole series; on Microsoft 365 the series' other changed dates stay as they are.
- **A colour per calendar.** The button beside a calendar in the sidebar gives it a colour of its own, or back to the account's.
- **Invitations meet the calendar.** An invitation says what else is on at that time and whether it is in your calendar already. Accepting puts it there, declining takes it out, and a cancellation offers to remove it. Events in calendars whose server sends invitations (Microsoft 365, Mailcow/SOGo, Nextcloud) can have attendees; the server invites them.
- **Invitations in the reader.** A mail with an invitation shows above its text when and where the event is, who is invited and how a series repeats ("Weekly on Tuesday, 4 times"), with the buttons Accept, Maybe and Decline. The answer goes to the organizer by mail; the app remembers it and offers the buttons again when the organizer changes the event. Cancellations and answers from others show as well.

- **Contacts as you type.** Writing a recipient or inviting to an event offers your contacts first, by name, company or address ("muller" finds "Müller"), then the addresses from your mail.
- **A menu for mails.** A right-click on a mail in the list offers reply, reply all and forward, read or unread, the star, archive, move and delete, each with its key. On a selected mail it acts on the whole selection.

### Changed

- **No browser menu.** A right-click no longer offers the web view's "Reload" and "Back"; in text fields and on selected text the menu for copying and pasting stays. F5 and Ctrl+R no longer reload the window.
- **After signing in to Microsoft**, calendars and contacts come in at once; while Microsoft still takes a few seconds to apply what was just allowed, the app asks again.

## 0.5.0 – 2026-10-30

Elevant Mail 0.5 lets you sort mail by dragging it and the window take the shape you like, and it keeps Microsoft accounts on Windows.

### New

- **Drag mail onto folders.** Drag a mail from the list onto a folder in the sidebar to move it there; a selected mail takes the whole selection along. The folder lights up under the pointer, and the notice offers to undo as after moving with `v`. `Esc` cancels.
- **Columns as wide as you like.** Drag the edge right of the sidebar or of the list; the app keeps the widths, and a double click on the edge goes back to the default. The reading pane always keeps room, also in a window made narrower later.

### Fixed

- **Microsoft accounts on Windows** could not be added or imported: after the sign-in the app said "Something went wrong". Windows keeps at most 1280 characters per stored credential, and Microsoft's sign-in is longer; the app now stores it in several parts.

### Updating

Install over the previous version; your accounts and mail stay. If adding a Microsoft account failed on Windows, add or import it again now.

## 0.4.1 – 2026-10-30

Elevant Mail 0.4.1 moves your accounts to another computer, so you set them up only once.

### New

- **Move accounts** under Settings → Move accounts. Export saves all accounts to a file, for a USB stick for example: servers, colour, sender name, signature, how long mail stays offline, the unified inbox choice and the senders whose images you allowed. Mail is not included; the other computer fetches it from the servers.
- **Passwords only on request, encrypted.** Switch on "Include passwords" and choose a passphrase: passwords and Microsoft sign-ins go along encrypted (Argon2id, AES-256-GCM). Without it the file holds no passwords, and the other computer asks for each one once.
- **Import** opens the file, unlocks it with the passphrase and adds the accounts, each login checked first. Accounts already set up are skipped; Microsoft accounts whose sign-in did not come along sign in in the browser.
- **Update notice:** once a day the app asks GitHub whether a newer version was released and shows it in the title bar. It downloads nothing; Settings → Notifications switches it off.

### Updating

Install over the previous version; your accounts and mail stay. Delete the export file once your accounts are moved.

## 0.4.0 – 2026-10-30

Elevant Mail 0.4 writes mail. Until now it only read; now it sends, replies and forwards from all your accounts.

### New

- **Sending** over each account's outgoing server, with its password or, for Microsoft 365 and Outlook.com, the sign-in via Microsoft. The outgoing server is suggested from the incoming one and can be changed under Settings → account → Sending.
- **Writing** right next to the list: new mail (`c`), reply (`r`), reply all (`a`) and forward (`f`). Bold, italic, underline, lists, quotes and links; `Ctrl+Enter` sends. Recipients come with suggestions from your mail, Cc and Bcc on request.
- **Replies stay in their conversation:** they follow Reply-To, quote the text below your answer and show up in the conversation, your own reply marked "Sent".
- **Sent mail lands in Sent once.** Gmail and Exchange file it themselves; for other servers the app does it.
- **Drafts** are saved as you type and kept in your account's drafts folder, so you can finish them later or on your phone. A draft in the drafts folder, also one from another device, opens to write on.
- **Attachments** with the paperclip or by dragging files onto the mail, up to 18 MB together. Forwarding takes the original's files along.
- **Signatures** per account, written in the app or pasted as HTML with logo and table; the app removes scripts and forms from pasted HTML. They go above the quote and change with the sending account.
- **Sender name** per account, taken from your sent mail until you set one.

### Updating

Install over the previous version; your accounts and mail stay. Existing accounts get their outgoing server suggested; check it under Settings → account → Sending if sending fails.

### Known limits

- Microsoft 365: if your organization switched off sending from mail apps, an admin has to allow "Authenticated SMTP" for your mailbox (Microsoft 365 admin center → Users → Mail → Manage email apps).
- No undo for sending yet.
- Closing the window quits the app, so no notifications arrive while it is closed.

## 0.3.0 – 2026-10-30

Elevant Mail 0.3 makes everyday reading comfortable. It still only reads mail: sending comes next.

### New

- **New mail right away.** The app waits for new mail with IMAP IDLE instead of checking every few minutes, and shows a desktop notification while its window is in the background, with or without sender and subject.
- **Conversations.** Replies sit with the mail they answer, your own replies from Sent included. The newest and the unread messages are open, older ones folded. Prefer single messages? Switch them back on in the appearance.
- **Archive, delete and move** one mail or many, with ten seconds to undo. Select with `Ctrl`/`Shift`-click, `Shift`+`j`/`k` or `Ctrl+A`; keys `e`, `#` or `Del`, `v` and `z`.
- **Filters and search.** A filter bar for unread, starred and mail with attachments. Filters in the search field: `from:`, `is:unread`, `is:starred`, `has:attachment`, in German too. All mail from a sender with one click in an open mail. On request, a search on your mail servers for older mail.
- **How much stays offline,** per account: one month, three months (the default), a year or everything.
- **Settings** behind the gear icon: per account its name, color, server settings and password, sign in again with Microsoft, and the offline period; notifications; and a **Reading** page with which accounts the unified inbox brings together, which inbox opens at start, when an opened mail counts as read, and the senders whose images always load.
- **Large folders** load in pages, so tens of thousands of mails scroll smoothly.
- **Keyboard:** `Tab` reaches each part of the window once, the arrow keys move through the folder tree, `F6` jumps between search, folders, list and message, `Home` and `End` go to the ends of the list. Dialogs give the focus back to where it was.
- **English interface** next to German. It follows the system language or is chosen in the appearance.

### Updating

Install over the previous version; your accounts and the mail cache stay. On the first start the app groups the cached mail into conversations once and then fetches the last three months of each inbox. With large mailboxes that runs for a few minutes in the background.

### Known limits

- No writing or sending yet.
- Closing the window quits the app, so no notifications arrive while it is closed.
- On Linux, shortcuts pause while the focus is inside a mail's text; click outside it to go on.

## 0.2.0 – 2026-10-29

The first release: Elevant Mail reads mail from several accounts.

### New

- **Accounts** with the server settings suggested from the address and the login checked before saving. Passwords stay in the system keyring (GNOME Keyring or KWallet on Linux, Credential Manager on Windows).
- **Microsoft 365 and Outlook.com** sign in with Microsoft in the browser, no app password needed.
- **Several accounts** with a folder tree each, special folders first, a color per account and a unified inbox.
- **Reading** in a fast list that also handles large folders, HTML mail shown safely in a sandbox; images from the web only on click, links open in the browser.
- **Attachments:** view, save and open.
- **Read, unread and starred,** at once in the app and then on the server.
- **Search** in the mail on your computer, and **keyboard control** for the list and the reader.
- **Works offline:** the app starts with the mail it has and fetches the rest when the network is back.
- **Dark and light interface** with several themes and two densities.

### Known limits

- No writing or sending yet.
- Closing the window quits the app.
