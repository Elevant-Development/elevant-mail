/**
 * Sample data for working on the UI in a plain browser (`pnpm dev`). All people, companies and
 * addresses are made up; domains are reserved example domains. Texts follow the interface language.
 * Adding an account or changing its login is simulated: the password "falsch" fails the login,
 * anything else succeeds.
 */
import type {
  Account,
  AccountSettings,
  Address,
  Backend,
  Folder,
  FolderRole,
  Message,
  MessageSummary,
  MoveOutcome,
  SuggestedSettings,
} from "./tauri";
import { language, locale } from "./i18n";
import { UNIFIED_INBOX } from "./tauri";

const MINUTE = 60 * 1000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** Sample text in the interface language. */
const L = (de: string, en: string) => (language === "de" ? de : en);

let accounts: Account[] = [
  {
    id: "acc-private",
    displayName: L("Privat", "Personal"),
    emailAddress: "alex@example.org",
    color: "oklch(0.72 0.13 250)",
  },
  {
    id: "acc-work",
    displayName: L("Arbeit", "Work"),
    emailAddress: "alex.berger@example.com",
    color: "oklch(0.74 0.14 60)",
  },
];

const accountSettings: Record<string, AccountSettings> = {
  "acc-private": {
    signIn: "password",
    imap: { host: "imap.example.org", port: 993, security: "tls", username: "alex@example.org" },
  },
  "acc-work": {
    signIn: "microsoft",
    imap: {
      host: "outlook.office365.com",
      port: 993,
      security: "tls",
      username: "alex.berger@example.com",
    },
  },
};

const folderRoles: { role: FolderRole; name: string }[] = [
  { role: "inbox", name: "INBOX" },
  { role: "drafts", name: "Drafts" },
  { role: "sent", name: "Sent" },
  { role: "archive", name: "Archive" },
  { role: "junk", name: "Junk" },
  { role: "trash", name: "Trash" },
];

const folderId = (accountId: string, role: FolderRole) => `${accountId}:${role}`;

/** Folders of the work account without a role, to show the tree: "/"-separated paths. */
const workFolders = [
  L("INBOX/Projekte", "INBOX/Projects"),
  L("INBOX/Projekte/Website", "INBOX/Projects/Website"),
  L("Kunden", "Clients"),
  L("Kunden/Berger", "Clients/Berger"),
  L("Kunden/Lindenau", "Clients/Lindenau"),
];

function roleFolder(accountId: string, role: FolderRole, name: string): Folder {
  return {
    id: folderId(accountId, role),
    accountId,
    name,
    path: name,
    pathNames: [name],
    role,
    unreadCount: 0,
    totalCount: 0,
  };
}

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
  /** Cached with headers only, like large mail from the server. */
  large?: boolean;
  body: string;
  html?: MockHtml;
  files?: MockFile[];
}

interface MockFile {
  name: string;
  type: string;
  size: number;
}

/** HTML as the backend hands it out: already sanitized. */
interface MockHtml {
  body: string;
  styled: boolean;
  /** External images: `data-remote="n"` in the body gets the n-th once allowed. */
  images?: string[];
}

