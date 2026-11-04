import { useMemo } from "react";
import { create } from "zustand";

import type { ListItem } from "@/features/listing";
import { isMesh } from "@/lib/utils";
import { useUi } from "@/store/ui";

export interface SelectedInfo {
  items: ListItem[];
  modelIds: number[];
  containerIds: number[];
  folderIds: number[];
  /** Meshes that can be sent to a slicer. Images and PDFs are left out. */
  openableIds: number[];
  vaultable: boolean;
  tagIdsPerItem: number[][];
}

export function describeSelection(items: ListItem[]): SelectedInfo {
  const modelIds: number[] = [];
  const containerIds: number[] = [];
  const folderIds: number[] = [];
  const openableIds: number[] = [];
  const tagIdsPerItem: number[][] = [];
  let vaultable = false;
  for (const it of items) {
    if (it.type === "model") {
      modelIds.push(it.model.id);
      tagIdsPerItem.push(it.model.tagIds);
      if (!it.model.encrypted && isMesh(it.model.ext)) openableIds.push(it.model.id);
      if (!it.model.vaulted) vaultable = true;
    } else if (it.type === "container") {
      containerIds.push(it.container.id);
      tagIdsPerItem.push(it.container.tagIds);
      if (it.container.kind === "folder") folderIds.push(it.container.id);
      else if (!it.container.vaulted && it.container.status !== "missing_parts") vaultable = true;
    }
  }
  return { items, modelIds, containerIds, folderIds, openableIds, vaultable, tagIdsPerItem };
}

export function useSelected(all: ListItem[]): SelectedInfo {
  const selection = useUi((s) => s.selection);
  return useMemo(() => {
    const set = new Set(selection);
    return describeSelection(all.filter((i) => set.has(i.key)));
  }, [all, selection]);
}

/** A tiny global "name this" prompt, used from menus that cannot host a form. */
interface PromptState {
  open: boolean;
  title: string;
  placeholder: string;
  initial: string;
  onSubmit: ((value: string) => void) | null;
  ask: (opts: {
    title: string;
    placeholder?: string;
    initial?: string;
    onSubmit: (v: string) => void;
  }) => void;
  close: () => void;
}

export const usePrompt = create<PromptState>()((set) => ({
  open: false,
  title: "",
  placeholder: "",
  initial: "",
  onSubmit: null,
  ask: ({ title, placeholder = "", initial = "", onSubmit }) =>
    set({ open: true, title, placeholder, initial, onSubmit }),
  close: () => set({ open: false, onSubmit: null }),
}));
