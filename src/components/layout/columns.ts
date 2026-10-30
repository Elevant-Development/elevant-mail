/**
 * Column widths shared by the title bar and the panels below it, so the logo lines up with the
 * sidebar and the search field with the message list. A width the user dragged a column to
 * comes in as a CSS variable (AppShell); without one the default applies.
 */
export const sidebarWidth = "w-[var(--sidebar-width,15rem)]";
export const listWidth = "w-[var(--list-width,clamp(320px,32vw,440px))]";

/** How narrow and wide a dragged column may get; the reader keeps at least `readerMin`. */
export const columnLimits = {
  sidebar: { min: 180, max: 400 },
  list: { min: 280, max: 720 },
  readerMin: 360,
} as const;