const swatch = (color: string, label: string) =>
  `data:image/svg+xml,${encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" width="552" height="180"><rect width="552" height="180" fill="${color}"/><text x="24" y="100" font-family="sans-serif" font-size="28" fill="#fff">${label}</text></svg>`,
  )}`;

const pumpkins = L("Kürbiswoche", "Pumpkin week");
const apples = L("Äpfel aus der Region", "Local apples");

const newsletter: MockHtml = {
  styled: true,
  images: [swatch("#c2410c", pumpkins), swatch("#3f6212", apples)],
  body: `<style>.card { background: #fff7ed; border-radius: 12px; padding: 16px 24px; } h1 { color: #9a3412; font-family: Georgia, serif; }</style>
<table width="600" cellpadding="0" cellspacing="0" bgcolor="#ffffff" align="center"><tbody>
<tr><td><img data-remote="0" width="552" height="180" alt="${pumpkins}"></td></tr>
<tr><td class="card"><h1>${L("Herbst auf dem Wochenmarkt", "Autumn at the market")}</h1>
<p>${L(
    "Diese Woche gibt es frische Kürbisse, Äpfel und den ersten Glühwein.",
    "This week: fresh pumpkins, apples and the first mulled wine.",
  )} <a href="https://markt.example.org/angebote">${L("Alle Angebote ansehen", "See all offers")}</a></p>
<p><img data-remote="1" width="552" height="180" alt="${apples}"></p>
<p style="color: #57534e; font-size: 13px">${L("Gutschein einlösen unter", "Redeem your voucher at")} <a href="https://login.example.net/gutschein">www.markt.example.org/gutschein</a></p>
</td></tr></tbody></table>`,
};

function renderMockHtml(html: MockHtml, remoteImages: boolean): string {
  return html.body.replace(/ data-remote="(\d+)"/g, (_, index: string) =>
    remoteImages ? ` src="${html.images?.[Number(index)] ?? ""}"` : "",
  );
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
    subject: L("Wanderung am Samstag?", "Hike on Saturday?"),
    ago: 14 * MINUTE,
    body: L(
      "Hi Alex,\n\nhast du am Samstag Lust auf die große Runde um den See? Wir würden gegen 9 Uhr am Bahnhof losfahren und unterwegs irgendwo einkehren.\n\nSag mir bis Freitag Bescheid, dann reserviere ich einen Tisch.\n\nViele Grüße\nLena",
      "Hi Alex,\n\nfancy the long loop around the lake on Saturday? We'd leave the station around 9 and stop for lunch somewhere on the way.\n\nLet me know by Friday and I'll book a table.\n\nCheers\nLena",
    ),
    html: {
      styled: false,
      body: L(
        `<div>Hi Alex,</div><div><br></div><div>hast du am Samstag Lust auf die <b>große Runde um den See</b>? Wir würden gegen 9 Uhr am Bahnhof losfahren und unterwegs irgendwo einkehren. Die Strecke: <a href="https://karten.example.org/route/see">Route ansehen</a></div><div><br></div><div>Sag mir bis Freitag Bescheid, dann reserviere ich einen Tisch.</div><div><br></div><div>Viele Grüße<br>Lena</div><blockquote><div>Am Montag schrieb Alex:</div><div>Wollen wir am Wochenende mal wieder raus?</div></blockquote>`,
        `<div>Hi Alex,</div><div><br></div><div>fancy the <b>long loop around the lake</b> on Saturday? We'd leave the station around 9 and stop for lunch somewhere on the way. The route: <a href="https://karten.example.org/route/see">see the map</a></div><div><br></div><div>Let me know by Friday and I'll book a table.</div><div><br></div><div>Cheers<br>Lena</div><blockquote><div>On Monday Alex wrote:</div><div>Shall we get out of town this weekend?</div></blockquote>`,
      ),
    },
  },
  {
    account: "acc-private",
    role: "inbox",
    from: {
      name: L("Wochenmarkt Lindenau", "Lindenau Farmers' Market"),
      address: "post@markt.example.org",
    },
    subject: L("Herbst auf dem Wochenmarkt", "Autumn at the market"),
    ago: 50 * MINUTE,
    read: true,
    body: L(
      "Herbst auf dem Wochenmarkt. Diese Woche gibt es frische Kürbisse, Äpfel und den ersten Glühwein.",
      "Autumn at the market. This week: fresh pumpkins, apples and the first mulled wine.",
    ),
    html: newsletter,
  },
  {
    account: "acc-work",
    role: "inbox",
    from: { name: "Martin Schulz", address: "m.schulz@example.com" },
    subject: L("Agenda für Donnerstag", "Agenda for Thursday"),
    ago: 52 * MINUTE,
    flagged: true,
    body: L(
      "Hallo zusammen,\n\nfür Donnerstag schlage ich folgende Punkte vor:\n\n1. Stand der Migration\n2. Budget für das erste Quartal\n3. Urlaubsplanung über die Feiertage\n\nErgänzungen gern bis morgen Mittag.\n\nGruß\nMartin",
      "Hi all,\n\nfor Thursday I suggest these points:\n\n1. Status of the migration\n2. Budget for the first quarter\n3. Holiday planning\n\nAdditions welcome until tomorrow noon.\n\nBest\nMartin",
    ),
  },
  {
    account: "acc-private",
    role: "inbox",
    from: { name: L("Paketdienst", "Parcel service"), address: "benachrichtigung@example.net" },
    subject: L("Ihre Sendung kommt heute", "Your parcel arrives today"),
    ago: 2 * HOUR,
    body: L(
      "Guten Tag,\n\nIhre Sendung wird heute zwischen 13:00 und 15:00 Uhr zugestellt. Wenn Sie nicht zu Hause sind, wird das Paket bei einem Nachbarn abgegeben.\n\nIhr Paketdienst",
      "Hello,\n\nyour parcel will be delivered today between 1 and 3 pm. If you are not at home, it will be left with a neighbour.\n\nYour parcel service",
    ),
  },
  {
    account: "acc-work",
    role: "inbox",
    from: { name: L("Projektteam", "Project team"), address: "team@example.com" },
    subject: L("Protokoll Sprint-Review", "Sprint review notes"),
    ago: 4 * HOUR,
    attachments: true,
    files: [
      {
        name: L("Protokoll Sprint-Review.pdf", "Sprint review notes.pdf"),
        type: "application/pdf",
        size: 248_320,
      },
    ],
    large: true,
    body: L(
      "Hallo,\n\nanbei das Protokoll vom heutigen Sprint-Review. Die offenen Punkte sind im Board eingetragen.\n\nDanke an alle für die Demo!\n\nDas Projektteam",
      "Hi,\n\nattached are the notes from today's sprint review. The open points are on the board.\n\nThanks everyone for the demo!\n\nThe project team",
    ),
  },
  {
    account: "acc-private",
    role: "inbox",
    from: { name: "Jonas Weber", address: "jonas.weber@example.org" },
    subject: L("Fotos vom Grillabend", "Photos from the barbecue"),
    ago: 6 * HOUR,
    read: true,
    attachments: true,
    files: [
      { name: "IMG_2041.jpg", type: "image/jpeg", size: 3_481_002 },
      { name: "IMG_2042.jpg", type: "image/jpeg", size: 3_102_114 },
      { name: L("Fotos.zip", "Photos.zip"), type: "application/zip", size: 18_420_660 },
    ],
    body: L(
      "Servus,\n\nhier sind endlich die Fotos vom Samstag. Das Bild mit dem verbrannten Maiskolben ist mein Favorit.\n\nBis bald\nJonas",
      "Hey,\n\nhere are the photos from Saturday at last. The one with the burnt corn cob is my favourite.\n\nSee you\nJonas",
    ),
  },
  {
    account: "acc-work",
    role: "inbox",
    from: { name: "Clara Neumann", address: "clara.neumann@example.com" },
    cc: [{ name: "Martin Schulz", address: "m.schulz@example.com" }],
    subject: L("Kurze Frage zum Angebot", "Quick question about the quote"),
    ago: 1 * DAY + 2 * HOUR,
    read: true,
    body: L(
      "Hallo Alex,\n\nim Angebot für den Kunden steht bei Position 4 ein Stundensatz, der nicht zu unserer aktuellen Preisliste passt. Ist das Absicht?\n\nDanke dir!\nClara",
      "Hi Alex,\n\nitem 4 of the quote for the client has an hourly rate that doesn't match our current price list. Is that on purpose?\n\nThanks!\nClara",
    ),
  },
  {
    account: "acc-private",
    role: "inbox",
    from: { name: L("Mama", "Mom"), address: "mama@example.org" },
    subject: L("Sonntag Essen", "Sunday lunch"),
    ago: 1 * DAY + 5 * HOUR,
    read: true,
    flagged: true,
    body: L(
      "Hallo mein Schatz,\n\nkommt ihr am Sonntag zum Essen? Es gibt Rouladen. Bring bitte die Auflaufform mit, die du noch hast.\n\nKuss\nMama",
      "Hi darling,\n\nare you coming for lunch on Sunday? I'm making a roast. Please bring back the baking dish you still have.\n\nLove\nMom",
    ),
  },
  {
    account: "acc-work",
    role: "inbox",
    from: { name: L("Rechnungswesen", "Accounts"), address: "buchhaltung@example.com" },
    subject: L("Rechnung 2026-1042", "Invoice 2026-1042"),
    ago: 2 * DAY,
    read: true,
    attachments: true,
    files: [
      {
        name: L("Rechnung 2026-1042.pdf", "Invoice 2026-1042.pdf"),
        type: "application/pdf",
        size: 96_210,
      },
      {
        name: L("Zahlungsportal.exe", "PaymentPortal.exe"),
        type: "application/octet-stream",
        size: 1_240_000,
      },
    ],
    body: L(
      "Sehr geehrte Damen und Herren,\n\nim Anhang finden Sie die Rechnung 2026-1042. Bitte überweisen Sie den Betrag innerhalb von 14 Tagen.\n\nMit freundlichen Grüßen\nRechnungswesen",
      "Dear Sir or Madam,\n\nplease find attached invoice 2026-1042. Kindly transfer the amount within 14 days.\n\nKind regards\nAccounts",
    ),
  },
  {
    account: "acc-private",
    role: "inbox",
    from: { name: L("Stadtbibliothek", "City library"), address: "ausleihe@example.net" },
    subject: L("Erinnerung: Rückgabe in 3 Tagen", "Reminder: due back in 3 days"),
    ago: 2 * DAY + 7 * HOUR,
    read: true,
    body: L(
      "Lieber Leser,\n\ndie Leihfrist für folgende Medien endet in drei Tagen:\n\n- Der Gesang der Flusskrebse\n- Kochen ohne Rezept\n\nSie können die Frist online verlängern.\n\nIhre Stadtbibliothek",
      "Dear reader,\n\nthe loan period for these items ends in three days:\n\n- Where the Crawdads Sing\n- Cooking without a recipe\n\nYou can renew them online.\n\nYour city library",
    ),
  },
  {
    account: "acc-work",
    role: "inbox",
    from: { name: "Tim Hoffmann", address: "tim.hoffmann@example.com" },
    subject: L("Re: Re: Release 0.1 – Testplan", "Re: Re: Release 0.1 – test plan"),
    ago: 3 * DAY,
    read: true,
    body: L(
      "Ich habe den Testplan um die Fälle für den Offline-Start ergänzt. Können wir den Rauchtest am Freitag machen?\n\nTim\n\n> Am Montag schrieb Alex:\n> Passt für mich, bitte noch die Windows-Fälle aufnehmen.",
      "I've added the offline start cases to the test plan. Can we do the smoke test on Friday?\n\nTim\n\n> On Monday Alex wrote:\n> Fine with me, please add the Windows cases too.",
    ),
  },
  {
    account: "acc-private",
    role: "inbox",
    from: {
      name: L("Hausverwaltung Berger & Söhne", "Berger & Sons Property Management"),
      address: "info@example.net",
    },
    subject: L("Ablesung der Heizkörper am 12. November", "Radiator reading on 12 November"),
    ago: 4 * DAY,
    read: true,
    attachments: true,
    body: L(
      "Sehr geehrte Mieter,\n\nam 12. November zwischen 8 und 12 Uhr werden die Heizkörper abgelesen. Bitte sorgen Sie dafür, dass alle Räume zugänglich sind.\n\nMit freundlichen Grüßen\nIhre Hausverwaltung",
      "Dear tenants,\n\nthe radiators will be read on 12 November between 8 am and noon. Please make sure all rooms are accessible.\n\nKind regards\nYour property management",
    ),
  },
  {
    account: "acc-work",
    role: "inbox",
    from: { name: L("Serverüberwachung", "Server monitoring"), address: "monitor@example.com" },
    subject: L("Warnung: Speicherplatz unter 15 %", "Warning: disk space below 15%"),
    ago: 5 * DAY,
    read: true,
    body: L(
      "Auf dem Host build-02 sind nur noch 14 % Speicherplatz frei.\n\nVolume: /var/lib/docker\nFrei: 41 GB von 300 GB",
      "Host build-02 has only 14% disk space left.\n\nVolume: /var/lib/docker\nFree: 41 GB of 300 GB",
    ),
  },
  {
    account: "acc-private",
    role: "inbox",
    from: { name: L("Kochwerk", "Kitchenworks"), address: "newsletter@example.net" },
    subject: L("5 Rezepte für den Herbst", "5 recipes for autumn"),
    ago: 6 * DAY,
    read: true,
    body: L(
      "Kürbis, Maronen, Pilze: Unsere fünf Lieblingsrezepte für die kalte Jahreszeit, alle in unter 30 Minuten fertig.",
      "Pumpkin, chestnuts, mushrooms: our five favourite recipes for the cold season, all done in under 30 minutes.",
    ),
  },
  {
    account: "acc-work",
    role: "inbox",
    from: { name: "Anna Krüger", address: "anna.krueger@example.com" },
    subject: L("Feedback zum Entwurf", "Feedback on the draft"),
    ago: 9 * DAY,
    read: true,
    flagged: true,
    body: L(
      "Hallo Alex,\n\nder Entwurf gefällt mir gut. Zwei Anmerkungen:\n\n- Die Einleitung könnte kürzer sein.\n- Bei den Kosten fehlt die Wartung.\n\nViele Grüße\nAnna",
      "Hi Alex,\n\nI like the draft. Two remarks:\n\n- The introduction could be shorter.\n- Maintenance is missing from the costs.\n\nBest\nAnna",
    ),
  },
  {
    account: "acc-private",
    role: "inbox",
    from: { name: "Sophie Braun", address: "sophie.braun@example.org" },
    subject: L("Re: Umzugshilfe", "Re: Help with the move"),
    ago: 16 * DAY,
    read: true,
    body: L(
      "Klar helfe ich! Ich bringe noch zwei Freunde mit, dann sind wir schnell durch.\n\nSophie",
      "Of course I'll help! I'll bring two friends, then we'll be done quickly.\n\nSophie",
    ),
  },
  {
    account: "acc-work",
    role: "inbox",
    from: { name: null, address: "kalender@example.com" },
    subject: L("Einladung: Jour fixe (wöchentlich)", "Invitation: weekly check-in"),
    ago: 23 * DAY,
    read: true,
    body: L(
      "Sie wurden zu einem wiederkehrenden Termin eingeladen.\n\nJour fixe\nJeden Montag, 10:00–10:30 Uhr",
      "You have been invited to a recurring meeting.\n\nWeekly check-in\nEvery Monday, 10:00–10:30",
    ),
  },
  {
    account: "acc-private",
    role: "inbox",
    from: { name: L("Fitnessstudio Nordstern", "Northstar Gym"), address: "kurse@example.net" },
    subject: null,
    ago: 40 * DAY,
    read: true,
    body: L(
      "Der neue Kursplan ist da. Neu im Programm: Rückenfit am Dienstagabend.",
      "The new class schedule is out. New: back fitness on Tuesday evenings.",
    ),
  },
  {
    account: "acc-work",
    role: "inbox",
    from: { name: L("Personalabteilung", "HR"), address: "personal@example.com" },
    subject: L("Weihnachtsfeier 2025", "Holiday party 2025"),
    ago: 320 * DAY,
    read: true,
    body: L(
      "Liebe Kollegen,\n\nunsere Weihnachtsfeier findet am 18. Dezember statt. Bitte meldet euch bis Ende November an.",
      "Dear colleagues,\n\nour holiday party is on 18 December. Please sign up by the end of November.",
    ),
  },
  {
    account: "acc-private",
    role: "sent",
    from: { name: "Alex Berger", address: "alex@example.org" },
    to: [{ name: "Sophie Braun", address: "sophie.braun@example.org" }],
    subject: L("Umzugshilfe", "Help with the move"),
    ago: 17 * DAY,
    read: true,
    body: L(
      "Hi Sophie,\n\nam 14. ziehe ich um. Hättest du Zeit, mit anzupacken? Pizza geht auf mich.\n\nAlex",
      "Hi Sophie,\n\nI'm moving on the 14th. Would you have time to lend a hand? Pizza is on me.\n\nAlex",
    ),
  },
  {
    account: "acc-work",
    role: "archive",
    from: { name: "Martin Schulz", address: "m.schulz@example.com" },
    subject: L("Zugänge für das neue Projekt", "Access for the new project"),
    ago: 60 * DAY,
    read: true,
    body: L(
      "Die Zugänge sind eingerichtet. Melde dich, falls etwas fehlt.",
      "The accounts are set up. Let me know if anything is missing.",
    ),
  },
];

/** Years of old project mail in one folder, so the virtualized list has something to scroll. */
function archiveFiller(count: number): MockMessage[] {
  const senders: Address[] = [
    { name: "Martin Schulz", address: "m.schulz@example.com" },
    { name: L("Projektteam", "Project team"), address: "team@example.com" },
    { name: L("Buchhaltung", "Accounting"), address: "buchhaltung@example.com" },
    { name: "Jana Wolf", address: "jana.wolf@example.com" },
  ];
  const topics = [
    L("Wochenbericht", "Weekly report"),
    L("Rückfrage zum Angebot", "Question about the quote"),
    L("Abstimmung Termine", "Scheduling"),
    L("Rechnung", "Invoice"),
  ];
  return Array.from({ length: count }, (_, index) => ({
    account: "acc-work",
    role: "archive",
    from: senders[index % senders.length]!,
    subject: `${topics[index % topics.length]} #${count - index}`,
    ago: 61 * DAY + index * 9 * HOUR,
    read: true,
    body: L(
      "Archivierte Nachricht aus einem abgeschlossenen Projekt.",
      "Archived message from a finished project.",
    ),
  }));
}

