/**
 * Sample data for working on the UI in a plain browser (`pnpm dev`). All people, companies and
 * addresses are made up; domains are reserved example domains. Adding an account is simulated:
 * the password "falsch" fails the login, anything else succeeds.
 */
import type {
  Account,
  Address,
  Backend,
  Folder,
  FolderRole,
  Message,
  MessageSummary,
  SuggestedSettings,
} from "./tauri";
import { UNIFIED_INBOX } from "./tauri";

const MINUTE = 60 * 1000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

let accounts: Account[] = [
  {
    id: "acc-private",
    displayName: "Privat",
    emailAddress: "alex@example.org",
    color: "oklch(0.72 0.13 250)",
  },
  {
    id: "acc-work",
    displayName: "Arbeit",
    emailAddress: "alex.berger@example.com",
    color: "oklch(0.74 0.14 60)",
  },
];

const folderRoles: { role: FolderRole; name: string }[] = [
  { role: "inbox", name: "INBOX" },
  { role: "drafts", name: "Drafts" },
  { role: "sent", name: "Sent" },
  { role: "archive", name: "Archive" },
  { role: "junk", name: "Junk" },
  { role: "trash", name: "Trash" },
];

const folderId = (accountId: string, role: FolderRole) => `${accountId}:${role}`;

interface MockMessage {
  account: string;
  role: FolderRole;
  from: Address;
  to?: Address[];
  cc?: Address[];
  subject: string | null;
  ago: number;
  read?: boolean;
  flagged?: boolean;
  attachments?: boolean;
  body: string;
}

const me = (accountId: string): Address => {
  const account = accounts.find((a) => a.id === accountId)!;
  return { name: "Alex Berger", address: account.emailAddress };
};

