/**
 * Every icon in the app goes through here (Lucide), so the icon set can be replaced in one place.
 */
import {
  Archive,
  ArrowLeft,
  Bell,
  Check,
  ChevronRight,
  CircleAlert,
  Copy,
  Download,
  Ellipsis,
  File,
  FileArchive,
  FileImage,
  FileSpreadsheet,
  FileText,
  FilePen,
  Folder,
  FolderInput,
  ImageOff,
  Inbox,
  Info,
  LoaderCircle,
  LogIn,
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
  RefreshCw,
  Search,
  Send,
  Settings,
  ShieldAlert,
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
  bell: Bell,
  check: Check,
  chevron: ChevronRight,
  close: X,
  download: Download,
  drafts: FilePen,
  file: File,
  fileArchive: FileArchive,
  fileImage: FileImage,
  fileSheet: FileSpreadsheet,
  fileText: FileText,
  folder: Folder,
  imageOff: ImageOff,
  inbox: Inbox,
  info: Info,
  junk: OctagonAlert,
  mail: Mail,
  mailOpen: MailOpen,
  maximize: Square,
  more: Ellipsis,
  minimize: Minus,
  monitor: Monitor,
  move: FolderInput,
  moon: Moon,
  palette: Palette,
  paperclip: Paperclip,
  plus: Plus,
  refresh: RefreshCw,
  restore: Copy,
  search: Search,
  sent: Send,
  settings: Settings,
  shieldAlert: ShieldAlert,
  signIn: LogIn,
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
