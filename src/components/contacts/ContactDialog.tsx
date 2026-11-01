import { useState } from "react";

import { Icon } from "@/components/common/Icon";
import { Notice } from "@/components/common/Notice";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { addressBookName, birthdayOf, birthdayParts, labelChoices, labelKey } from "@/lib/contacts";
import { describeError } from "@/lib/errors";
import { t } from "@/lib/i18n";
import type { ContactInput, ContactLabel, Labeled, PostalAddress } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useAccounts } from "@/queries/accounts";
import { useAddressBooks, useSaveContact } from "@/queries/contacts";
import { useContactsStore } from "@/stores/contacts";

const fieldClass =
  "h-8 w-full min-w-0 rounded-lg border border-input bg-transparent px-2.5 text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 dark:bg-input/30";

export function ContactDialog() {
  const editing = useContactsStore((s) => s.editing);
  const closeDialog = useContactsStore((s) => s.closeDialog);
  return (
    <Dialog open={editing !== null} onOpenChange={(open) => !open && closeDialog()}>
      <DialogContent aria-describedby={undefined} className="gap-0 p-0 sm:max-w-lg">
        {editing && <ContactForm key={editing.id ?? "new"} initial={editing} />}
      </DialogContent>
    </Dialog>
  );
}

function LabelSelect({
  kind,
  value,
  onChange,
}: {
  kind: keyof typeof labelChoices;
  value: ContactLabel;
  onChange: (label: ContactLabel) => void;
}) {
  const choices = labelChoices[kind].includes(value)
    ? labelChoices[kind]
    : [...labelChoices[kind], value];
  return (
    <select
      aria-label={t("contacts.field.label")}
      className={cn(fieldClass, "w-28 shrink-0")}
      value={value}
      onChange={(input) => onChange(input.target.value as ContactLabel)}
    >
      {choices.map((label) => (
        <option key={label} value={label}>
          {t(labelKey(label))}
        </option>
      ))}
    </select>
  );
}

function RemoveButton({ onClick }: { onClick: () => void }) {
  return (
    <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      aria-label={t("contacts.remove")}
      title={t("contacts.remove")}
      onClick={onClick}
    >
      <Icon name="close" />
    </Button>
  );
}

function AddButton({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex items-center gap-1.5 justify-self-start rounded-md px-1 py-0.5 text-xs text-muted-foreground hover:text-foreground"
    >
      <Icon name="plus" className="size-3.5" />
      {label}
    </button>
  );
}

/** Mail addresses or numbers, each with what it is. */
function LabeledList({
  kind,
  values,
  onChange,
  placeholder,
  addLabel,
  type,
}: {
  kind: "email" | "phone";
  values: Labeled[];
  onChange: (values: Labeled[]) => void;
  placeholder: string;
  addLabel: string;
  type: "email" | "tel";
}) {
  // One added by a click gets the cursor; the ones there before leave it where it is.
  const [added, setAdded] = useState(false);
  const change = (index: number, changes: Partial<Labeled>) =>
    onChange(values.map((value, i) => (i === index ? { ...value, ...changes } : value)));
  return (
    <div className="grid gap-2">
      {values.map((value, index) => (
        <div key={index} className="flex items-center gap-2">
          <LabelSelect
            kind={kind}
            value={value.label}
            onChange={(label) => change(index, { label })}
          />
          <Input
            type={type}
            aria-label={placeholder}
            placeholder={placeholder}
            autoFocus={added && index === values.length - 1}
            value={value.value}
            onChange={(input) => change(index, { value: input.target.value })}
          />
          <RemoveButton onClick={() => onChange(values.filter((_, i) => i !== index))} />
        </div>
      ))}
      <AddButton
        label={addLabel}
        onClick={() => {
          setAdded(true);
          onChange([...values, { label: labelChoices[kind][0] ?? "other", value: "" }]);
        }}
      />
    </div>
  );
}

