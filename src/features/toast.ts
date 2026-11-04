import { create } from "zustand";

export type ToastTone = "default" | "success" | "error";

export interface Toast {
  id: number;
  title: string;
  description?: string;
  tone: ToastTone;
}

interface ToastState {
  toasts: Toast[];
  push: (t: Omit<Toast, "id">) => void;
  dismiss: (id: number) => void;
}

let nextId = 1;

export const useToasts = create<ToastState>()((set, get) => ({
  toasts: [],
  push: (t) => {
    const id = nextId++;
    set({ toasts: [...get().toasts.slice(-3), { ...t, id }] });
    window.setTimeout(() => get().dismiss(id), t.tone === "error" ? 8000 : 4000);
  },
  dismiss: (id) => set({ toasts: get().toasts.filter((x) => x.id !== id) }),
}));

export const toast = {
  info: (title: string, description?: string) =>
    useToasts.getState().push({ title, description, tone: "default" }),
  success: (title: string, description?: string) =>
    useToasts.getState().push({ title, description, tone: "success" }),
  error: (title: string, description?: string) =>
    useToasts.getState().push({ title, description, tone: "error" }),
};