const allMockMessages = [...mockMessages, ...archiveFiller(1500)];

function buildMessages(): Message[] {
  const now = Date.now();
  return allMockMessages.map((m, index) => ({
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
    // Large mail arrives without its body; the reader loads it when opened.
    bodyText: m.large ? null : m.body,
    bodyComplete: !m.large,
    bodyHtml: null,
    htmlStyled: m.html?.styled ?? false,
    remoteImageCount: m.html?.images?.length ?? 0,
    attachments: (m.files ?? []).map((file, n) => ({
      id: `att-${index + 1}-${n + 1}`,
      fileName: file.name,
      mimeType: file.type,
      size: file.size,
      canOpen: !/\.(exe|bat|cmd|ps1|sh|desktop|appimage|docm|xlsm)$/i.test(file.name),
    })),
  }));
}

const messages = buildMessages();

function buildFolders(): Folder[] {
  const withRoles = accounts.flatMap((account) =>
    folderRoles.map(({ role, name }) => {
      const folder = roleFolder(account.id, role, name);
      const inFolder = messages.filter((m) => m.folderId === folder.id);
      return {
        ...folder,
        unreadCount: inFolder.filter((m) => !m.isRead).length,
        totalCount: inFolder.length,
      };
    }),
  );
  const plain = workFolders.map((path, index) => {
    const pathNames = path.split("/");
    return {
      id: `acc-work:folder-${index + 1}`,
      accountId: "acc-work",
      name: pathNames[pathNames.length - 1] ?? path,
      path,
      pathNames,
      role: null,
      unreadCount: path.endsWith("/Berger") ? 2 : 0,
      totalCount: 0,
    };
  });
  return [...withRoles, ...plain];
}