const mockMessages: MockMessage[] = [
  {
    account: "acc-private",
    role: "inbox",
    from: { name: "Lena Fischer", address: "lena.fischer@example.org" },
    subject: "Wanderung am Samstag?",
    ago: 14 * MINUTE,
    body: "Hi Alex,\n\nhast du am Samstag Lust auf die große Runde um den See? Wir würden gegen 9 Uhr am Bahnhof losfahren und unterwegs irgendwo einkehren.\n\nSag mir bis Freitag Bescheid, dann reserviere ich einen Tisch.\n\nViele Grüße\nLena",
  },
  {
    account: "acc-work",
    role: "inbox",
    from: { name: "Martin Schulz", address: "m.schulz@example.com" },
    subject: "Agenda für Donnerstag",
    ago: 52 * MINUTE,
    flagged: true,
    body: "Hallo zusammen,\n\nfür Donnerstag schlage ich folgende Punkte vor:\n\n1. Stand der Migration\n2. Budget für das erste Quartal\n3. Urlaubsplanung über die Feiertage\n\nErgänzungen gern bis morgen Mittag.\n\nGruß\nMartin",
  },
  {
    account: "acc-private",
    role: "inbox",
    from: { name: "Paketdienst", address: "benachrichtigung@example.net" },
    subject: "Ihre Sendung kommt heute",
    ago: 2 * HOUR,
    body: "Guten Tag,\n\nIhre Sendung wird heute zwischen 13:00 und 15:00 Uhr zugestellt. Wenn Sie nicht zu Hause sind, wird das Paket bei einem Nachbarn abgegeben.\n\nIhr Paketdienst",
  },
  {
    account: "acc-work",
    role: "inbox",
    from: { name: "Projektteam", address: "team@example.com" },
    subject: "Protokoll Sprint-Review",
    ago: 4 * HOUR,
    attachments: true,
    body: "Hallo,\n\nanbei das Protokoll vom heutigen Sprint-Review. Die offenen Punkte sind im Board eingetragen.\n\nDanke an alle für die Demo!\n\nDas Projektteam",
  },
  {
    account: "acc-private",
    role: "inbox",
    from: { name: "Jonas Weber", address: "jonas.weber@example.org" },
    subject: "Fotos vom Grillabend",
    ago: 6 * HOUR,
    read: true,
    attachments: true,
    body: "Servus,\n\nhier sind endlich die Fotos vom Samstag. Das Bild mit dem verbrannten Maiskolben ist mein Favorit.\n\nBis bald\nJonas",
  },
  {
    account: "acc-work",
    role: "inbox",
    from: { name: "Clara Neumann", address: "clara.neumann@example.com" },
    cc: [{ name: "Martin Schulz", address: "m.schulz@example.com" }],
    subject: "Kurze Frage zum Angebot",
    ago: 1 * DAY + 2 * HOUR,
    read: true,
    body: "Hallo Alex,\n\nim Angebot für den Kunden steht bei Position 4 ein Stundensatz, der nicht zu unserer aktuellen Preisliste passt. Ist das Absicht?\n\nDanke dir!\nClara",
  },
  {
    account: "acc-private",
    role: "inbox",
    from: { name: "Mama", address: "mama@example.org" },
    subject: "Sonntag Essen",
    ago: 1 * DAY + 5 * HOUR,
    read: true,
    flagged: true,
    body: "Hallo mein Schatz,\n\nkommt ihr am Sonntag zum Essen? Es gibt Rouladen. Bring bitte die Auflaufform mit, die du noch hast.\n\nKuss\nMama",
  },
  {
    account: "acc-work",
    role: "inbox",
    from: { name: "Rechnungswesen", address: "buchhaltung@example.com" },
    subject: "Rechnung 2026-1042",
    ago: 2 * DAY,
    read: true,
    attachments: true,
    body: "Sehr geehrte Damen und Herren,\n\nim Anhang finden Sie die Rechnung 2026-1042. Bitte überweisen Sie den Betrag innerhalb von 14 Tagen.\n\nMit freundlichen Grüßen\nRechnungswesen",
  },
  {
    account: "acc-private",
    role: "inbox",
    from: { name: "Stadtbibliothek", address: "ausleihe@example.net" },
    subject: "Erinnerung: Rückgabe in 3 Tagen",
    ago: 2 * DAY + 7 * HOUR,
    read: true,
    body: "Liebe Leserin, lieber Leser,\n\ndie Leihfrist für folgende Medien endet in drei Tagen:\n\n- Der Gesang der Flusskrebse\n- Kochen ohne Rezept\n\nSie können die Frist online verlängern.\n\nIhre Stadtbibliothek",
  },
  {
    account: "acc-work",
    role: "inbox",
    from: { name: "Tim Hoffmann", address: "tim.hoffmann@example.com" },
    subject: "Re: Re: Release 0.1 – Testplan",
    ago: 3 * DAY,
    read: true,
    body: "Ich habe den Testplan um die Fälle für den Offline-Start ergänzt. Können wir den Rauchtest am Freitag machen?\n\nTim\n\n> Am Montag schrieb Alex:\n> Passt für mich, bitte noch die Windows-Fälle aufnehmen.",
  },
  {
    account: "acc-private",
    role: "inbox",
    from: { name: "Hausverwaltung Berger & Söhne", address: "info@example.net" },
    subject: "Ablesung der Heizkörper am 12. November",
    ago: 4 * DAY,
    read: true,
    attachments: true,
    body: "Sehr geehrte Mieterinnen und Mieter,\n\nam 12. November zwischen 8 und 12 Uhr werden die Heizkörper abgelesen. Bitte sorgen Sie dafür, dass alle Räume zugänglich sind.\n\nMit freundlichen Grüßen\nIhre Hausverwaltung",
  },
  {
    account: "acc-work",
    role: "inbox",
    from: { name: "Serverüberwachung", address: "monitor@example.com" },
    subject: "Warnung: Speicherplatz unter 15 %",
    ago: 5 * DAY,
    read: true,
    body: "Auf dem Host build-02 sind nur noch 14 % Speicherplatz frei.\n\nVolume: /var/lib/docker\nFrei: 41 GB von 300 GB",
  },
  {
    account: "acc-private",
    role: "inbox",
    from: { name: "Kochwerk", address: "newsletter@example.net" },
    subject: "5 Rezepte für den Herbst",
    ago: 6 * DAY,
    read: true,
    body: "Kürbis, Maronen, Pilze: Unsere fünf Lieblingsrezepte für die kalte Jahreszeit, alle in unter 30 Minuten fertig.",
  },
  {
    account: "acc-work",
    role: "inbox",
    from: { name: "Anna Krüger", address: "anna.krueger@example.com" },
    subject: "Feedback zum Entwurf",
    ago: 9 * DAY,
    read: true,
    flagged: true,
    body: "Hallo Alex,\n\nder Entwurf gefällt mir gut. Zwei Anmerkungen:\n\n- Die Einleitung könnte kürzer sein.\n- Bei den Kosten fehlt die Wartung.\n\nViele Grüße\nAnna",
  },
  {
    account: "acc-private",
    role: "inbox",
    from: { name: "Sophie Braun", address: "sophie.braun@example.org" },
    subject: "Re: Umzugshilfe",
    ago: 16 * DAY,
    read: true,
    body: "Klar helfe ich! Ich bringe noch zwei Freunde mit, dann sind wir schnell durch.\n\nSophie",
  },
  {
    account: "acc-work",
    role: "inbox",
    from: { name: null, address: "kalender@example.com" },
    subject: "Einladung: Jour fixe (wöchentlich)",
    ago: 23 * DAY,
    read: true,
    body: "Sie wurden zu einem wiederkehrenden Termin eingeladen.\n\nJour fixe\nJeden Montag, 10:00–10:30 Uhr",
  },
  {
    account: "acc-private",
    role: "inbox",
    from: { name: "Fitnessstudio Nordstern", address: "kurse@example.net" },
    subject: null,
    ago: 40 * DAY,
    read: true,
    body: "Der neue Kursplan ist da. Neu im Programm: Rückenfit am Dienstagabend.",
  },
  {
    account: "acc-work",
    role: "inbox",
    from: { name: "Personalabteilung", address: "personal@example.com" },
    subject: "Weihnachtsfeier 2025",
    ago: 320 * DAY,
    read: true,
    body: "Liebe Kolleginnen und Kollegen,\n\nunsere Weihnachtsfeier findet am 18. Dezember statt. Bitte meldet euch bis Ende November an.",
  },
  {
    account: "acc-private",
    role: "sent",
    from: { name: "Alex Berger", address: "alex@example.org" },
    to: [{ name: "Sophie Braun", address: "sophie.braun@example.org" }],
    subject: "Umzugshilfe",
    ago: 17 * DAY,
    read: true,
    body: "Hi Sophie,\n\nam 14. ziehe ich um. Hättest du Zeit, mit anzupacken? Pizza geht auf mich.\n\nAlex",
  },
  {
    account: "acc-work",
    role: "archive",
    from: { name: "Martin Schulz", address: "m.schulz@example.com" },
    subject: "Zugänge für das neue Projekt",
    ago: 60 * DAY,
    read: true,
    body: "Die Zugänge sind eingerichtet. Melde dich, falls etwas fehlt.",
  },
];

