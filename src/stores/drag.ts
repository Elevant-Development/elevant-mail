/** Mail being dragged onto a folder in the sidebar. Not persisted. */
import { create } from "zustand";

export interface MailDrag {
  messageIds: string[];
  /** The folders the mail may go to: the other folders of its one account. */
  targets: Set<string>;
  x: number;
  y: number;
  /** The folder under the pointer, if the mail may go there. */
  overFolderId: string | null;
}

interface DragState {
  drag: MailDrag | null;
  set: (drag: MailDrag | null) => void;
}

export const useDragStore = create<DragState>()((set) => ({
  drag: null,
  set: (drag) => set({ drag }),
}));
