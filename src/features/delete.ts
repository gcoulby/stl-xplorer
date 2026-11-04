import { useMutation, useQueryClient } from "@tanstack/react-query";
import { create } from "zustand";

import type { ListItem } from "@/features/listing";
import { invalidateIndex } from "@/features/queries";
import { toast } from "@/features/toast";
import { api, errorMessage } from "@/lib/api";
import { plural } from "@/lib/utils";
import { useUi } from "@/store/ui";

export interface DeleteTarget {
  key: string;
  name: string;
  modelId?: number;
  containerId?: number;
  /** Extra words for the confirmation, e.g. "folder and everything in it". */
  note?: string;
  vaulted: boolean;
}

/**
 * Which selected items can be deleted on their own. Files inside archives, nested archives
 * and folders inside archives cannot: the archive itself would have to be rewritten.
 */
export function deletable(items: ListItem[]): DeleteTarget[] {
  const out: DeleteTarget[] = [];
  for (const it of items) {
    if (it.type === "model" && !it.model.inArchive) {
      out.push({
        key: it.key,
        name: it.model.name,
        modelId: it.model.id,
        vaulted: it.model.vaulted,
      });
    } else if (
      it.type === "container" &&
      it.container.entryPath === null &&
      it.container.parentId !== null
    ) {
      const c = it.container;
      out.push({
        key: it.key,
        name: c.name,
        containerId: c.id,
        vaulted: c.vaulted,
        note:
          c.kind === "folder"
            ? `folder and the ${plural(c.fileCount, "file")} in it`
            : c.partCount > 1
              ? `all ${c.partCount} parts`
              : undefined,
      });
    }
  }
  return out;
}

interface ConfirmState {
  targets: DeleteTarget[];
  /** Selected items that cannot be deleted, so the dialog can say so. */
  skipped: number;
  ask: (items: ListItem[]) => void;
  close: () => void;
}

export const useDeleteConfirm = create<ConfirmState>()((set) => ({
  targets: [],
  skipped: 0,
  ask: (items) => {
    const targets = deletable(items);
    if (targets.length === 0) {
      toast.info(
        "Nothing here can be deleted on its own",
        "Files inside an archive can only go with the whole archive.",
      );
      return;
    }
    set({ targets, skipped: items.length - targets.length });
  },
  close: () => set({ targets: [], skipped: 0 }),
}));

export function useDeleteItems() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (targets: DeleteTarget[]) =>
      api.deleteItems(
        targets.flatMap((t) => (t.modelId !== undefined ? [t.modelId] : [])),
        targets.flatMap((t) => (t.containerId !== undefined ? [t.containerId] : [])),
      ),
    onSuccess: (r, targets) => {
      invalidateIndex(qc);
      const ui = useUi.getState();
      const gone = new Set(targets.map((t) => t.key));
      ui.setSelection(
        ui.selection.filter((k) => !gone.has(k)),
        null,
      );
      if (ui.previewModelId !== null && gone.has(`m:${ui.previewModelId}`)) ui.setPreview(null);
      if (r.deleted.length > 0) {
        toast.success(
          `Moved ${plural(r.deleted.length, "item")} to the Recycle Bin`,
          r.deleted.slice(0, 3).join(", ") + (r.deleted.length > 3 ? "…" : ""),
        );
      }
      for (const err of r.errors) toast.error("Not deleted", err);
    },
    onError: (e) => toast.error("Could not delete", errorMessage(e)),
  });
}
