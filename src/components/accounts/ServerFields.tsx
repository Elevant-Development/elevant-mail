import type { ReactNode } from "react";

import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { formatSecurity } from "@/lib/formatting";
import { t } from "@/lib/i18n";
import type { ImapSettings, Security } from "@/lib/tauri";
import { cn } from "@/lib/utils";

/** Standard ports; switching the security moves the port along unless it was changed by hand. */
const defaultPorts: Record<Security, number> = { tls: 993, startTls: 143 };

export function Field({ id, label, children }: { id: string; label: string; children: ReactNode }) {
  return (
    <div className="grid gap-1.5">
      <Label htmlFor={id}>{label}</Label>
      {children}
    </div>
  );
}

interface ServerFieldsProps {
  /** Prefix for the field ids, unique on the page. */
  idPrefix: string;
  settings: ImapSettings;
  /** Read-only, not disabled, while busy: disabling would drop the keyboard focus. */
  readOnly: boolean;
  onChange: (settings: ImapSettings) => void;
}

/** IMAP server, port, encryption and user name. */
export function ServerFields({ idPrefix, settings, readOnly, onChange }: ServerFieldsProps) {
  const update = (patch: Partial<ImapSettings>) => onChange({ ...settings, ...patch });

  const changeSecurity = (security: Security) => {
    const port =
      settings.port === defaultPorts[settings.security] ? defaultPorts[security] : settings.port;
    update({ security, port });
  };

  return (
    <div className="grid gap-3">
      <div className="grid grid-cols-[1fr_6rem] gap-2">
        <Field id={`${idPrefix}-host`} label={t("account.field.host")}>
          <Input
            id={`${idPrefix}-host`}
            required
            value={settings.host}
            readOnly={readOnly}
            onChange={(event) => update({ host: event.target.value })}
          />
        </Field>
        <Field id={`${idPrefix}-port`} label={t("account.field.port")}>
          <Input
            id={`${idPrefix}-port`}
            type="number"
            min={1}
            max={65535}
            required
            value={settings.port}
            readOnly={readOnly}
            onChange={(event) => update({ port: Number(event.target.value) })}
          />
        </Field>
      </div>
      <div className="grid gap-1.5">
        <span className="text-sm font-medium">{t("account.field.security")}</span>
        <div
          role="radiogroup"
          aria-label={t("account.field.security")}
          className="grid grid-cols-2 gap-1 rounded-lg bg-muted p-1"
        >
          {(["tls", "startTls"] as const).map((security) => (
            <button
              key={security}
              type="button"
              role="radio"
              aria-checked={settings.security === security}
              disabled={readOnly}
              onClick={() => changeSecurity(security)}
              className={cn(
                "h-7 rounded-md text-xs font-medium transition-colors",
                settings.security === security
                  ? "bg-card text-foreground shadow-sm"
                  : "text-muted-foreground hover:text-foreground",
              )}
            >
              {formatSecurity(security)}
            </button>
          ))}
        </div>
      </div>
      <Field id={`${idPrefix}-username`} label={t("account.field.username")}>
        <Input
          id={`${idPrefix}-username`}
          autoComplete="username"
          required
          value={settings.username}
          readOnly={readOnly}
          onChange={(event) => update({ username: event.target.value })}
        />
      </Field>
    </div>
  );
}
