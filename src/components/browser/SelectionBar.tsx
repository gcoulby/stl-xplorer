import {
  ChevronDown,
  FolderInput,
  Printer,
  Plus,
  Send,
  ShieldCheck,
  Tag as TagIcon,
  Trash2,
  X,
} from "lucide-react";

import { TagPicker } from "@/components/browser/TagPicker";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/menu";
import { Popover, PopoverContent, PopoverTrigger, Tip } from "@/components/ui/overlay";
import { useCollectionAdd, useOpenInSlicer, useVaultSave } from "@/features/actions";
import { useBatch } from "@/features/batch";
import { deletable, useDeleteConfirm } from "@/features/delete";
import { useCollections, useSlicers } from "@/features/queries";
import { usePrompt, type SelectedInfo } from "@/features/selection";
import { toast } from "@/features/toast";
import { api, errorMessage } from "@/lib/api";
import { plural } from "@/lib/utils";

export function SelectionBar({
  sel,
  onClear,
  onOpen,
}: {
  sel: SelectedInfo;
  onClear: () => void;
  onOpen: (ids: number[]) => void;
}) {
  const slicers = useSlicers().data ?? [];
  const collections = useCollections().data ?? [];
  const open = useOpenInSlicer();
  const vault = useVaultSave();
  const collectionAdd = useCollectionAdd();
  const ask = usePrompt((s) => s.ask);
  const askDelete = useDeleteConfirm((s) => s.ask);
  const addToBatch = useBatch((s) => s.add);
  const n = sel.items.length;
  if (n === 0) return null;
  const def = slicers.find((s) => s.isDefault);
  const canOpen = sel.openableIds.length > 0;

  return (
    <div className="flex h-12 shrink-0 animate-in items-center gap-1.5 border-t bg-card px-3 slide-in-from-bottom-2 fade-in-0">
      <span className="mr-1 shrink-0 text-sm font-medium whitespace-nowrap tabular-nums">
        {plural(n, "item")} selected
      </span>
      <div className="flex-1" />
      {canOpen && (
        <div className="flex">
          <Button
            size="sm"
            className="rounded-r-none"
            onClick={() => onOpen(sel.openableIds)}
            disabled={open.isPending}
          >
            <Send />
            {def ? `Open in ${def.name}` : "Open in slicer"}
            {sel.openableIds.length > 1 && (
              <span className="rounded bg-primary-foreground/20 px-1 text-[11px] tabular-nums">
                {sel.openableIds.length}
              </span>
            )}
          </Button>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                size="sm"
                className="rounded-l-none border-l border-primary-foreground/20 px-1.5"
              >
                <ChevronDown />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuLabel>Open with</DropdownMenuLabel>
              {slicers.map((s) => (
                <DropdownMenuItem
                  key={s.id}
                  onSelect={() => open.mutate({ modelIds: sel.openableIds, slicerId: s.id })}
                >
                  {s.name}
                  {s.isDefault && (
                    <span className="ml-auto text-xs text-muted-foreground">default</span>
                  )}
                </DropdownMenuItem>
              ))}
              {slicers.length === 0 && (
                <DropdownMenuItem disabled>No slicers set up</DropdownMenuItem>
              )}
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      )}
      {canOpen && (
        <Tip label="Add to print batch (B)">
          <Button variant="outline" size="sm" onClick={() => addToBatch(sel.openableIds)}>
            <Printer />
            Batch
          </Button>
        </Tip>
      )}
      {(sel.modelIds.length > 0 || sel.containerIds.length > 0) && (
        <Popover>
          <Tip label="Tags">
            <PopoverTrigger asChild>
              <Button variant="outline" size="sm">
                <TagIcon />
                Tag
              </Button>
            </PopoverTrigger>
          </Tip>
          <PopoverContent align="end" className="w-64">
            <TagPicker
              modelIds={sel.modelIds}
              containerIds={sel.containerIds}
              currentTagIds={sel.tagIdsPerItem}
            />
          </PopoverContent>
        </Popover>
      )}
      {sel.containerIds.length > 0 && (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="outline" size="sm">
              <FolderInput />
              Collection
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            {collections.map((c) => (
              <DropdownMenuItem
                key={c.id}
                onSelect={() =>
                  collectionAdd.mutate({ id: c.id, containerIds: sel.containerIds, add: true })
                }
              >
                {c.name}
              </DropdownMenuItem>
            ))}
            {collections.length > 0 && <DropdownMenuSeparator />}
            <DropdownMenuItem
              onSelect={() =>
                ask({
                  title: "New collection",
                  placeholder: "e.g. Dungeon tiles",
                  onSubmit: (name) => {
                    void api
                      .createCollection(name)
                      .then((c) =>
                        collectionAdd.mutate({
                          id: c.id,
                          containerIds: sel.containerIds,
                          add: true,
                        }),
                      )
                      .catch((e: unknown) =>
                        toast.error("Could not create collection", errorMessage(e)),
                      );
                  },
                })
              }
            >
              <Plus /> New collection…
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      )}
      {sel.vaultable && (
        <Tip label="Move into the vault so the downloads clean-up cannot delete it">
          <Button
            variant="outline"
            size="sm"
            disabled={vault.isPending}
            onClick={() => vault.mutate({ modelIds: sel.modelIds, containerIds: sel.containerIds })}
          >
            <ShieldCheck />
            Save
          </Button>
        </Tip>
      )}
      {deletable(sel.items).length > 0 && (
        <Tip label="Delete (Del)">
          <Button
            variant="ghost"
            size="icon-sm"
            className="text-muted-foreground hover:text-destructive"
            onClick={() => askDelete(sel.items)}
          >
            <Trash2 />
          </Button>
        </Tip>
      )}
      <Tip label="Clear selection (Esc)">
        <Button variant="ghost" size="icon-sm" onClick={onClear}>
          <X />
        </Button>
      </Tip>
    </div>
  );
}
