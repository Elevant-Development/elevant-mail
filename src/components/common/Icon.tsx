/**
 * Every icon in the app goes through here (Lucide), so the icon set can be replaced in one place.
 */
import {
  Archive,
  ArrowLeft,
  Check,
  CircleAlert,
  Copy,
  FilePen,
  Folder,
  Inbox,
  Info,
  LoaderCircle,
  type LucideIcon,
  Mail,
  MailOpen,
  Mails,
  Minus,
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
  Square,
  Star,
  Sun,
  Trash2,
  X,
} from "lucide-react";

import { cn } from "@/lib/utils";

const icons = {
  alert: CircleAlert,
  archive: Archive,
  back: ArrowLeft,
  check: Check,
  close: X,
  drafts: FilePen,
  folder: Folder,
  inbox: Inbox,
  info: Info,
  junk: OctagonAlert,
  mail: Mail,
  mailOpen: MailOpen,
  maximize: Square,
  minimize: Minus,
  monitor: Monitor,
  moon: Moon,
  palette: Palette,
  paperclip: Paperclip,
  plus: Plus,
  restore: Copy,
  search: Search,
  sent: Send,
  settings: Settings,
  sidebar: PanelLeft,
  spinner: LoaderCircle,
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