function AddressFields({
  address,
  autoFocus,
  onChange,
  onRemove,
}: {
  address: PostalAddress;
  /** Added by a click: the street gets the cursor. */
  autoFocus: boolean;
  onChange: (changes: Partial<PostalAddress>) => void;
  onRemove: () => void;
}) {
  return (
    <div className="grid gap-2 rounded-lg border p-2">
      <div className="flex items-center gap-2">
        <LabelSelect
          kind="address"
          value={address.label}
          onChange={(label) => onChange({ label })}
        />
        <span className="flex-1" />
        <RemoveButton onClick={onRemove} />
      </div>
      <textarea
        rows={2}
        autoFocus={autoFocus}
        aria-label={t("contacts.field.street")}
        placeholder={t("contacts.field.street")}
        value={address.street}
        onChange={(input) => onChange({ street: input.target.value })}
        className="w-full resize-none rounded-lg border border-input bg-transparent px-2.5 py-1.5 text-sm outline-none placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 dark:bg-input/30"
      />
      <div className="grid grid-cols-[6rem_1fr] gap-2">
        <Input
          aria-label={t("contacts.field.postalCode")}
          placeholder={t("contacts.field.postalCode")}
          value={address.postalCode}
          onChange={(input) => onChange({ postalCode: input.target.value })}
        />
        <Input
          aria-label={t("contacts.field.city")}
          placeholder={t("contacts.field.city")}
          value={address.city}
          onChange={(input) => onChange({ city: input.target.value })}
        />
      </div>
      <div className="grid grid-cols-2 gap-2">
        <Input
          aria-label={t("contacts.field.region")}
          placeholder={t("contacts.field.region")}
          value={address.region}
          onChange={(input) => onChange({ region: input.target.value })}
        />
        <Input
          aria-label={t("contacts.field.country")}
          placeholder={t("contacts.field.country")}
          value={address.country}
          onChange={(input) => onChange({ country: input.target.value })}
        />
      </div>
    </div>
  );
}

