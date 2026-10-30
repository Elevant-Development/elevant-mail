import { useIsMutating } from "@tanstack/react-query";
import type { ReactNode } from "react";

import { Icon } from "@/components/common/Icon";
import { AppearanceSettings } from "@/components/layout/AppearanceSettings";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { t } from "@/lib/i18n";
import { api } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useAccounts } from "@/queries/accounts";
import { useUiStore } from "@/stores/ui";

import { AccountSettingsPanel } from "./AccountSettingsPanel";
import { NotificationSettings } from "./NotificationSettings";
import { ReadingSettings } from "./ReadingSettings";

function NavGroup({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid gap-px">
      <h3 className="px-2 pb-1 text-xs font-medium text-muted-foreground">{label}</h3>
      {children}
    </div>
  );
}

interface NavItemProps {
  selected?: boolean;
  onSelect: () => void;
  children: ReactNode;
}

function NavItem({ selected = false, onSelect, children }: NavItemProps) {
  return (
    <button
      type="button"
      aria-current={selected ? "page" : undefined}
      onClick={onSelect}
      className={cn(
        "flex h-8 w-full min-w-0 items-center gap-2 rounded-md px-2 text-left text-sm transition-colors hover:bg-sidebar-accent/60 [&_svg]:size-4 [&_svg]:shrink-0 [&_svg]:text-muted-foreground",
        selected &&
          "bg-sidebar-accent font-medium text-sidebar-accent-foreground hover:bg-sidebar-accent",
      )}
    >
      {children}
    </button>
  );
}

/** Accounts on the left, each with its own page; later more sections join them. */
export function SettingsDialog() {
  const page = useUiStore((s) => s.settingsPage);
  const openSettings = useUiStore((s) => s.openSettings);
  const closeSettings = useUiStore((s) => s.closeSettings);
  const setAccountDialogOpen = useUiStore((s) => s.setAccountDialogOpen);
  const { data: accounts = [] } = useAccounts();
  // A sign-in still waiting for the browser ends with the window.
  const signingIn = useIsMutating({ mutationKey: ["sign-in-again"] }) > 0;

  const close = () => {
    if (signingIn) void api.cancelSignIn();
    closeSettings();
  };

  // No account chosen, or one that was just removed: the first one.
  const account =
    page?.section === "accounts"
      ? (accounts.find((a) => a.id === page.accountId) ?? accounts[0] ?? null)
      : null;

  const addAccount = () => {
    close();
    setAccountDialogOpen(true);
  };

  return (
    <Dialog open={page !== null} onOpenChange={(open) => !open && close()}>
      <DialogContent
        aria-describedby={undefined}
        className="grid h-[min(40rem,calc(100dvh-2rem))] grid-cols-[9rem_1fr] gap-0 overflow-hidden p-0 sm:max-w-3xl sm:grid-cols-[13rem_1fr]"
      >
        <nav className="flex min-h-0 flex-col gap-5 overflow-y-auto border-r bg-sidebar p-2 text-sidebar-foreground">
          <DialogTitle className="px-2 pt-3">{t("settings.title")}</DialogTitle>
          <NavGroup label={t("settings.accounts")}>
            {accounts.map((a) => (
              <NavItem
                key={a.id}
                selected={a.id === account?.id}
                onSelect={() => openSettings({ section: "accounts", accountId: a.id })}
              >
                <span
                  aria-hidden
                  className="size-2 shrink-0 rounded-full"
                  style={{ backgroundColor: a.color }}
                />
                <span className="truncate">{a.displayName}</span>
              </NavItem>
            ))}
            <NavItem onSelect={addAccount}>
              <Icon name="plus" />
              <span className="truncate">{t("sidebar.addAccount")}</span>
            </NavItem>
          </NavGroup>
          <NavGroup label={t("settings.general")}>
            <NavItem
              selected={page?.section === "appearance"}
              onSelect={() => openSettings({ section: "appearance" })}
            >
              <Icon name="palette" />
              <span className="truncate">{t("appearance.open")}</span>
            </NavItem>
            <NavItem
              selected={page?.section === "reading"}
              onSelect={() => openSettings({ section: "reading" })}
            >
              <Icon name="mailOpen" />
              <span className="truncate">{t("settings.reading")}</span>
            </NavItem>
            <NavItem
              selected={page?.section === "notifications"}
              onSelect={() => openSettings({ section: "notifications" })}
            >
              <Icon name="bell" />
              <span className="truncate">{t("settings.notifications")}</span>
            </NavItem>
          </NavGroup>
        </nav>

        <div className="min-h-0 min-w-0 overflow-y-auto p-6 pt-12 sm:pt-6">
          {page?.section === "appearance" ? (
            <div className="grid max-w-md gap-6">
              <h2 className="text-base font-medium">{t("appearance.open")}</h2>
              <AppearanceSettings />
            </div>
          ) : page?.section === "reading" ? (
            <div className="grid max-w-md gap-6">
              <h2 className="text-base font-medium">{t("settings.reading")}</h2>
              <ReadingSettings />
            </div>
          ) : page?.section === "notifications" ? (
            <div className="grid max-w-md gap-6">
              <h2 className="text-base font-medium">{t("settings.notifications")}</h2>
              <NotificationSettings />
            </div>
          ) : account ? (
            <div className="max-w-md">
              {/* Keyed, so switching accounts starts with fresh fields. */}
              <AccountSettingsPanel key={account.id} account={account} />
            </div>
          ) : (
            <div className="grid justify-items-start gap-3">
              <p className="text-sm text-muted-foreground">{t("settings.noAccounts")}</p>
              <Button onClick={addAccount}>
                <Icon name="plus" />
                {t("sidebar.addAccount")}
              </Button>
            </div>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