let folders = buildFolders();

function toSummary({
  to: _to,
  cc: _cc,
  bodyText: _body,
  bodyComplete: _complete,
  bodyHtml: _html,
  htmlStyled: _styled,
  remoteImageCount: _images,
  attachments: _attachments,
  ...summary
}: Message): MessageSummary {
  return summary;
}

function setMockFlag(messageIds: string[], flag: "isRead" | "isFlagged", value: boolean) {
  const accountIds = new Set<string>();
  for (const id of messageIds) {
    const index = messages.findIndex((m) => m.id === id);
    const message = messages[index];
    if (!message || message[flag] === value) continue;
    messages[index] = { ...message, [flag]: value };
    accountIds.add(message.accountId);
    if (flag === "isRead") {
      folders = folders.map((f) =>
        f.id === message.folderId
          ? { ...f, unreadCount: Math.max(f.unreadCount + (value ? -1 : 1), 0) }
          : f,
      );
    }
  }
  return [...accountIds];
}

/** Where moved sample mail came from, for undo; sample mail deleted for good. */
const movedFrom = new Map<string, string>();
const deletedForGood = new Set<string>();

const isListed = (message: Message) => !deletedForGood.has(message.id);

/** Role folders count their sample mail; the plain folders keep their made-up numbers. */
function recount() {
  folders = folders.map((f) => {
    if (!f.role) return f;
    const inFolder = messages.filter((m) => m.folderId === f.id && isListed(m));
    return {
      ...f,
      unreadCount: inFolder.filter((m) => !m.isRead).length,
      totalCount: inFolder.length,
    };
  });
}