function ContactForm({ initial }: { initial: ContactInput }) {
  const { data: books = [] } = useAddressBooks();
  const { data: accounts = [] } = useAccounts();
  const closeDialog = useContactsStore((s) => s.closeDialog);
  const openContact = useContactsStore((s) => s.openContact);
  const save = useSaveContact();
  const [contact, setContact] = useState(initial);
  const [birthday, setBirthday] = useState(() => birthdayParts(initial.birthday));
  const [addedAddress, setAddedAddress] = useState(false);
  const change = (changes: Partial<ContactInput>) => setContact((c) => ({ ...c, ...changes }));
  const writable = books.filter((book) => !book.readOnly);

  const [day, month, year] = birthday;
  const noBirthday = !day.trim() && !month.trim() && !year.trim();
  const birthdayValue = noBirthday ? null : birthdayOf(day, month, year);
  const badBirthday = !noBirthday && birthdayValue === null;
  const nameless =
    !contact.givenName.trim() &&
    !contact.familyName.trim() &&
    !contact.company.trim() &&
    contact.emails.every((email) => !email.value.trim());

  const bookLabel = (bookId: string) => {
    const book = books.find((b) => b.id === bookId);
    if (!book) return "";
    const account = accounts.find((a) => a.id === book.accountId);
    return account
      ? `${account.displayName} · ${addressBookName(book)}`
      : `${t("calendar.thisComputer")} · ${addressBookName(book)}`;
  };

  return (
    <form
      className="grid max-h-[85vh] gap-4 overflow-y-auto p-5"
      onSubmit={(submit) => {
        submit.preventDefault();
        if (nameless || badBirthday) return;
        save.mutate(
          { ...contact, birthday: birthdayValue },
          {
            onSuccess: (id) => {
              openContact(id);
              closeDialog();
            },
          },
        );
      }}
    >
      <DialogTitle>{contact.id ? t("contacts.edit") : t("contacts.new")}</DialogTitle>
      <div className="grid grid-cols-2 gap-2">
        <Input
          data-autofocus=""
          aria-label={t("contacts.field.givenName")}
          placeholder={t("contacts.field.givenName")}
          value={contact.givenName}
          onChange={(input) => change({ givenName: input.target.value })}
        />
        <Input
          aria-label={t("contacts.field.familyName")}
          placeholder={t("contacts.field.familyName")}
          value={contact.familyName}
          onChange={(input) => change({ familyName: input.target.value })}
        />
      </div>
      <Input
        aria-label={t("contacts.field.company")}
        placeholder={t("contacts.field.company")}
        value={contact.company}
        onChange={(input) => change({ company: input.target.value })}
      />

      <LabeledList
        kind="email"
        type="email"
        values={contact.emails}
        onChange={(emails) => change({ emails })}
        placeholder={t("contacts.field.email")}
        addLabel={t("contacts.addEmail")}
      />
      <LabeledList
        kind="phone"
        type="tel"
        values={contact.phones}
        onChange={(phones) => change({ phones })}
        placeholder={t("contacts.field.phone")}
        addLabel={t("contacts.addPhone")}
      />
      <div className="grid gap-2">
        {contact.addresses.map((address, index) => (
          <AddressFields
            key={index}
            address={address}
            autoFocus={addedAddress && index === contact.addresses.length - 1}
            onChange={(changes) =>
              change({
                addresses: contact.addresses.map((a, i) =>
                  i === index ? { ...a, ...changes } : a,
                ),
              })
            }
            onRemove={() => change({ addresses: contact.addresses.filter((_, i) => i !== index) })}
          />
        ))}
        <AddButton
          label={t("contacts.addAddress")}
          onClick={() => {
            setAddedAddress(true);
            change({
              addresses: [
                ...contact.addresses,
                { label: "home", street: "", postalCode: "", city: "", region: "", country: "" },
              ],
            });
          }}
        />
      </div>

      <fieldset className="grid gap-1.5">
        <legend className="mb-1.5 text-xs text-muted-foreground">
          {t("contacts.field.birthday")}
        </legend>
        <div className="grid grid-cols-[4rem_4rem_6rem] gap-2">
          {(
            [
              ["day", 0],
              ["month", 1],
              ["year", 2],
            ] as const
          ).map(([part, index]) => (
            <Input
              key={part}
              inputMode="numeric"
              aria-label={t(`contacts.field.${part}`)}
              placeholder={t(`contacts.field.${part}`)}
              aria-invalid={badBirthday}
              value={birthday[index]}
              onChange={(input) =>
                setBirthday((parts) => {
                  const next: [string, string, string] = [...parts];
                  next[index] = input.target.value;
                  return next;
                })
              }
            />
          ))}
        </div>
        {badBirthday && <p className="text-xs text-destructive">{t("contacts.badBirthday")}</p>}
      </fieldset>

      <textarea
        aria-label={t("contacts.field.note")}
        placeholder={t("contacts.field.note")}
        rows={3}
        value={contact.note}
        onChange={(input) => change({ note: input.target.value })}
        className="min-h-20 w-full resize-y rounded-lg border border-input bg-transparent px-2.5 py-1.5 text-sm outline-none placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 dark:bg-input/30"
      />

      <label className="grid gap-1.5 text-xs text-muted-foreground">
        {t("contacts.field.addressBook")}
        <select
          className={fieldClass}
          value={contact.addressBookId}
          onChange={(input) => change({ addressBookId: input.target.value })}
        >
          {writable.map((book) => (
            <option key={book.id} value={book.id}>
              {bookLabel(book.id)}
            </option>
          ))}
        </select>
      </label>

      {save.isError && (
        <Notice
          tone="error"
          text={describeError(save.error).message}
          detail={describeError(save.error).action}
        />
      )}
      <div className="flex items-center justify-end gap-2 border-t pt-4">
        {nameless && (
          <p className="mr-auto text-xs text-muted-foreground">{t("contacts.needsName")}</p>
        )}
        <Button type="button" variant="ghost" size="sm" onClick={closeDialog}>
          {t("account.add.cancel")}
        </Button>
        <Button type="submit" size="sm" disabled={nameless || badBirthday || save.isPending}>
          {save.isPending && <Icon name="spinner" className="animate-spin" />}
          {t("calendar.save")}
        </Button>
      </div>
    </form>
  );
}
