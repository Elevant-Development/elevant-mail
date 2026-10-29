import { useState } from "react";

import { Icon } from "@/components/common/Icon";
import { Notice } from "@/components/common/Notice";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { notificationsAllowed, showNotification } from "@/lib/desktop-notify";
import { t } from "@/lib/i18n";
import { useUiStore } from "@/stores/ui";

function Setting({
  id,
  label,
  hint,
  checked,
  disabled = false,
  onChange,
}: {
  id: string;
  label: string;
  hint: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <div className="flex items-start justify-between gap-4">
      <div className="grid gap-0.5">
        <label htmlFor={id} className="text-sm font-medium">
          {label}
        </label>
        <p id={`${id}-hint`} className="text-sm text-muted-foreground">
          {hint}
        </p>
      </div>
      <Switch
        id={id}
        aria-describedby={`${id}-hint`}
        checked={checked}
        disabled={disabled}
        onCheckedChange={onChange}
        className="mt-0.5"
      />
    </div>
  );
}

/** New mail as a desktop notification, with or without sender and subject. */
export function NotificationSettings() {
  const notifications = useUiStore((s) => s.notifications);
  const preview = useUiStore((s) => s.notificationPreview);
  const setNotifications = useUiStore((s) => s.setNotifications);
  const setPreview = useUiStore((s) => s.setNotificationPreview);
  const [denied, setDenied] = useState(false);

  // Asks the system right away, so the first new mail is not where it asks.
  const ask = async () => {
    const allowed = await notificationsAllowed();
    setDenied(!allowed);
    return allowed;
  };

  return (
    <div className="grid gap-6">
      <Setting
        id="settings-notifications"
        label={t("settings.notifications.enabled")}
        hint={t("settings.notifications.enabledHint")}
        checked={notifications}
        onChange={(on) => {
          setNotifications(on);
          if (on) void ask();
        }}
      />
      <Setting
        id="settings-notification-preview"
        label={t("settings.notifications.preview")}
        hint={t("settings.notifications.previewHint")}
        checked={preview}
        disabled={!notifications}
        onChange={setPreview}
      />
      {denied && <Notice tone="error" text={t("settings.notifications.denied")} />}
      <Button
        variant="outline"
        className="justify-self-start"
        disabled={!notifications}
        onClick={async () => {
          if (await ask()) {
            showNotification({ title: "Elevant Mail", body: t("notify.sample") });
          }
        }}
      >
        <Icon name="bell" />
        {t("settings.notifications.try")}
      </Button>
    </div>
  );
}