/** Sample mail moves at once; `target` says where to, null for deleting it for good. */
function mockMove(
  messageIds: string[],
  target: (message: Message) => string | null | undefined,
): MoveOutcome {
  const outcome: MoveOutcome = { moved: [], deleted: [], accounts: [] };
  for (const id of messageIds) {
    const index = messages.findIndex((m) => m.id === id);
    const message = messages[index];
    if (!message || !isListed(message)) continue;
    const to = target(message);
    if (to === undefined || to === message.folderId) continue;
    if (to === null) {
      deletedForGood.add(id);
      outcome.deleted.push(id);
    } else {
      movedFrom.set(id, message.folderId);
      messages[index] = { ...message, folderId: to };
      outcome.moved.push(id);
    }
    if (!outcome.accounts.includes(message.accountId)) outcome.accounts.push(message.accountId);
  }
  recount();
  return outcome;
}

let cancelMockSignIn: () => void = () => undefined;

/** Pretends the user signs in within two seconds; cancelSignIn ends the wait. */
async function mockBrowserSignIn() {
  const signedIn = await new Promise<boolean>((resolve) => {
    const timer = setTimeout(() => resolve(true), 2000);
    cancelMockSignIn = () => {
      clearTimeout(timer);
      resolve(false);
    };
  });
  if (!signedIn) throw { kind: "signInCancelled", hint: null };
}

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

