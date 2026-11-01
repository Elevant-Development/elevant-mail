/**
 * Sample address books and contacts for `pnpm dev` (see mock.ts): the private account's address
 * book, the work account's with a read-only company directory, and the one on this computer.
 */
import { fold } from "./contacts";
import type {
  Address,
  AddressBook,
  Backend,
  ContactDetails,
  ContactInput,
  ContactSummary,
  Labeled,
} from "./tauri";

const addressBooks: AddressBook[] = [
  { id: "book-private", accountId: "acc-private", name: "Kontakte", readOnly: false },
  { id: "book-work", accountId: "acc-work", name: "Kontakte", readOnly: false },
  { id: "book-directory", accountId: "acc-work", name: "Firmenverzeichnis", readOnly: true },
  { id: "local", accountId: null, name: "", readOnly: false },
];

type MockContact = Omit<ContactDetails, "name" | "readOnly">;

const mail = (value: string, label: Labeled["label"] = "work"): Labeled => ({ label, value });

function person(
  id: string,
  addressBookId: string,
  givenName: string,
  familyName: string,
  more: Partial<MockContact> = {},
): MockContact {
  return {
    id,
    addressBookId,
    givenName,
    familyName,
    company: "",
    emails: [],
    phones: [],
    addresses: [],
    birthday: null,
    note: "",
    ...more,
  };
}

let contacts: MockContact[] = [
  person("c-1", "book-work", "Jonas", "Weber", {
    company: "Elevant GmbH",
    emails: [mail("jonas@elevant.dev")],
    phones: [{ label: "mobile", value: "+49 170 1234567" }],
    addresses: [
      {
        label: "work",
        street: "Kaiser-Joseph-Straße 12",
        postalCode: "79098",
        city: "Freiburg",
        region: "",
        country: "Deutschland",
      },
    ],
  }),
  person("c-2", "book-work", "Mira", "Schulz", {
    company: "Elevant GmbH",
    emails: [mail("mira@elevant.dev"), mail("mira.schulz@example.org", "home")],
    phones: [{ label: "work", value: "+49 761 400 22" }],
    birthday: "1990-03-14",
  }),
  person("c-3", "book-private", "Anna", "Müller", {
    emails: [mail("anna.mueller@example.org", "home")],
    phones: [{ label: "mobile", value: "+49 151 2345678" }],
    birthday: "--07-31",
    note: "Kennt sich mit Segelbooten aus.",
  }),
  person("c-4", "book-private", "Ümit", "Kaya", {
    emails: [mail("uemit@example.com", "home")],
  }),
  person("c-5", "book-private", "Lena", "Fischer", {
    emails: [mail("lena.fischer@example.net", "home")],
    phones: [{ label: "home", value: "+49 761 55 66 77" }],
    addresses: [
      {
        label: "home",
        street: "Wiesenweg 4",
        postalCode: "79100",
        city: "Freiburg",
        region: "",
        country: "",
      },
    ],
    birthday: "1992-11-01",
  }),
  person("c-6", "book-directory", "Thomas", "Brandt", {
    company: "Elevant GmbH",
    emails: [mail("thomas.brandt@elevant.dev")],
    phones: [{ label: "work", value: "+49 761 400 10" }],
  }),
  person("c-7", "book-directory", "Sophie", "Wagner", {
    company: "Elevant GmbH",
    emails: [mail("sophie.wagner@elevant.dev")],
  }),
  person("c-8", "local", "", "", {
    company: "Stadtwerke Freiburg",
    emails: [mail("service@stadtwerke.example")],
    phones: [{ label: "work", value: "0800 123 456" }],
  }),
  person("c-9", "local", "Paul", "Becker", {
    emails: [mail("paul.becker@example.org", "home")],
  }),
];
let nextId = 100;

function nameOf(contact: MockContact): string {
  const parts = [contact.givenName, contact.familyName].filter(Boolean).join(" ");
  return parts || contact.company || contact.emails[0]?.value || "";
}

function details(contact: MockContact): ContactDetails {
  return {
    ...contact,
    name: nameOf(contact),
    readOnly: addressBooks.find((book) => book.id === contact.addressBookId)?.readOnly ?? false,
  };
}

/** Contacts' addresses whose name, company or address contains `query`, as the backend offers
 * them first while typing a recipient. */
export function contactSuggestions(query: string): Address[] {
  const needle = fold(query.trim());
  if (!needle) return [];
  return contacts.flatMap((contact) =>
    fold(`${nameOf(contact)} ${contact.company}`).includes(needle) ||
    contact.emails.some((email) => email.value.toLowerCase().includes(needle))
      ? contact.emails.map((email) => ({ name: nameOf(contact), address: email.value }))
      : [],
  );
}

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

function writable(addressBookId: string) {
  const book = addressBooks.find((b) => b.id === addressBookId);
  if (!book) throw { kind: "notFound", hint: null };
  if (book.readOnly) throw { kind: "invalidInput", hint: null };
}

export const mockContacts: Pick<
  Backend,
  | "listAddressBooks"
  | "listContacts"
  | "getContact"
  | "saveContact"
  | "deleteContact"
  | "syncContacts"
> = {
  syncContacts: async () => ({ changed: false }),
  listAddressBooks: async () => addressBooks,
  listContacts: async (addressBookId, query): Promise<ContactSummary[]> => {
    const words = fold(query).split(/\s+/).filter(Boolean);
    return contacts
      .filter((contact) => addressBookId === null || contact.addressBookId === addressBookId)
      .filter((contact) => {
        const text = fold(
          [
            nameOf(contact),
            contact.company,
            ...contact.emails.map((e) => e.value),
            ...contact.phones.map((p) => p.value),
          ].join(" "),
        );
        return words.every((word) => text.includes(word));
      })
      .map((contact) => ({
        id: contact.id,
        addressBookId: contact.addressBookId,
        name: nameOf(contact),
        company: contact.company || null,
        email: contact.emails[0]?.value ?? null,
      }))
      .sort((a, b) => fold(a.name).localeCompare(fold(b.name)));
  },
  getContact: async (contactId) => {
    const contact = contacts.find((c) => c.id === contactId);
    if (!contact) throw { kind: "notFound", hint: null };
    return details(contact);
  },
  saveContact: async (input: ContactInput) => {
    await wait(250);
    writable(input.addressBookId);
    const fields = {
      addressBookId: input.addressBookId,
      givenName: input.givenName.trim(),
      familyName: input.familyName.trim(),
      company: input.company.trim(),
      emails: input.emails.filter((e) => e.value.trim()),
      phones: input.phones.filter((p) => p.value.trim()),
      addresses: input.addresses.filter((a) =>
        [a.street, a.postalCode, a.city, a.region, a.country].some((part) => part.trim()),
      ),
      birthday: input.birthday,
      note: input.note.trim(),
    };
    if (!nameOf({ ...fields, id: "" })) throw { kind: "invalidInput", hint: null };
    if (fields.emails.some((e) => !e.value.includes("@")))
      throw { kind: "invalidInput", hint: "checkEmailAddress" };
    const existing = input.id ? contacts.find((c) => c.id === input.id) : undefined;
    if (existing) {
      Object.assign(existing, fields);
      return existing.id;
    }
    const id = `c-${++nextId}`;
    contacts = [...contacts, { id, ...fields }];
    return id;
  },
  deleteContact: async (contactId) => {
    await wait(150);
    const contact = contacts.find((c) => c.id === contactId);
    if (!contact) throw { kind: "notFound", hint: null };
    writable(contact.addressBookId);
    contacts = contacts.filter((c) => c.id !== contactId);
  },
};
