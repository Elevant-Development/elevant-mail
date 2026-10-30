import { Icon } from "@/components/common/Icon";
import { IconButton } from "@/components/common/IconButton";
import { Segmented } from "@/components/common/Segmented";
import { unifiedInboxLabel } from "@/lib/folders";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { useAccounts, useSetInUnifiedInbox } from "@/queries/accounts";
import { useImageSenders, useSetImageSender } from "@/queries/image-senders";
import { useUiStore } from "@/stores/ui";

/** The choices for "mark as read", by delay in ms; "never" leaves it to u and the button. */
const delays = { now: 0, second: 1000, seconds: 3000, never: null } as const;
type DelayKey = keyof typeof delays;

const heading = "mb-2 text-xs font-medium text-muted-foreground";
const hint = "mt-2 text-xs text-muted-foreground";

const row =
  "flex h-8 w-full items-center gap-2.5 rounded-md px-2 text-left text-sm transition-colors hover:bg-muted/60";

/**
 * Which accounts the unified inbox brings together, where the app opens, when an opened mail
 * counts as read, and whose images always load.
 */
export function ReadingSettings() {
  const { data: accounts = [] } = useAccounts();
  const setInUnifiedInbox = useSetInUnifiedInbox();
  const startAccountId = useUiStore((s) => s.startAccountId);
  const setStartAccount = useUiStore((s) => s.setStartAccount);
  const delay = useUiStore((s) => s.markReadDelay);
  const setDelay = useUiStore((s) => s.setMarkReadDelay);
  const { data: senders = [] } = useImageSenders();
  const setImageSender = useSetImageSender();

  // A removed account no longer opens; then all inboxes do.
  const start = accounts.some((a) => a.id === startAccountId) ? startAccountId : null;
  const delayKey =
    (Object.keys(delays) as DelayKey[]).find((key) => delays[key] === delay) ?? "second";
  const startOptions = [
    { id: null, label: unifiedInboxLabel(accounts), color: null },
    ...accounts.map((a) => ({ id: a.id, label: a.displayName, color: a.color })),
  ];

  return (
    <div className="grid gap-8">
      {/* With one account there is nothing to bring together and only its inbox to open. */}
      {accounts.length > 1 && (
        <section>
          <h3 id="settings-unified" className={heading}>
            {t("settings.reading.unified")}
          </h3>
          <div role="group" aria-labelledby="settings-unified" className="grid gap-px">
            {accounts.map((account) => {
              const included = account.inUnifiedInbox;
              // The last one stays: an empty unified inbox would show nothing.
              const last = included && accounts.filter((a) => a.inUnifiedInbox).length === 1;
              return (
                <button
                  key={account.id}
                  type="button"
                  role="checkbox"
                  aria-checked={included}
                  disabled={last}
                  onClick={() =>
                    setInUnifiedInbox.mutate({ accountId: account.id, included: !included })
                  }
                  className={cn(row, "disabled:cursor-default disabled:hover:bg-transparent")}
                >
                  <span
                    aria-hidden
                    className={cn(
                      "flex size-4 shrink-0 items-center justify-center rounded border [&_svg]:size-3",
                      included
                        ? "border-primary bg-primary text-primary-foreground"
                        : "border-input",
                      last && "opacity-60",
                    )}
                  >
                    {included && <Icon name="check" />}
                  </span>
                  <span
                    aria-hidden
                    className="size-2 shrink-0 rounded-full"
                    style={{ backgroundColor: account.color }}
                  />
                  <span className="min-w-0 flex-1 truncate">{account.displayName}</span>
                </button>
              );
            })}
          </div>
          <p className={hint}>{t("settings.reading.unifiedHint")}</p>
        </section>
      )}

      {accounts.length > 1 && (
        <section>
          <h3 id="settings-start" className={heading}>
            {t("settings.reading.start")}
          </h3>
          <div role="radiogroup" aria-labelledby="settings-start" className="grid gap-px">
            {startOptions.map((option) => (
              <button
                key={option.id ?? "all"}
                type="button"
                role="radio"
                aria-checked={start === option.id}
                onClick={() => setStartAccount(option.id)}
                className={cn(row, start === option.id && "bg-muted font-medium hover:bg-muted")}
              >
                {option.color ? (
                  <span
                    aria-hidden
                    className="mx-1 size-2 shrink-0 rounded-full"
                    style={{ backgroundColor: option.color }}
                  />
                ) : (
                  <Icon name="unified" className="size-4 shrink-0 text-muted-foreground" />
                )}
                <span className="min-w-0 flex-1 truncate">{option.label}</span>
                {start === option.id && <Icon name="check" className="size-4 text-primary" />}
              </button>
            ))}
          </div>
        </section>
      )}

      <section>
        <Segmented
          label={t("settings.reading.markRead")}
          value={delayKey}
          options={(Object.keys(delays) as DelayKey[]).map((key) => ({
            value: key,
            label: t(`settings.reading.markRead.${key}`),
          }))}
          onChange={(key) => setDelay(delays[key])}
        />
        <p className={hint}>{t("settings.reading.markReadHint")}</p>
      </section>

      <section>
        <h3 className={heading}>{t("settings.reading.images")}</h3>
        {senders.length === 0 ? (
          <p className="rounded-lg border border-dashed px-3 py-3 text-sm text-muted-foreground">
            {t("settings.reading.imagesEmpty")}
          </p>
        ) : (
          <ul className="divide-y rounded-lg border">
            {senders.map((address) => (
              <li key={address} className="flex items-center gap-2 py-1 pr-1 pl-3 text-sm">
                <span className="min-w-0 flex-1 truncate">{address}</span>
                <IconButton
                  label={t("settings.reading.imagesRemove", { address })}
                  icon={<Icon name="close" />}
                  onClick={() => setImageSender.mutate({ address, allowed: false })}
                />
              </li>
            ))}
          </ul>
        )}
        <p className={hint}>{t("settings.reading.imagesHint")}</p>
      </section>
    </div>
  );
}
