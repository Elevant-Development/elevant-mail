import { t } from "@/lib/i18n";
import type { Address } from "@/lib/tauri";

/** What the list knows of a row: its newest message and the messages behind it. */
interface Row {
  id: string;
  messageIds: string[];
}

/** The row an id stands for: the row itself, or the one whose conversation holds the message. */
export function rowFor<T extends Row>(rows: T[], id: string | null): T | undefined {
  if (id === null) return undefined;
  return rows.find((row) => row.id === id) ?? rows.find((row) => row.messageIds.includes(id));
}

/** The messages behind the rows, each once. An id the list does not know stands for itself. */
export function messagesOf(rows: Row[], ids: string[]): string[] {
  return [...new Set(ids.flatMap((id) => rowFor(rows, id)?.messageIds ?? [id]))];
}

/** "Clara Neumann" → "Clara", "Müller, Hans" → "Hans", "Dr. Jana Wolf" → "Jana". */
export function firstName(address: Address): string {
  const name = address.name?.trim();
  if (!name) return address.address.split("@")[0] ?? address.address;
  const [last, first] = name.split(",").map((part) => part.trim());
  const words = (first || last || name).split(/\s+/);
  return words.find((word) => !word.endsWith(".")) ?? name;
}

/** Who wrote in a conversation, by first name and the user as "me": "Lena, me, Jonas". */
export function senderNames(senders: Address[], ownAddresses: Set<string>): string {
  return senders
    .map((sender) =>
      ownAddresses.has(sender.address.toLowerCase()) ? t("conversation.me") : firstName(sender),
    )
    .join(", ");
}
