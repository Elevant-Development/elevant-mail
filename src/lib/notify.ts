/** What a desktop notification about new mail says. */
import { displayName } from "./formatting";
import { t, tCount } from "./i18n";
import type { NewMail } from "./tauri";

export interface Notice {
  title: string;
  body: string;
}

/**
 * One account's new mail: a single message by sender and subject, several by count and senders.
 * Without `preview` only the count and the account, for screens others can see.
 */
export function newMailNotice(mail: NewMail[], accountName: string, preview: boolean): Notice {
  const [first] = mail;
  if (preview && mail.length === 1 && first) {
    return { title: displayName(first.from), body: first.subject ?? t("message.noSubject") };
  }
  const title = tCount("notify.newMail", mail.length);
  if (!preview) return { title, body: t("notify.in", { account: accountName }) };
  const names = [...new Set(mail.map((m) => displayName(m.from)))];
  const shown = names.slice(0, 3).join(", ");
  return {
    title,
    body:
      names.length > 3
        ? t("notify.fromMore", { names: shown })
        : t("notify.from", { names: shown }),
  };
}
