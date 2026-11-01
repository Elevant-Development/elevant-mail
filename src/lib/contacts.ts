/** Helpers for the contacts area: names, letters, new contacts and what the dialog edits. */
import { type MessageKey, t } from "./i18n";
import type { AddressBook, ContactDetails, ContactInput, ContactLabel } from "./tauri";

/** The address book on this computer and an account's default one have no name of their own. */
export const addressBookName = (book: AddressBook) =>
  book.name || (book.accountId ? t("contacts.default") : t("contacts.local"));

/** Lower case without accents, as the backend sorts and searches ("Müller" → "muller"). */
export function fold(text: string): string {
  return text
    .toLowerCase()
    .replaceAll("ß", "ss")
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "");
}

/** The letter a contact is listed under; "#" for names that start with something else. */
export function letterOf(name: string): string {
  const first = fold(name.trim()).charAt(0).toUpperCase();
  return /^[A-Z]$/.test(first) ? first : "#";
}

/** Up to two letters for the round picture: "Anna Müller" → "AM", "Stadtwerke" → "S". */
export function initials(name: string): string {
  const words = name
    .split(/[\s@.-]+/)
    .map((word) => word.replace(/[^\p{L}\p{N}]/gu, ""))
    .filter((word) => word.length > 0);
  const letters = words.length > 1 ? [words[0], words.at(-1)] : words.slice(0, 1);
  return letters.map((word) => word?.charAt(0).toUpperCase() ?? "").join("");
}

/** Where new contacts go: the first address book that can be written, the one here last. */
export function defaultAddressBook(
  books: AddressBook[],
  shown: string | null,
): AddressBook | undefined {
  const writable = books.filter((book) => !book.readOnly);
  return writable.find((book) => book.id === shown) ?? writable[0];
}

export function newContact(addressBookId: string): ContactInput {
  return {
    id: null,
    addressBookId,
    givenName: "",
    familyName: "",
    company: "",
    emails: [{ label: "work", value: "" }],
    phones: [],
    addresses: [],
    birthday: null,
    note: "",
  };
}

export function toInput(contact: ContactDetails): ContactInput {
  return {
    id: contact.id,
    addressBookId: contact.addressBookId,
    givenName: contact.givenName,
    familyName: contact.familyName,
    company: contact.company,
    emails: contact.emails,
    phones: contact.phones,
    addresses: contact.addresses,
    birthday: contact.birthday,
    note: contact.note,
  };
}

/** The labels the dialog offers for mail addresses, numbers and postal addresses. */
export const labelChoices: Record<"email" | "phone" | "address", ContactLabel[]> = {
  email: ["work", "home", "other"],
  phone: ["mobile", "work", "home", "other"],
  address: ["work", "home", "other"],
};

export const labelKey = (label: ContactLabel): MessageKey => `contacts.label.${label}`;

/** A birthday as `YYYY-MM-DD` or `--MM-DD` from the dialog's day, month and (optional) year. */
export function birthdayOf(day: string, month: string, year: string): string | null {
  const d = Number(day);
  const m = Number(month);
  if (!Number.isInteger(d) || !Number.isInteger(m) || d < 1 || d > 31 || m < 1 || m > 12) {
    return null;
  }
  const pad = (value: number) => String(value).padStart(2, "0");
  const y = year.trim();
  if (y === "") return `--${pad(m)}-${pad(d)}`;
  if (!/^\d{4}$/.test(y)) return null;
  return `${y}-${pad(m)}-${pad(d)}`;
}

/** Day, month and year (empty without one) of a birthday. */
export function birthdayParts(birthday: string | null): [string, string, string] {
  const match = birthday?.match(/^(\d{4}|-)-(\d{2})-(\d{2})$/);
  if (!match) return ["", "", ""];
  const [, year = "-", month = "", day = ""] = match;
  return [String(Number(day)), String(Number(month)), year === "-" ? "" : year];
}

/** "12. April 1985", or "12. April" without a year. */
export function formatBirthday(birthday: string, locale: string): string {
  const [day, month, year] = birthdayParts(birthday);
  if (!day) return birthday;
  const date = new Date(Date.UTC(year ? Number(year) : 2000, Number(month) - 1, Number(day)));
  return new Intl.DateTimeFormat(locale, {
    day: "numeric",
    month: "long",
    ...(year && { year: "numeric" }),
    timeZone: "UTC",
  }).format(date);
}
