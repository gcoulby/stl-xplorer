import { ChevronDown, Eye, Printer, Send, Trash2, X } from "lucide-react";

import { ModelIcon } from "@/components/browser/ItemVisuals";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/components/ui/menu";
import { Kbd, Skeleton } from "@/components/ui/misc";
import { Popover, PopoverContent, PopoverTrigger, Tip } from "@/components/ui/overlay";
import { useOpenInSlicer } from "@/features/actions";
import { useBatch, useBatchItems } from "@/features/batch";
import { modelKey } from "@/features/listing";
import { useSlicers } from "@/features/queries";
import { cn, formatBytes, plural } from "@/lib/utils";
import { useUi } from "@/store/ui";
import type { BatchItem } from "@/types";

export function BatchButton() {
  const { ids, open, setOpen } = useBatch();
  const n = ids.length;
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <Tip label="Print batch (B adds the selection)">
        <PopoverTrigger asChild>
          <Button
            variant={n > 0 ? "secondary" : "ghost"}
            size="sm"
            className={cn("gap-1.5", n === 0 && "px-2")}
            aria-label={`Print batch, ${plural(n, "file")}`}
          >
            <Printer />
            {n > 0 && <span className="text-xs tabular-nums">{n}</span>}
          </Button>
        </PopoverTrigger>
      </Tip>
      <PopoverContent align="end" className="flex w-[26rem] flex-col gap-0 p-0">
        <BatchPanel />
      </PopoverContent>
    </Popover>
  );
}

function BatchPanel() {
  const { ids, remove, clear, setOpen } = useBatch();
  const { items, isLoading } = useBatchItems();
  const slicers = useSlicers().data ?? [];
  const def = slicers.find((s) => s.isDefault);
  const open = useOpenInSlicer();
  const ready = items.filter((i) => !i.model.missing).map((i) => i.model.id);
  const send = (slicerId?: number) => open.mutate({ modelIds: ready, slicerId });
  const total = items.reduce((t, i) => t + i.model.size, 0);

  return (
    <>
      <div className="flex items-center justify-between gap-2 border-b px-3 py-2.5">
        <div>
          <h3 className="text-sm font-semibold">Print batch</h3>
          <p className="text-xs text-muted-foreground">
            {ids.length === 0
              ? "Empty"
              : `${plural(ids.length, "file")}${total > 0 ? ` · ${formatBytes(total)}` : ""}`}
          </p>
        </div>
        {ids.length > 0 && (
          <Button
            variant="ghost"
            size="sm"
            className="text-muted-foreground hover:text-destructive"
            onClick={clear}
          >
            <Trash2 /> Clear
          </Button>
        )}
      </div>

      {ids.length === 0 ? (
        <div className="flex flex-col items-center gap-2 px-6 py-8 text-center">
          <Printer className="size-6 text-muted-foreground" />
          <p className="text-sm">Gather files from different folders and archives</p>
          <p className="text-xs text-muted-foreground">
            Select files and press <Kbd>B</Kbd>, or right-click and choose{" "}
            <span className="text-foreground">Add to print batch</span>. Then open them all in your
            slicer at once.
          </p>
        </div>
      ) : (
        <ul className="max-h-80 overflow-y-auto py-1">
          {isLoading && items.length === 0
            ? ids.slice(0, 4).map((id) => (
                <li key={id} className="flex flex-col gap-1.5 px-3 py-2">
                  <Skeleton className="h-3.5 w-40" />
                  <Skeleton className="h-3 w-56" />
                </li>
              ))
            : items.map((it) => (
                <BatchRow
                  key={it.model.id}
                  item={it}
                  onRemove={() => remove([it.model.id])}
                  onShow={() => {
                    showInBrowser(it);
                    setOpen(false);
                  }}
                />
              ))}
        </ul>
      )}

      {ids.length > 0 && (
        <div className="flex items-center justify-end gap-2 border-t px-3 py-2.5">
          <div className="flex">
            <Button
              size="sm"
              className="rounded-r-none"
              disabled={open.isPending || !def || ready.length === 0}
              onClick={() => send(def?.id)}
            >
              <Send />
              {def ? `Open all in ${def.name}` : "Add a slicer in Settings"}
            </Button>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  size="sm"
                  className="rounded-l-none border-l border-primary-foreground/20 px-1.5"
                  aria-label="Choose a slicer"
                >
                  <ChevronDown />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuLabel>Open with</DropdownMenuLabel>
                {slicers.map((s) => (
                  <DropdownMenuItem key={s.id} onSelect={() => send(s.id)}>
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
        </div>
      )}
    </>
  );
}

function BatchRow({
  item,
  onRemove,
  onShow,
}: {
  item: BatchItem;
  onRemove: () => void;
  onShow: () => void;
}) {
  const m = item.model;
  return (
    <li className="group flex items-center gap-2 px-3 py-1.5 hover:bg-accent/50">
      <ModelIcon ext={m.ext} className="size-4 shrink-0" />
      <button
        type="button"
        onClick={onShow}
        className="flex min-w-0 flex-1 flex-col text-left"
        title={`${item.location} › ${m.name}`}
      >
        <span className={cn("truncate text-sm", m.missing && "text-muted-foreground line-through")}>
          {m.name}
        </span>
        <span className="truncate text-[11px] text-muted-foreground">
          {m.missing ? "Missing from disk · " : ""}
          {item.location}
        </span>
      </button>
      <Tip label="Show in browser">
        <Button
          variant="ghost"
          size="icon-xs"
          className="opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
          onClick={onShow}
        >
          <Eye />
        </Button>
      </Tip>
      <Tip label="Remove from batch">
        <Button variant="ghost" size="icon-xs" onClick={onRemove} aria-label={`Remove ${m.name}`}>
          <X />
        </Button>
      </Tip>
    </li>
  );
}

function showInBrowser(item: BatchItem) {
  const ui = useUi.getState();
  ui.navigate({ kind: "container", containerId: item.containerId, subpath: item.subpath });
  ui.setSelection([modelKey(item.model.id)], modelKey(item.model.id));
  ui.setPreview(item.model.id);
}
