/**
 * Every icon in the app goes through here (Lucide), so the icon set can be replaced in one place.
 */
import {
  Archive,
  ArrowLeft,
  Check,
  CircleAlert,
  FilePen,
  Folder,
  Inbox,
  type LucideIcon,
  Mail,
  MailOpen,
  Mails,
  Monitor,
  Moon,
  OctagonAlert,
  Palette,
  PanelLeft,
  Paperclip,
  Plus,
  Search,
  Send,
  Settings,
  Star,
  Sun,
  Trash2,
} from "lucide-react";

import { cn } from "@/lib/utils";

const icons = {
  alert: CircleAlert,
  archive: Archive,
  back: ArrowLeft,
  check: Check,
  drafts: FilePen,
  folder: Folder,
  inbox: Inbox,
  junk: OctagonAlert,
  mail: Mail,
  mailOpen: MailOpen,
  monitor: Monitor,
  moon: Moon,
  palette: Palette,
  paperclip: Paperclip,
  plus: Plus,
  search: Search,
  sent: Send,
  settings: Settings,
  sidebar: PanelLeft,
  star: Star,
  sun: Sun,
  trash: Trash2,
  unified: Mails,
} satisfies Record<string, LucideIcon>;

export type IconName = keyof typeof icons;

interface IconProps {
  name: IconName;
  /** Filled shape, e.g. for a set star. */
  filled?: boolean;
  className?: string;
}

export function Icon({ name, filled = false, className }: IconProps) {
  const Glyph = icons[name];
  return <Glyph aria-hidden className={cn(filled && "fill-current", className)} />;
}
