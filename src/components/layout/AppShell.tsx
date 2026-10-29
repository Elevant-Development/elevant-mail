import { useEffect } from "react";

import { MessageList } from "@/components/mail/MessageList";
import { MessageReader } from "@/components/mail/MessageReader";
import { useMediaQuery } from "@/hooks/use-media-query";
import { useUiStore } from "@/stores/ui";

import { Sidebar } from "./Sidebar";
import { TitleBar } from "./TitleBar";

/**
 * Wide windows: sidebar, list and reader side by side.
 * Narrow windows: list or reader, the sidebar opens as an overlay.
 */
export function AppShell() {
  const isWide = useMediaQuery("(min-width: 1024px)");
  const selectedMessageId = useUiStore((s) => s.selectedMessageId);
  const selectMessage = useUiStore((s) => s.selectMessage);
  const sidebarOpen = useUiStore((s) => s.sidebarOpen);
  const setSidebarOpen = useUiStore((s) => s.setSidebarOpen);

  const showOverlay = !isWide && sidebarOpen;

  useEffect(() => {
    if (!showOverlay) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setSidebarOpen(false);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [showOverlay, setSidebarOpen]);

  return (
    <div className="flex h-full flex-col">
      <TitleBar isWide={isWide} />
      <div className="flex min-h-0 flex-1">
        {isWide ? (
          <>
            <Sidebar className="w-60 shrink-0 border-r border-sidebar-border" />
            <MessageList className="w-[clamp(320px,32vw,440px)] shrink-0 border-r" />
            <MessageReader className="flex-1" />
          </>
        ) : selectedMessageId ? (
          <MessageReader className="flex-1" onBack={() => selectMessage(null)} />
        ) : (
          <MessageList className="flex-1" />
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
    </div>
  );
}
