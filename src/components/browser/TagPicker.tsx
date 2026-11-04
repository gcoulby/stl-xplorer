import { useQueryClient } from "@tanstack/react-query";
import { Plus } from "lucide-react";
import { useState } from "react";

import { Input } from "@/components/ui/input";
import { Checkbox } from "@/components/ui/misc";
import { useTagItems } from "@/features/actions";
import { qk, useTags } from "@/features/queries";
import { toast } from "@/features/toast";
import { api, errorMessage } from "@/lib/api";

export const TAG_COLOURS = [
  "#38bdf8",
  "#a78bfa",
  "#f472b6",
  "#fb923c",
  "#facc15",
  "#4ade80",
  "#2dd4bf",
  "#94a3b8",
];

/** Checklist of tags for a set of items. Ticks show tags every item already has. */
export function TagPicker({
  modelIds,
  containerIds,
  currentTagIds,
}: {
  modelIds: number[];
  containerIds: number[];
  /** Tag ids per item, for showing ticked or mixed state. */
  currentTagIds: number[][];
}) {
  const qc = useQueryClient();
  const tags = useTags().data ?? [];
  const tagItems = useTagItems();
  const [name, setName] = useState("");
  const total = currentTagIds.length;

  const state = (id: number): boolean | "indeterminate" => {
    const n = currentTagIds.filter((ids) => ids.includes(id)).length;
    if (n === 0) return false;
    return n === total ? true : "indeterminate";
  };

  const create = async () => {
    const n = name.trim();
    if (!n) return;
    try {
      const colour = TAG_COLOURS[tags.length % TAG_COLOURS.length];
      const tag = await api.createTag(n, colour);
      await qc.invalidateQueries({ queryKey: qk.tags });
      tagItems.mutate({ tagId: tag.id, modelIds, containerIds, add: true });
      setName("");
    } catch (e) {
      toast.error("Could not create tag", errorMessage(e));
    }
  };

  return (
    <div className="flex flex-col gap-2">
      <div className="max-h-56 overflow-y-auto">
        {tags.length === 0 && (
          <p className="px-1 py-2 text-xs text-muted-foreground">No tags yet. Create one below.</p>
        )}
        {tags.map((t) => {
          const s = state(t.id);
          return (
            <label
              key={t.id}
              className="flex cursor-pointer items-center gap-2 rounded-md px-1.5 py-1.5 text-sm hover:bg-accent"
            >
              <Checkbox
                checked={s}
                onCheckedChange={() =>
                  tagItems.mutate({ tagId: t.id, modelIds, containerIds, add: s !== true })
                }
              />
              <span className="size-2.5 rounded-full" style={{ backgroundColor: t.colour }} />
              <span className="truncate">{t.name}</span>
            </label>
          );
        })}
      </div>
      <form
        className="flex gap-1.5"
        onSubmit={(e) => {
          e.preventDefault();
          void create();
        }}
      >
        <Input
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="New tag"
          className="h-8"
        />
        <button
          type="submit"
          className="inline-flex size-8 shrink-0 items-center justify-center rounded-md border hover:bg-accent disabled:opacity-50"
          disabled={!name.trim()}
          aria-label="Create tag"
        >
          <Plus className="size-4" />
        </button>
      </form>
    </div>
  );
}
