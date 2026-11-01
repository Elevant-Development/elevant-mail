import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";

import { Icon } from "@/components/common/Icon";
import { Notice } from "@/components/common/Notice";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { notificationsAllowed, showNotification } from "@/lib/desktop-notify";
import { locale, t } from "@/lib/i18n";
import { reminderChoices, reminderLabel } from "@/lib/reminders";
import { api } from "@/lib/tauri";
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
  const keepRunning = useUiStore((s) => s.keepRunning);
  const setKeepRunning = useUiStore((s) => s.setKeepRunning);
  const queryClient = useQueryClient();
  const autostart = useQuery({ queryKey: ["autostart"], queryFn: api.autostartEnabled });
  const setAutostart = useMutation({
    mutationFn: api.setAutostart,
    onSuccess: (enabled) => queryClient.setQueryData(["autostart"], enabled),
  });
  const eventReminders = useUiStore((s) => s.eventReminders);
  const setEventReminders = useUiStore((s) => s.setEventReminders);
  const defaultReminder = useUiStore((s) => s.defaultReminder);
  const setDefaultReminder = useUiStore((s) => s.setDefaultReminder);
  const updateCheck = useUiStore((s) => s.updateCheck);
  const setUpdateCheck = useUiStore((s) => s.setUpdateCheck);
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
      <Setting
        id="settings-event-reminders"
        label={t("settings.notifications.reminders")}
        hint={t("settings.notifications.remindersHint")}
        checked={eventReminders}
        onChange={(on) => {
          setEventReminders(on);
          if (on) void ask();
        }}
      />
      <label className="flex items-center justify-between gap-4 text-sm">
        <span className="font-medium">{t("settings.notifications.defaultReminder")}</span>
        <select
          className="h-8 rounded-lg border border-input bg-transparent px-2.5 text-sm dark:bg-input/30"
          value={defaultReminder ?? "none"}
          onChange={(event) =>
            setDefaultReminder(event.target.value === "none" ? null : Number(event.target.value))
          }
        >
          {reminderChoices.map((minutes) => (
            <option key={minutes ?? "none"} value={minutes ?? "none"}>
              {reminderLabel(minutes, locale)}
            </option>
          ))}
        </select>
      </label>
      <Setting
        id="settings-keep-running"
        label={t("settings.notifications.keepRunning")}
        hint={t("settings.notifications.keepRunningHint")}
        checked={keepRunning}
        onChange={setKeepRunning}
      />
      <Setting
        id="settings-autostart"
        label={t("settings.notifications.autostart")}
        hint={t("settings.notifications.autostartHint")}
        checked={autostart.data ?? false}
        disabled={autostart.isPending || setAutostart.isPending}
        onChange={(on) => setAutostart.mutate(on)}
      />
      <Setting
        id="settings-update-check"
        label={t("settings.notifications.updates")}
        hint={t("settings.notifications.updatesHint")}
        checked={updateCheck}
        onChange={setUpdateCheck}
      />
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
