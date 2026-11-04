import { useMutation, useQueryClient } from "@tanstack/react-query";

import { invalidateIndex, invalidateTags, qk } from "@/features/queries";
import { toast } from "@/features/toast";
import { api, errorMessage } from "@/lib/api";
import { plural } from "@/lib/utils";

export function parseKey(key: string): { type: "m" | "c" | "d"; id: number; path?: string } {
  const [type, id, ...rest] = key.split(":");
  return { type: type as "m" | "c" | "d", id: Number(id), path: rest.join(":") };
}

export function splitSelection(keys: string[]) {
  const modelIds: number[] = [];
  const containerIds: number[] = [];
  for (const k of keys) {
    const p = parseKey(k);
    if (p.type === "m") modelIds.push(p.id);
    else if (p.type === "c") containerIds.push(p.id);
  }
  return { modelIds, containerIds };
}

export function useOpenInSlicer() {
  return useMutation({
    mutationFn: ({ modelIds, slicerId }: { modelIds: number[]; slicerId?: number }) =>
      api.openInSlicer(modelIds, slicerId),
    onMutate: ({ modelIds }) => {
      if (modelIds.length > 0) toast.info(`Preparing ${plural(modelIds.length, "file")}…`);
    },
    onSuccess: (r) => {
      if (r.message) toast.info("Opened one at a time", r.message);
      else toast.success(`Sent ${plural(r.files, "file")} to the slicer`);
    },
    onError: (e) => toast.error("Could not open in slicer", errorMessage(e)),
  });
}

export function useVaultSave() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ modelIds, containerIds }: { modelIds: number[]; containerIds: number[] }) =>
      api.vaultSave(modelIds, containerIds),
    onSuccess: (r) => {
      invalidateIndex(qc);
      if (r.moved.length > 0) {
        toast.success(
          `Saved ${plural(r.moved.length, "item")} to the vault`,
          r.moved.slice(0, 3).join(", ") + (r.moved.length > 3 ? "…" : ""),
        );
      }
      for (const err of r.errors) toast.error("Not saved", err);
      if (r.moved.length === 0 && r.errors.length === 0) toast.info("Already in the vault");
    },
    onError: (e) => toast.error("Could not save to the vault", errorMessage(e)),
  });
}

export function useTagItems() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (v: { tagId: number; modelIds: number[]; containerIds: number[]; add: boolean }) =>
      api.tagItems(v.tagId, v.modelIds, v.containerIds, v.add),
    onSuccess: () => invalidateTags(qc),
    onError: (e) => toast.error("Could not change tags", errorMessage(e)),
  });
}

export function useSetPack() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (v: { containerIds: number[]; isPack: boolean }) =>
      api.setPack(v.containerIds, v.isPack),
    onSuccess: () => invalidateIndex(qc),
    onError: (e) => toast.error("Could not mark as pack", errorMessage(e)),
  });
}

export function useCollectionAdd() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (v: { id: number; containerIds: number[]; add: boolean }) =>
      api.collectionAdd(v.id, v.containerIds, v.add),
    onSuccess: (_, v) => {
      void qc.invalidateQueries({ queryKey: qk.collections });
      void qc.invalidateQueries({ queryKey: ["collection-items"] });
      if (v.add) toast.success(`Added ${plural(v.containerIds.length, "pack")} to the collection`);
    },
    onError: (e) => toast.error("Could not change the collection", errorMessage(e)),
  });
}