function buildMessages(): Message[] {
  const now = Date.now();
  return mockMessages.map((m, index) => ({
    id: `msg-${index + 1}`,
    accountId: m.account,
    folderId: folderId(m.account, m.role),
    subject: m.subject,
    from: m.from,
    to: m.to ?? [me(m.account)],
    cc: m.cc ?? [],
    dateSent: now - m.ago,
    snippet: m.body.replace(/\s+/g, " ").slice(0, 160),
    isRead: m.read ?? false,
    isFlagged: m.flagged ?? false,
    hasAttachments: m.attachments ?? false,
    bodyText: m.body,
  }));
}

const messages = buildMessages();

function buildFolders(): Folder[] {
  return accounts.flatMap((account) =>
    folderRoles.map(({ role, name }) => {
      const id = folderId(account.id, role);
      const inFolder = messages.filter((m) => m.folderId === id);
      return {
        id,
        accountId: account.id,
        name,
        role,
        unreadCount: inFolder.filter((m) => !m.isRead).length,
        totalCount: inFolder.length,
      };
    }),
  );
}

let folders = buildFolders();

function toSummary({ to: _to, cc: _cc, bodyText: _body, ...summary }: Message): MessageSummary {
  return summary;
}

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

function suggest(email: string): SuggestedSettings {
  const domain = email.split("@")[1]?.toLowerCase() ?? "";
  const known = domain === "gmail.com";
  return {
    imap: {
      host: known ? "imap.gmail.com" : `imap.${domain}`,
      port: 993,
      security: "tls",
      username: email,
    },
    source: known ? "preset" : "guess",
    provider: known ? "gmail" : null,
    needsOauth: false,
  };
}

export const mockBackend: Backend = {
  listAccounts: async () => accounts,
  detectSettings: async (email) => {
    await wait(400);
    return suggest(email.trim());
  },
  addAccount: async (account, password) => {
    await wait(900);
    if (password === "falsch") {
      throw { kind: "authenticationFailed", hint: "checkCredentials" };
    }
    const added: Account = {
      id: `acc-${accounts.length + 1}-${Date.now()}`,
      displayName: account.displayName || account.emailAddress,
      emailAddress: account.emailAddress,
      color: "oklch(0.72 0.14 150)",
    };
    accounts = [...accounts, added];
    folders = [
      ...folders,
      ...folderRoles.map(({ role, name }) => ({
        id: folderId(added.id, role),
        accountId: added.id,
        name,
        role,
        unreadCount: 0,
        totalCount: 0,
      })),
    ];
    return added;
  },
  removeAccount: async (accountId) => {
    accounts = accounts.filter((a) => a.id !== accountId);
    folders = folders.filter((f) => f.accountId !== accountId);
  },
  listFolders: async () => folders,
  listMessages: async (mailboxId) => {
    const inMailbox =
      mailboxId === UNIFIED_INBOX
        ? messages.filter((m) => m.folderId.endsWith(":inbox"))
        : messages.filter((m) => m.folderId === mailboxId);
    return inMailbox.map(toSummary).sort((a, b) => b.dateSent - a.dateSent);
  },
  getMessage: async (messageId) => messages.find((m) => m.id === messageId) ?? null,
};
