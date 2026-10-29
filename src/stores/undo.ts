/** The notice after archiving, deleting or moving, with the way back. Not persisted. */
import { create } from "zustand";

export interface UndoNotice {
  /** Changes with every notice, so a new one starts its own timer. */
  key: number;
  text: string;
  /** Messages that can be taken back; null when there is no way back. */
  messageIds: string[] | null;
}

interface UndoState {
  notice: UndoNotice | null;
  show: (text: string, messageIds: string[] | null) => void;
  clear: () => void;
}

export const useUndoStore = create<UndoState>()((set) => ({
  notice: null,
  show: (text, messageIds) => set({ notice: { key: Date.now(), text, messageIds } }),
  clear: () => set({ notice: null }),
}));
