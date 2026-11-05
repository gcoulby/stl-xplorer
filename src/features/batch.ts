import { useQuery } from "@tanstack/react-query";
import { useEffect } from "react";
import { create } from "zustand";
import { persist } from "zustand/middleware";

import { qk } from "@/features/queries";
import { toast } from "@/features/toast";
import { api } from "@/lib/api";
import { plural } from "@/lib/utils";

/**
 * Files gathered from any number of folders and archives, to send to a slicer in one launch.
 * Only ids are kept; names and locations are looked up fresh so renames and deletes show.
 */
interface BatchState {
  ids: number[];
  open: boolean;
  add: (ids: number[]) => void;
  remove: (ids: number[]) => void;
  clear: () => void;
  setOpen: (open: boolean) => void;
}

export const useBatch = create<BatchState>()(
  persist(
    (set, get) => ({
      ids: [],
      open: false,
      add: (ids) => {
        const have = new Set(get().ids);
        const fresh = ids.filter((id) => !have.has(id));
        if (fresh.length === 0) {
          toast.info(ids.length === 1 ? "Already in the batch" : "All already in the batch");
          return;
        }
        set({ ids: [...get().ids, ...fresh] });
        const skipped = ids.length - fresh.length;
        toast.success(
          `Added ${plural(fresh.length, "file")} to the print batch`,
          skipped > 0 ? `${plural(skipped, "file")} already there.` : undefined,
        );
      },
      remove: (ids) => {
        const drop = new Set(ids);
        set({ ids: get().ids.filter((id) => !drop.has(id)) });
      },
      clear: () => set({ ids: [] }),
      setOpen: (open) => set({ open }),
    }),
    { name: "stl-xplorer-batch", partialize: (s) => ({ ids: s.ids }) },
  ),
);

/** Current details for the batch. Files that no longer exist are dropped from it. */
export function useBatchItems() {
  const ids = useBatch((s) => s.ids);
  const q = useQuery({
    queryKey: qk.batch(ids),
    queryFn: () => api.batchItems(ids),
    enabled: ids.length > 0,
    staleTime: 30_000,
  });
  const data = q.data;
  useEffect(() => {
    if (!data) return;
    const known = new Set(data.map((i) => i.model.id));
    const gone = ids.filter((id) => !known.has(id));
    if (gone.length > 0) useBatch.getState().remove(gone);
  }, [data, ids]);
  return { ...q, items: ids.length === 0 ? [] : (data ?? []) };
}
