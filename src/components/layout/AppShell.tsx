import { type CSSProperties, useEffect } from "react";

import { AccountDialog } from "@/components/accounts/AccountDialog";
import { SettingsDialog } from "@/components/settings/SettingsDialog";
import { WelcomeScreen } from "@/components/accounts/WelcomeScreen";
import { Composer } from "@/components/compose/Composer";
import { DeleteForeverDialog } from "@/components/mail/DeleteForeverDialog";
import { MessageList } from "@/components/mail/MessageList";
import { MessageReader } from "@/components/mail/MessageReader";
import { DragLabel } from "@/components/mail/DragLabel";
import { MoveDialog } from "@/components/mail/MoveDialog";
import { UndoToast } from "@/components/mail/UndoToast";
import { UpdateCheckIntro } from "@/components/updates/UpdateNotice";
import { useBackground } from "@/hooks/use-background";
import { useDraftLeaving, useKeptDrafts } from "@/hooks/use-compose";
import { useMailKeyboard } from "@/hooks/use-mail-keyboard";
import { useMediaQuery } from "@/hooks/use-media-query";
import { useStartMailbox } from "@/hooks/use-start-mailbox";
import { cn } from "@/lib/utils";
import { useAccounts } from "@/queries/accounts";
import { useUiStore } from "@/stores/ui";

import { ColumnResizer } from "./ColumnResizer";
import { columnLimits, listWidth, sidebarWidth } from "./columns";
import { Sidebar } from "./Sidebar";
import { TitleBar } from "./TitleBar";

/** List and reader float as rounded panels on the canvas; the sidebar sits on the canvas. */
const panel = "overflow-hidden rounded-2xl border bg-card shadow-[0_1px_3px_var(--shadow-color)]";

/**
 * Wide windows: sidebar, list and reader side by side.
 * Narrow windows: list or reader, the sidebar opens as an overlay.
 */
export function AppShell() {
  const isWide = useMediaQuery("(min-width: 1024px)");
  const { data: accounts } = useAccounts();
  const firstStart = accounts !== undefined && accounts.length === 0;
  const selectedMessageId = useUiStore((s) => s.selectedMessageId);
  const selectMessage = useUiStore((s) => s.selectMessage);
  const composing = useUiStore((s) => s.composeKey !== null);
  const closeCompose = useUiStore((s) => s.closeCompose);
  const sidebarOpen = useUiStore((s) => s.sidebarOpen);
  const setSidebarOpen = useUiStore((s) => s.setSidebarOpen);
  const columnWidths = useUiStore((s) => s.columnWidths);
  // Read by the column classes of the panels and the title bar (columns.ts).
  const widths = {
    ...(columnWidths.sidebar !== null && { "--sidebar-width": `${columnWidths.sidebar}px` }),
    // In a window made narrower since, the reader keeps its room.
    ...(columnWidths.list !== null && {
      "--list-width": `max(${columnLimits.list.min}px, min(${columnWidths.list}px, calc(100vw - var(--sidebar-width, 15rem) - ${columnLimits.readerMin + 40}px)))`,
    }),
  } as CSSProperties;

  const showOverlay = !isWide && sidebarOpen;
  useMailKeyboard({ enabled: !firstStart && !showOverlay, isWide });
  useStartMailbox();
  useKeptDrafts();
  useDraftLeaving();
  useBackground();

  useEffect(() => {
    if (!showOverlay) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setSidebarOpen(false);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [showOverlay, setSidebarOpen]);

  return (
    <div className="flex h-full flex-col bg-canvas" style={widths}>
      <TitleBar isWide={isWide} />
      <div className="flex min-h-0 flex-1 gap-2 px-2 pb-2">
        {firstStart ? (
          <WelcomeScreen />
        ) : isWide ? (
          <>
            <Sidebar className={cn("shrink-0", sidebarWidth)} />
            <ColumnResizer column="sidebar" />
            <MessageList className={cn("shrink-0", listWidth, panel)} />
            <ColumnResizer column="list" />
            {composing ? (
              <Composer className={cn("flex-1", panel)} />
            ) : (
              <MessageReader className={cn("flex-1", panel)} />
            )}
          </>
        ) : composing ? (
          <Composer className={cn("flex-1", panel)} onBack={closeCompose} />
        ) : selectedMessageId ? (
          <MessageReader className={cn("flex-1", panel)} onBack={() => selectMessage(null)} />
        ) : (
          <MessageList className={cn("flex-1", panel)} />
        )}
      </div>

      {showOverlay && (
        <div className="fixed inset-0 z-40 flex">
          <div
            aria-hidden
            className="absolute inset-0 bg-black/40"
            onClick={() => setSidebarOpen(false)}
          />
          <Sidebar className="relative w-72 max-w-[85vw] border-r border-sidebar-border shadow-xl" />
        </div>
      )}
      <AccountDialog />
      <SettingsDialog />
      <MoveDialog />
      <DragLabel />
      <DeleteForeverDialog />
      <UndoToast />
      <UpdateCheckIntro />
    </div>
  );
}
