import { type ReactNode, useState } from "react";

import { Icon, type IconName } from "@/components/common/Icon";
import { Notice } from "@/components/common/Notice";
import { Button } from "@/components/ui/button";
import { useCompose } from "@/hooks/use-compose";
import { addressBookName, formatBirthday, labelKey, toInput } from "@/lib/contacts";
import { describeError } from "@/lib/errors";
import { locale, t } from "@/lib/i18n";
import type { ContactLabel } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useAddressBooks, useContact, useDeleteContact } from "@/queries/contacts";
import { useCalendarStore } from "@/stores/calendar";
import { useContactsStore } from "@/stores/contacts";

import { ContactAvatar } from "./ContactAvatar";

function Section({
  icon,
  label,
  children,
}: {
  icon: IconName;
  label: string;
  children: ReactNode;
}) {
  return (
    <section aria-label={label} className="flex items-start gap-4 border-t py-3">
      <Icon name={icon} className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
      <div className="grid min-w-0 flex-1 gap-2">{children}</div>
    </section>
  );
}

function Labeled({ label, children }: { label: ContactLabel; children: ReactNode }) {
  return (
    <div className="grid min-w-0 gap-0.5">
      <span className="text-xs text-muted-foreground">{t(labelKey(label))}</span>
      <div className="min-w-0 text-sm break-words">{children}</div>
    </div>
  );
}

/** The contact chosen in the list, with a mail to each address a click away. */
export function ContactPane({ className, onBack }: { className?: string; onBack?: () => void }) {
  const contactId = useContactsStore((s) => s.contactId);
  const openContact = useContactsStore((s) => s.openContact);
  const editContact = useContactsStore((s) => s.editContact);
  const { data: contact, isError, error } = useContact(contactId);
  const { data: books = [] } = useAddressBooks();
  const remove = useDeleteContact();
  const { compose } = useCompose();
  const setArea = useCalendarStore((s) => s.setArea);
  const [confirming, setConfirming] = useState<string | null>(null);

  if (!contactId || !contact) {
    return (
      <section className={cn("flex flex-col items-center justify-center gap-2", className)}>
        {isError ? (
          <Notice tone="error" text={describeError(error).message} />
        ) : (
          !contactId && (
            <>
              <Icon name="contacts" className="size-8 text-muted-foreground/60" />
              <p className="text-sm text-muted-foreground">{t("contacts.none")}</p>
            </>
          )
        )}
      </section>
    );
  }

  const book = books.find((b) => b.id === contact.addressBookId);
  const writeTo = (address: string) => {
    setArea("mail");
    void compose([{ name: contact.name, address }]);
  };

  return (
    <section aria-label={contact.name} className={cn("flex min-h-0 flex-col", className)}>
      <div className="flex h-12 shrink-0 items-center gap-1 border-b px-2">
        {onBack && (
          <Button variant="ghost" size="icon-sm" aria-label={t("contacts.back")} onClick={onBack}>
            <Icon name="back" />
          </Button>
        )}
        <div className="ml-auto flex items-center gap-1">
          {!contact.readOnly &&
            (confirming === contact.id ? (
              <>
                <span className="mr-1 text-sm text-muted-foreground">
                  {t("contacts.deleteQuestion")}
                </span>
                <Button variant="ghost" size="sm" onClick={() => setConfirming(null)}>
                  {t("account.add.cancel")}
                </Button>
                <Button
                  variant="destructive"
                  size="sm"
                  disabled={remove.isPending}
                  onClick={() =>
                    remove.mutate(
                      { contactId: contact.id, addressBookId: contact.addressBookId },
                      {
                        onSuccess: () => {
                          setConfirming(null);
                          openContact(null);
                        },
                      },
                    )
                  }
                >
                  <Icon name="trash" />
                  {t("calendar.delete")}
                </Button>
              </>
            ) : (
              <>
                <Button variant="ghost" size="sm" onClick={() => setConfirming(contact.id)}>
                  <Icon name="trash" />
                  {t("calendar.delete")}
                </Button>
                <Button size="sm" onClick={() => editContact(toInput(contact))}>
                  <Icon name="compose" />
                  {t("calendar.edit")}
                </Button>
              </>
            ))}
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">
        <div className="mx-auto grid max-w-xl gap-1">
          <div className="flex items-center gap-4 pb-4">
            <ContactAvatar name={contact.name} className="size-14 text-lg" />
            <div className="min-w-0">
              <h2 className="text-xl leading-tight font-semibold break-words">{contact.name}</h2>
              {contact.company && contact.company !== contact.name && (
                <p className="text-sm text-muted-foreground">{contact.company}</p>
              )}
            </div>
          </div>
          {remove.isError && <Notice tone="error" text={describeError(remove.error).message} />}
          {contact.emails.length > 0 && (
            <Section icon="mail" label={t("contacts.field.email")}>
              {contact.emails.map((email, index) => (
                <Labeled key={index} label={email.label}>
                  <button
                    type="button"
                    onClick={() => writeTo(email.value)}
                    title={t("contacts.writeMail")}
                    className="text-left text-primary hover:underline"
                  >
                    {email.value}
                  </button>
                </Labeled>
              ))}
            </Section>
          )}
          {contact.phones.length > 0 && (
            <Section icon="phone" label={t("contacts.field.phone")}>
              {contact.phones.map((phone, index) => (
                <Labeled key={index} label={phone.label}>
                  <span className="select-text">{phone.value}</span>
                </Labeled>
              ))}
            </Section>
          )}
          {contact.addresses.length > 0 && (
            <Section icon="mapPin" label={t("contacts.field.address")}>
              {contact.addresses.map((address, index) => (
                <Labeled key={index} label={address.label}>
                  <span className="whitespace-pre-line select-text">
                    {[
                      address.street,
                      [address.postalCode, address.city].filter(Boolean).join(" "),
                      address.region,
                      address.country,
                    ]
                      .filter(Boolean)
                      .join("\n")}
                  </span>
                </Labeled>
              ))}
            </Section>
          )}
          {contact.birthday && (
            <Section icon="cake" label={t("contacts.field.birthday")}>
              <span className="text-sm">{formatBirthday(contact.birthday, locale)}</span>
            </Section>
          )}
          {contact.note && (
            <Section icon="fileText" label={t("contacts.field.note")}>
              <p className="text-sm whitespace-pre-wrap text-muted-foreground select-text">
                {contact.note}
              </p>
            </Section>
          )}
          {book && (
            <Section icon="contacts" label={t("contacts.field.addressBook")}>
              <span className="text-sm text-muted-foreground">
                {addressBookName(book)}
                {book.readOnly && ` · ${t("calendar.readOnly")}`}
              </span>
            </Section>
          )}
        </div>
      </div>
    </section>
  );
}