function suggest(email: string): SuggestedSettings {
  const domain = email.split("@")[1]?.toLowerCase() ?? "";
  // Sample domains: Gmail by name, Microsoft 365 for "outlook.com" and "example.com".
  const provider =
    domain === "gmail.com"
      ? "gmail"
      : domain === "outlook.com" || domain === "example.com"
        ? "microsoft"
        : null;
  const host =
    provider === "gmail"
      ? "imap.gmail.com"
      : provider === "microsoft"
        ? "outlook.office365.com"
        : `imap.${domain}`;
  return {
    imap: { host, port: 993, security: "tls", username: email },
    source: provider ? "preset" : "guess",
    provider,
    needsOauth: provider === "microsoft",
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
    accountSettings[added.id] = { signIn: "password", imap: account.imap };
    folders = [
      ...folders,
      ...folderRoles.map(({ role, name }) => roleFolder(added.id, role, name)),
    ];
    return added;
  },
  addMicrosoftAccount: async (account) => {
    await mockBrowserSignIn();
    const added = await mockBackend.addAccount(account, "");
    accountSettings[added.id] = { signIn: "microsoft", imap: account.imap };
    return added;
  },
  cancelSignIn: async () => cancelMockSignIn(),
  removeAccount: async (accountId) => {
    accounts = accounts.filter((a) => a.id !== accountId);
    folders = folders.filter((f) => f.accountId !== accountId);
  },
  getAccountSettings: async (accountId) => {
    const settings = accountSettings[accountId];
    if (!settings) throw { kind: "notFound", hint: null };
    return settings;
  },
  updateAccount: async (accountId, changes) => {
    const current = accounts.find((a) => a.id === accountId);
    if (!current) throw { kind: "notFound", hint: null };
    const updated: Account = {
      ...current,
      displayName: changes.displayName.trim() || current.emailAddress,
      color: changes.color,
    };
    accounts = accounts.map((a) => (a.id === accountId ? updated : a));
    return updated;
  },
  changeLogin: async (accountId, imap, password) => {
    await wait(900);
    if (password === "falsch") {
      throw { kind: "authenticationFailed", hint: "checkCredentials" };
    }
    const current = accountSettings[accountId];
    if (!current) throw { kind: "notFound", hint: null };
    accountSettings[accountId] = { ...current, imap };
  },
  signInAgain: async () => {
    await mockBrowserSignIn();
  },
  listFolders: async () => folders,
  listMessages: async (mailboxId) => {
    const inMailbox = (
      mailboxId === UNIFIED_INBOX
        ? messages.filter((m) => m.folderId.endsWith(":inbox"))
        : messages.filter((m) => m.folderId === mailboxId)
    ).filter(isListed);
    return inMailbox.map(toSummary).sort((a, b) => b.dateSent - a.dateSent);
  },
  getMessage: async (messageId, remoteImages) => {
    const index = messages.findIndex((m) => m.id === messageId);
    const message = messages[index];
    if (!message) return null;
    const html = allMockMessages[index]?.html;
    return {
      ...message,
      bodyHtml: html && message.bodyComplete ? renderMockHtml(html, remoteImages) : null,
    };
  },
  searchMessages: async (query) => {
    const words = query.toLocaleLowerCase(locale).split(/\s+/).filter(Boolean);
    if (words.length === 0) return [];
    return messages
      .filter(isListed)
      .filter((m) => {
        const text = [m.subject, m.from.name, m.from.address, m.bodyText]
          .join(" ")
          .toLocaleLowerCase(locale);
        return words.every((word) => text.includes(word));
      })
      .map(toSummary)
      .sort((a, b) => b.dateSent - a.dateSent)
      .slice(0, 300);
  },
  setRead: async (messageIds, read) => setMockFlag(messageIds, "isRead", read),
  setFlagged: async (messageIds, flagged) => setMockFlag(messageIds, "isFlagged", flagged),
  pushActions: async () => 0,
  moveMessages: async (messageIds, folderId) => {
    const accountId = folders.find((f) => f.id === folderId)?.accountId;
    return mockMove(messageIds, (m) => (m.accountId === accountId ? folderId : undefined));
  },
  archiveMessages: async (messageIds) =>
    mockMove(messageIds, (m) => folderId(m.accountId, "archive")),
  deleteMessages: async (messageIds) =>
    mockMove(messageIds, (m) =>
      m.folderId === folderId(m.accountId, "trash") ? null : folderId(m.accountId, "trash"),
    ),
  undoMoves: async (messageIds) => {
    const restored = messageIds.filter((id) => {
      const from = movedFrom.get(id);
      const index = messages.findIndex((m) => m.id === id);
      const message = messages[index];
      if (!from || !message) return false;
      messages[index] = { ...message, folderId: from };
      movedFrom.delete(id);
      return true;
    });
    recount();
    return restored;
  },
  saveAttachment: async (attachmentId) => {
    await wait(500);
    console.info("save attachment", attachmentId);
    return true;
  },
  openAttachment: async (attachmentId) => {
    await wait(700);
    console.info("open attachment", attachmentId);
  },
  openLink: async (url) => console.info("open link", url),
  onMailLink: async () => () => undefined,
  loadMessageBody: async (messageId) => {
    await wait(900);
    const index = messages.findIndex((m) => m.id === messageId);
    const current = messages[index];
    const sample = mockMessages[index];
    if (!current || !sample) throw { kind: "notFound", hint: null };
    messages[index] = { ...current, bodyText: sample.body, bodyComplete: true };
  },
  // Sample data is always "in sync".
  syncAccount: async () => ({ added: 0, updated: 0, removed: 0 }),
  syncFolder: async () => ({ added: 0, updated: 0, removed: 0 }),
  loadOlder: async () => ({ added: 0, updated: 0, removed: 0 }),
  onSyncProgress: async () => () => undefined,
};
