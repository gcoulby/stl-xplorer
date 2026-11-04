import { useQueryClient } from "@tanstack/react-query";
import {
  Copy,
  Eye,
  ExternalLink,
  FolderInput,
  FolderOpen,
  FolderSearch,
  Package,
  PackageOpen,
  Plus,
  Send,
  ShieldCheck,
  Tag as TagIcon,
  Trash2,
} from "lucide-react";

import {
  ContextMenuCheckboxItem,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
} from "@/components/ui/menu";
import {
  useCollectionAdd,
  useOpenInSlicer,
  useSetPack,
  useTagItems,
  useVaultSave,
} from "@/features/actions";
import type { ListItem } from "@/features/listing";
import { deletable, useDeleteConfirm } from "@/features/delete";
import { qk, useCollections, useSlicers, useTags } from "@/features/queries";
import { usePrompt, type SelectedInfo } from "@/features/selection";
import { toast } from "@/features/toast";
import { api, errorMessage } from "@/lib/api";
import { isMesh, plural } from "@/lib/utils";
import { TAG_COLOURS } from "@/components/browser/TagPicker";

export function ItemMenuContent({
  sel,
  onActivate,
}: {
  sel: SelectedInfo;
  onActivate: (item: ListItem) => void;
}) {
  const qc = useQueryClient();
  const slicers = useSlicers().data ?? [];
  const tags = useTags().data ?? [];
  const collections = useCollections().data ?? [];
  const open = useOpenInSlicer();
  const vault = useVaultSave();
  const tagItems = useTagItems();
  const setPack = useSetPack();
  const collectionAdd = useCollectionAdd();
  const ask = usePrompt((s) => s.ask);
  const askDelete = useDeleteConfirm((s) => s.ask);

  const single = sel.items.length === 1 ? sel.items[0] : undefined;
  const def = slicers.find((s) => s.isDefault);
  const others = slicers.filter((s) => !s.isDefault);
  const n = sel.items.length;
  if (n === 0) return null;

  const tagState = (id: number): boolean | "indeterminate" => {
    const c = sel.tagIdsPerItem.filter((ids) => ids.includes(id)).length;
    if (c === 0) return false;
    return c === sel.tagIdsPerItem.length ? true : "indeterminate";
  };
  const packs = sel.items.filter((i) => i.type === "container");
  const allPacks = packs.every((i) => i.type === "container" && i.container.isPack);

  const newTag = () =>
    ask({
      title: "New tag",
      placeholder: "e.g. Painted, Terrain, To print",
      onSubmit: (name) => {
        void (async () => {
          try {
            const t = await api.createTag(name, TAG_COLOURS[tags.length % TAG_COLOURS.length]);
            await qc.invalidateQueries({ queryKey: qk.tags });
            tagItems.mutate({
              tagId: t.id,
              modelIds: sel.modelIds,
              containerIds: sel.containerIds,
              add: true,
            });
          } catch (e) {
            toast.error("Could not create tag", errorMessage(e));
          }
        })();
      },
    });

  const newCollection = () =>
    ask({
      title: "New collection",
      placeholder: "e.g. Dungeon tiles",
      onSubmit: (name) => {
        void (async () => {
          try {
            const c = await api.createCollection(name);
            collectionAdd.mutate({ id: c.id, containerIds: sel.containerIds, add: true });
          } catch (e) {
            toast.error("Could not create collection", errorMessage(e));
          }
        })();
      },
    });

  return (
    <ContextMenuContent className="w-60">
      <ContextMenuLabel className="truncate">
        {single
          ? single.type === "model"
            ? single.model.name
            : single.type === "container"
              ? single.container.name
              : single.type === "tag"
                ? single.tag.name
                : single.dir.name
          : plural(n, "item")}
      </ContextMenuLabel>
      {single && single.type !== "model" && (
        <ContextMenuItem onSelect={() => onActivate(single)}>
          <FolderOpen /> Open
        </ContextMenuItem>
      )}
      {single?.type === "model" && (
        <ContextMenuItem onSelect={() => onActivate(single)}>
          <Eye /> Preview
        </ContextMenuItem>
      )}
      {single?.type === "model" && !isMesh(single.model.ext) && (
        <ContextMenuItem
          onSelect={() =>
            void api
              .openExternally(single.model.id)
              .catch((e: unknown) => toast.error("Could not open the file", errorMessage(e)))
          }
        >
          <ExternalLink /> Open with default app
        </ContextMenuItem>
      )}
      {sel.openableIds.length > 0 && (
        <>
          <ContextMenuItem
            disabled={!def}
            onSelect={() => open.mutate({ modelIds: sel.openableIds, slicerId: def?.id })}
          >
            <Send /> Open in {def?.name ?? "slicer"}
          </ContextMenuItem>
          {others.length > 0 && (
            <ContextMenuSub>
              <ContextMenuSubTrigger>
                <Send /> Open with
              </ContextMenuSubTrigger>
              <ContextMenuSubContent>
                {others.map((s) => (
                  <ContextMenuItem
                    key={s.id}
                    onSelect={() => open.mutate({ modelIds: sel.openableIds, slicerId: s.id })}
                  >
                    {s.name}
                  </ContextMenuItem>
                ))}
              </ContextMenuSubContent>
            </ContextMenuSub>
          )}
        </>
      )}
      {(sel.modelIds.length > 0 || sel.containerIds.length > 0) && (
        <>
          <ContextMenuSeparator />
          <ContextMenuSub>
            <ContextMenuSubTrigger>
              <TagIcon /> Tags
            </ContextMenuSubTrigger>
            <ContextMenuSubContent className="max-h-80 overflow-y-auto">
              {tags.map((t) => {
                const s = tagState(t.id);
                return (
                  <ContextMenuCheckboxItem
                    key={t.id}
                    checked={s}
                    onSelect={(e) => e.preventDefault()}
                    onCheckedChange={() =>
                      tagItems.mutate({
                        tagId: t.id,
                        modelIds: sel.modelIds,
                        containerIds: sel.containerIds,
                        add: s !== true,
                      })
                    }
                  >
                    <span className="size-2.5 rounded-full" style={{ backgroundColor: t.colour }} />
                    {t.name}
                  </ContextMenuCheckboxItem>
                );
              })}
              {tags.length > 0 && <ContextMenuSeparator />}
              <ContextMenuItem onSelect={newTag}>
                <Plus /> New tag…
              </ContextMenuItem>
            </ContextMenuSubContent>
          </ContextMenuSub>
        </>
      )}
      {sel.containerIds.length > 0 && (
        <ContextMenuSub>
          <ContextMenuSubTrigger>
            <FolderInput /> Add to collection
          </ContextMenuSubTrigger>
          <ContextMenuSubContent className="max-h-80 overflow-y-auto">
            {collections.map((c) => (
              <ContextMenuItem
                key={c.id}
                onSelect={() =>
                  collectionAdd.mutate({ id: c.id, containerIds: sel.containerIds, add: true })
                }
              >
                {c.name}
              </ContextMenuItem>
            ))}
            {collections.length > 0 && <ContextMenuSeparator />}
            <ContextMenuItem onSelect={newCollection}>
              <Plus /> New collection…
            </ContextMenuItem>
          </ContextMenuSubContent>
        </ContextMenuSub>
      )}
      {sel.folderIds.length > 0 && sel.folderIds.length === packs.length && (
        <ContextMenuItem
          onSelect={() => setPack.mutate({ containerIds: sel.folderIds, isPack: !allPacks })}
        >
          {allPacks ? <PackageOpen /> : <Package />}
          {allPacks ? "Unmark as pack" : "Mark as pack"}
        </ContextMenuItem>
      )}
      {sel.vaultable && (
        <>
          <ContextMenuSeparator />
          <ContextMenuItem
            onSelect={() =>
              vault.mutate({ modelIds: sel.modelIds, containerIds: sel.containerIds })
            }
          >
            <ShieldCheck /> Save to vault
          </ContextMenuItem>
        </>
      )}
      {single && (single.type === "model" || single.type === "container") && (
        <>
          <ContextMenuSeparator />
          <ContextMenuItem
            onSelect={() =>
              void api
                .revealInFolder(
                  single.type === "model"
                    ? { modelId: single.model.id }
                    : { containerId: single.container.id },
                )
                .catch((e: unknown) => toast.error("Could not show it", errorMessage(e)))
            }
          >
            <FolderSearch /> Show in folder
          </ContextMenuItem>
          <ContextMenuItem
            onSelect={() => {
              const text = single.type === "model" ? single.model.relPath : single.container.path;
              void navigator.clipboard.writeText(text).then(() => toast.info("Path copied"));
            }}
          >
            <Copy /> Copy path
          </ContextMenuItem>
        </>
      )}
      {deletable(sel.items).length > 0 && (
        <>
          <ContextMenuSeparator />
          <ContextMenuItem variant="destructive" onSelect={() => askDelete(sel.items)}>
            <Trash2 /> Delete…
            <span className="ml-auto text-[11px] opacity-60">Del</span>
          </ContextMenuItem>
        </>
      )}
    </ContextMenuContent>
  );
}
