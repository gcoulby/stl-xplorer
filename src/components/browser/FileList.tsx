import { useVirtualizer } from "@tanstack/react-virtual";
import {
  ArrowDown,
  ArrowUp,
  FolderSearch,
  FolderTree,
  ListTree,
  SearchX,
  Tag as TagIcon,
  TriangleAlert,
} from "lucide-react";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";

import {
  ArchiveProblemBanner,
  ArchiveProblemPanel,
  hasProblem,
} from "@/components/browser/ArchiveProblem";
import { AddressBar } from "@/components/browser/AddressBar";
import { ItemMenuContent } from "@/components/browser/ItemMenu";
import {
  ArchiveDirIcon,
  ContainerBadges,
  ContainerIcon,
  ModelBadges,
  ModelIcon,
  TagDots,
} from "@/components/browser/ItemVisuals";
import { ColumnsMenu, FilterMenu, SortMenu } from "@/components/browser/ListToolbar";
import { SelectionBar } from "@/components/browser/SelectionBar";
import { Button } from "@/components/ui/button";
import { ContextMenu, ContextMenuTrigger } from "@/components/ui/menu";
import { Skeleton } from "@/components/ui/misc";
import { Tip } from "@/components/ui/overlay";
import { useOpenInSlicer } from "@/features/actions";
import { useDeleteConfirm } from "@/features/delete";
import { applyFilters, typeLabel } from "@/features/filtering";
import { useListing, type ListItem } from "@/features/listing";
import { useLibraries, useSlicers } from "@/features/queries";
import { useSelected } from "@/features/selection";
import { toast } from "@/features/toast";
import { api, errorMessage } from "@/lib/api";
import { cn, formatBytes, formatDate, isMesh, plural } from "@/lib/utils";
import { treeKey, useUi, type Columns, type SortKey } from "@/store/ui";

const ROW_H = 34;
const TILE_MIN_W = 148;
const TILE_H = 152;

function itemName(it: ListItem): string {
  if (it.type === "model") return it.model.name;
  if (it.type === "container") return it.container.name;
  if (it.type === "tag") return it.tag.name;
  return it.dir.name;
}

function openExternally(modelId: number) {
  void api
    .openExternally(modelId)
    .catch((e: unknown) => toast.error("Could not open the file", errorMessage(e)));
}

export function FileList() {
  const loc = useUi((s) => s.location);
  const flat = useUi((s) => s.flat);
  const listing = useListing(loc, flat);
  const filters = useUi((s) => s.filters);
  const sort = useUi((s) => s.sort);
  const columns = useUi((s) => s.columns);
  const items = useMemo(
    () => applyFilters(listing.items, filters, sort),
    [listing.items, filters, sort],
  );
  const hiddenByFilter = listing.items.length - items.length;
  const viewMode = useUi((s) => s.viewMode);
  const selection = useUi((s) => s.selection);
  const focused = useUi((s) => s.focused);
  const anchor = useUi((s) => s.anchor);
  const previewId = useUi((s) => s.previewModelId);
  const { navigate, setSelection, setFocused, setPreview, toggleExpanded } = useUi.getState();
  const sel = useSelected(items);
  const open = useOpenInSlicer();
  const slicers = useSlicers().data;

  const scrollRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(800);
  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    setWidth(el.clientWidth);
    return () => ro.disconnect();
  }, []);

  const cols = viewMode === "grid" ? Math.max(1, Math.floor((width - 16) / TILE_MIN_W)) : 1;
  const rowCount = Math.ceil(items.length / cols);
  const virtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => (viewMode === "grid" ? TILE_H + 8 : ROW_H),
    overscan: 12,
  });
  useEffect(() => virtualizer.measure(), [viewMode, cols, virtualizer]);

  // Keep the tree in step with where we are.
  useEffect(() => {
    if (!listing.crumbs) return;
    for (const c of listing.crumbs.slice(0, -1))
      toggleExpanded(treeKey(c.containerId, c.subpath), true);
  }, [listing.crumbs, toggleExpanded]);

  const indexOf = useMemo(() => {
    const m = new Map<string, number>();
    items.forEach((it, i) => m.set(it.key, i));
    return m;
  }, [items]);

  const activate = useCallback(
    (it: ListItem) => {
      if (it.type === "dir")
        navigate({ kind: "container", containerId: it.containerId, subpath: it.dir.path });
      else if (it.type === "container")
        navigate({ kind: "container", containerId: it.container.id, subpath: "" });
      else if (it.type === "tag") navigate({ kind: "tag", tagId: it.tag.id });
      else setPreview(it.model.id);
    },
    [navigate, setPreview],
  );

  const sendToSlicer = useCallback(
    (ids: number[]) => {
      if (ids.length === 0) return;
      if (slicers && slicers.length === 0) {
        toast.info("No slicer set up yet", "Add one in Settings → Slicers.");
        useUi.getState().openSettings("slicers");
        return;
      }
      open.mutate({ modelIds: ids });
    },
    [open, slicers],
  );

  const focusIndex = useCallback(
    (i: number, extend: boolean) => {
      const it = items[Math.max(0, Math.min(items.length - 1, i))];
      if (!it) return;
      setFocused(it.key);
      if (extend && anchor) {
        const a = indexOf.get(anchor) ?? 0;
        const b = indexOf.get(it.key) ?? 0;
        const [lo, hi] = a < b ? [a, b] : [b, a];
        setSelection(items.slice(lo, hi + 1).map((x) => x.key));
      } else {
        setSelection([it.key], it.key);
      }
      if (it.type === "model") setPreview(it.model.id);
      virtualizer.scrollToIndex(Math.floor((indexOf.get(it.key) ?? 0) / cols));
    },
    [items, anchor, indexOf, setFocused, setSelection, setPreview, virtualizer, cols],
  );

  const onClick = (e: React.MouseEvent, it: ListItem) => {
    const i = indexOf.get(it.key) ?? 0;
    setFocused(it.key);
    if (e.shiftKey && anchor) {
      const a = indexOf.get(anchor) ?? 0;
      const [lo, hi] = a < i ? [a, i] : [i, a];
      setSelection(items.slice(lo, hi + 1).map((x) => x.key));
    } else if (e.ctrlKey || e.metaKey) {
      const has = selection.includes(it.key);
      setSelection(has ? selection.filter((k) => k !== it.key) : [...selection, it.key], it.key);
    } else {
      setSelection([it.key], it.key);
    }
    if (it.type === "model") setPreview(it.model.id);
  };

  const onDoubleClick = (it: ListItem) => {
    if (it.type !== "model") return activate(it);
    if (isMesh(it.model.ext)) sendToSlicer([it.model.id]);
    else openExternally(it.model.id);
  };

  const goUp = () => {
    const crumbs = listing.crumbs;
    if (!crumbs || crumbs.length < 2) return;
    const parent = crumbs[crumbs.length - 2]!;
    navigate({ kind: "container", containerId: parent.containerId, subpath: parent.subpath });
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    const cur = focused ? (indexOf.get(focused) ?? -1) : -1;
    const step = (d: number) => {
      e.preventDefault();
      focusIndex(cur < 0 ? 0 : cur + d, e.shiftKey);
    };
    switch (e.key) {
      case "ArrowDown":
        return step(cols);
      case "ArrowUp":
        return step(-cols);
      case "ArrowRight":
        if (cols > 1) return step(1);
        if (cur >= 0 && items[cur] && items[cur]!.type !== "model") {
          e.preventDefault();
          activate(items[cur]!);
        }
        return;
      case "ArrowLeft":
        if (cols > 1) return step(-1);
        e.preventDefault();
        return goUp();
      case "Home":
        e.preventDefault();
        return focusIndex(0, e.shiftKey);
      case "End":
        e.preventDefault();
        return focusIndex(items.length - 1, e.shiftKey);
      case "PageDown":
        return step(cols * 15);
      case "PageUp":
        return step(-cols * 15);
      case " ": {
        e.preventDefault();
        if (!focused) return focusIndex(0, false);
        const has = selection.includes(focused);
        setSelection(
          has ? selection.filter((k) => k !== focused) : [...selection, focused],
          focused,
        );
        return;
      }
      case "Enter": {
        e.preventDefault();
        const it = cur >= 0 ? items[cur] : undefined;
        if (sel.openableIds.length > 0) return sendToSlicer(sel.openableIds);
        if (it && it.type !== "model") return activate(it);
        if (it?.type === "model" && isMesh(it.model.ext)) return sendToSlicer([it.model.id]);
        if (it?.type === "model") return openExternally(it.model.id);
        return;
      }
      case "Backspace":
        e.preventDefault();
        return goUp();
      case "Delete":
        if (sel.items.length > 0) {
          e.preventDefault();
          useDeleteConfirm.getState().ask(sel.items);
        }
        return;
      case "Escape":
        if (selection.length > 0) {
          e.preventDefault();
          setSelection([], null);
        }
        return;
      case "a":
        if (e.ctrlKey || e.metaKey) {
          e.preventDefault();
          setSelection(items.map((i) => i.key));
        }
        return;
    }
  };

  const onContextMenu = (e: React.MouseEvent) => {
    const el = (e.target as HTMLElement).closest<HTMLElement>("[data-key]");
    const key = el?.dataset.key;
    if (!key) return;
    if (!selection.includes(key)) {
      setSelection([key], key);
      setFocused(key);
    }
  };

  const selectedSet = useMemo(() => new Set(selection), [selection]);
  const problem = listing.container && hasProblem(listing.container) ? listing.container : null;

  return (
    <div className="flex h-full min-w-0 flex-col bg-pane">
      <ListHeader listing={listing} shown={items.length} hidden={hiddenByFilter} onUp={goUp} />
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div
            ref={scrollRef}
            tabIndex={0}
            data-file-list
            onKeyDown={onKeyDown}
            onContextMenu={onContextMenu}
            onMouseDown={(e) => {
              if (e.target === e.currentTarget) setSelection([], null);
            }}
            className="@container relative min-h-0 flex-1 overflow-y-auto outline-none"
          >
            {listing.isLoading && items.length === 0 ? (
              <LoadingRows grid={viewMode === "grid"} />
            ) : listing.error ? (
              <ListError error={listing.error} />
            ) : problem && items.length === 0 ? (
              <ArchiveProblemPanel container={problem} />
            ) : items.length === 0 ? (
              <EmptyState loc={loc.kind} hint={listing.emptyHint} />
            ) : (
              <>
                {problem && <ArchiveProblemBanner container={problem} />}
                {viewMode === "list" && (
                  <ListColumns columns={columns} showLocation={listing.showLocation} />
                )}
                <div
                  style={{ height: virtualizer.getTotalSize(), position: "relative" }}
                  onMouseDown={(e) => {
                    if (e.target === e.currentTarget) setSelection([], null);
                  }}
                >
                  {virtualizer.getVirtualItems().map((vr) => {
                    if (viewMode === "list") {
                      const it = items[vr.index];
                      if (!it) return null;
                      return (
                        <ListRow
                          key={it.key}
                          it={it}
                          top={vr.start}
                          selected={selectedSet.has(it.key)}
                          focused={focused === it.key}
                          previewing={it.type === "model" && it.model.id === previewId}
                          showLocation={listing.showLocation}
                          columns={columns}
                          onClick={onClick}
                          onDoubleClick={onDoubleClick}
                        />
                      );
                    }
                    const rowItems = items.slice(vr.index * cols, vr.index * cols + cols);
                    return (
                      <div
                        key={vr.key}
                        className="absolute inset-x-0 grid gap-2 px-2"
                        style={{
                          top: vr.start + 8,
                          height: TILE_H,
                          gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))`,
                        }}
                      >
                        {rowItems.map((it) => (
                          <Tile
                            key={it.key}
                            it={it}
                            selected={selectedSet.has(it.key)}
                            focused={focused === it.key}
                            previewing={it.type === "model" && it.model.id === previewId}
                            onClick={onClick}
                            onDoubleClick={onDoubleClick}
                          />
                        ))}
                      </div>
                    );
                  })}
                </div>
              </>
            )}
          </div>
        </ContextMenuTrigger>
        <ItemMenuContent sel={sel} onActivate={activate} />
      </ContextMenu>
      <SelectionBar sel={sel} onClear={() => setSelection([], null)} onOpen={sendToSlicer} />
    </div>
  );
}

function ListHeader({
  listing,
  shown,
  hidden,
  onUp,
}: {
  listing: ReturnType<typeof useListing>;
  shown: number;
  hidden: number;
  onUp: () => void;
}) {
  const viewMode = useUi((s) => s.viewMode);
  const setViewMode = useUi((s) => s.setViewMode);
  const flat = useUi((s) => s.flat);
  const setFlat = useUi((s) => s.setFlat);
  const loc = useUi((s) => s.location);
  const setFilters = useUi((s) => s.setFilters);
  const browsing = loc.kind === "container";
  return (
    <div className="flex shrink-0 flex-col border-b">
      <div className="flex h-10 items-center gap-2 px-2">
        <AddressBar
          crumbs={listing.crumbs}
          container={listing.container}
          subpath={loc.kind === "container" ? loc.subpath.replace(/^\/+|\/+$/g, "") : ""}
          title={listing.title}
          onUp={onUp}
        />
      </div>
      <div className="flex h-9 items-center gap-1.5 px-2">
        {browsing && (
          <div className="flex shrink-0 rounded-md border p-0.5">
            {([false, true] as const).map((f) => (
              <Tip
                key={String(f)}
                label={f ? "Every file below here in one list" : "Folders and archives"}
              >
                <Button
                  variant="ghost"
                  size="xs"
                  className={cn(
                    "h-6 gap-1 px-1.5 text-xs",
                    flat === f ? "bg-accent text-foreground" : "text-muted-foreground",
                  )}
                  onClick={() => setFlat(f)}
                  aria-pressed={flat === f}
                >
                  {f ? <ListTree className="size-3.5" /> : <FolderTree className="size-3.5" />}
                  {f ? "All files" : "Folders"}
                </Button>
              </Tip>
            ))}
          </div>
        )}
        <span className="min-w-0 truncate px-1 text-xs text-muted-foreground tabular-nums">
          {shown > 0 && plural(shown, flat && browsing ? "file" : "item")}
          {hidden > 0 && (
            <>
              {" · "}
              <button
                type="button"
                className="text-primary hover:underline"
                onClick={() => setFilters({ types: [], tagIds: [] })}
              >
                {hidden.toLocaleString("en-GB")} hidden by filters
              </button>
            </>
          )}
        </span>
        <div className="flex-1" />
        <FilterMenu />
        <SortMenu />
        <div className="flex shrink-0 items-center rounded-md border p-0.5">
          {(["list", "grid"] as const).map((m) => (
            <Button
              key={m}
              variant="ghost"
              size="icon-xs"
              className={cn("size-6", viewMode === m && "bg-accent text-foreground")}
              onClick={() => setViewMode(m)}
              aria-label={m === "list" ? "List view" : "Grid view"}
            >
              {m === "list" ? <ListIconSvg /> : <GridIconSvg />}
            </Button>
          ))}
          {viewMode === "list" && <ColumnsMenu />}
        </div>
      </div>
    </div>
  );
}

function ListIconSvg() {
  return (
    <svg viewBox="0 0 16 16" className="size-3.5" fill="currentColor">
      <rect x="1" y="2.5" width="14" height="2" rx="1" />
      <rect x="1" y="7" width="14" height="2" rx="1" />
      <rect x="1" y="11.5" width="14" height="2" rx="1" />
    </svg>
  );
}

function GridIconSvg() {
  return (
    <svg viewBox="0 0 16 16" className="size-3.5" fill="currentColor">
      <rect x="1" y="1" width="6" height="6" rx="1.5" />
      <rect x="9" y="1" width="6" height="6" rx="1.5" />
      <rect x="1" y="9" width="6" height="6" rx="1.5" />
      <rect x="9" y="9" width="6" height="6" rx="1.5" />
    </svg>
  );
}

function SortHeader({ k, label, className }: { k: SortKey; label: string; className?: string }) {
  const sort = useUi((s) => s.sort);
  const setSort = useUi((s) => s.setSort);
  const active = sort.key === k;
  return (
    <button
      type="button"
      onClick={() =>
        setSort(
          active
            ? { key: k, desc: !sort.desc }
            : { key: k, desc: k === "size" || k === "modified" },
        )
      }
      className={cn(
        "flex items-center gap-0.5 uppercase hover:text-foreground",
        active && "text-foreground",
        className,
      )}
    >
      {label}
      {active && (sort.desc ? <ArrowDown className="size-3" /> : <ArrowUp className="size-3" />)}
    </button>
  );
}

function ListColumns({ columns, showLocation }: { columns: Columns; showLocation: boolean }) {
  return (
    <div className="sticky top-0 z-10 flex h-7 items-center gap-3 border-b bg-pane/95 px-3 text-[11px] font-medium tracking-wide text-muted-foreground backdrop-blur">
      <span className="flex min-w-0 flex-1">
        <SortHeader k="name" label="Name" />
      </span>
      {showLocation && columns.location && (
        <span className="w-[30%] min-w-0 uppercase">Location</span>
      )}
      {columns.type && <SortHeader k="type" label="Type" className="w-14" />}
      {columns.size && <SortHeader k="size" label="Size" className="w-20 justify-end" />}
      {columns.modified && (
        <SortHeader
          k="modified"
          label="Modified"
          className="hidden w-24 justify-end @[34rem]:flex"
        />
      )}
    </div>
  );
}

interface RowProps {
  it: ListItem;
  selected: boolean;
  focused: boolean;
  previewing: boolean;
  onClick: (e: React.MouseEvent, it: ListItem) => void;
  onDoubleClick: (it: ListItem) => void;
}

function ItemIcon({ it, className }: { it: ListItem; className?: string }) {
  if (it.type === "dir") return <ArchiveDirIcon className={className} />;
  if (it.type === "container")
    return <ContainerIcon container={it.container} className={className} />;
  if (it.type === "tag")
    return (
      <TagIcon
        className={cn("size-4 shrink-0", className)}
        style={{ color: it.tag.colour }}
        fill={it.tag.colour}
        fillOpacity={0.2}
      />
    );
  return <ModelIcon ext={it.model.ext} className={className} />;
}

function counts(models: number, files: number): string {
  const extra = files - models;
  if (models === 0) return plural(files, "file");
  return extra > 0
    ? `${plural(models, "model")} · ${plural(extra, "other file")}`
    : plural(models, "model");
}

function subtitle(it: ListItem): string | null {
  if (it.type === "dir") return counts(it.dir.modelCount, it.dir.fileCount);
  if (it.type === "tag") {
    const t = it.tag;
    return t.autoUsage > 0
      ? `${plural(t.usage, "item")} · ${t.autoUsage} auto`
      : plural(t.usage, "item");
  }
  if (it.type === "container") {
    const c = it.container;
    if (c.kind !== "folder" && (c.status !== "ok" || c.encrypted)) return null;
    if (c.fileCount === 0 && c.issueCount > 0) {
      return `${plural(c.issueCount, "archive")} ${c.issueCount === 1 ? "needs" : "need"} attention`;
    }
    return counts(c.modelCount, c.fileCount);
  }
  return null;
}

function ListRow({
  it,
  top,
  selected,
  focused,
  previewing,
  showLocation,
  columns,
  onClick,
  onDoubleClick,
}: RowProps & { top: number; showLocation: boolean; columns: Columns }) {
  const size =
    it.type === "model"
      ? it.model.size
      : it.type === "container" && it.container.kind !== "folder"
        ? it.container.size
        : null;
  const mtime =
    it.type === "model" ? it.model.mtime : it.type === "container" ? it.container.mtime : 0;
  const sub = subtitle(it);
  const tagIds =
    it.type === "model" ? it.model.tagIds : it.type === "container" ? it.container.tagIds : [];
  const location = it.type === "model" || it.type === "container" ? it.location : undefined;
  return (
    <div
      data-key={it.key}
      onClick={(e) => onClick(e, it)}
      onDoubleClick={() => onDoubleClick(it)}
      className={cn(
        "absolute inset-x-0 flex cursor-default items-center gap-3 px-3 text-sm",
        "hover:bg-accent/60",
        selected && "bg-primary/15 hover:bg-primary/20",
        focused && "shadow-[inset_2px_0_0_var(--primary)]",
      )}
      style={{ top, height: ROW_H }}
    >
      <ItemIcon it={it} />
      <div className="flex min-w-0 flex-1 items-center gap-2">
        <span className={cn("truncate", previewing && "font-medium text-primary")}>
          {itemName(it)}
        </span>
        {it.type === "container" && <ContainerBadges c={it.container} compact />}
        {it.type === "model" && <ModelBadges m={it.model} compact />}
        {columns.tags && <TagDots ids={tagIds} />}
        {sub && <span className="shrink-0 text-xs text-muted-foreground">{sub}</span>}
      </div>
      {showLocation && columns.location && (
        <span
          className="w-[30%] min-w-0 shrink-0 truncate text-xs text-muted-foreground"
          title={location}
        >
          {location || "Here"}
        </span>
      )}
      {columns.type && (
        <span className="w-14 shrink-0 truncate text-xs text-muted-foreground">
          {typeLabel(it)}
        </span>
      )}
      {columns.size && (
        <span className="w-20 shrink-0 text-right text-xs text-muted-foreground tabular-nums">
          {size !== null ? formatBytes(size) : ""}
        </span>
      )}
      {columns.modified && (
        <span className="hidden w-24 shrink-0 text-right text-xs text-muted-foreground tabular-nums @[34rem]:block">
          {mtime ? formatDate(mtime) : ""}
        </span>
      )}
    </div>
  );
}

function Tile({ it, selected, focused, previewing, onClick, onDoubleClick }: RowProps) {
  const sub = subtitle(it) ?? (it.type === "model" ? formatBytes(it.model.size) : "");
  return (
    <div
      data-key={it.key}
      onClick={(e) => onClick(e, it)}
      onDoubleClick={() => onDoubleClick(it)}
      className={cn(
        "group flex cursor-default flex-col overflow-hidden rounded-lg border bg-card transition-colors hover:border-primary/40",
        selected && "border-primary/70 bg-primary/10",
        focused && "ring-2 ring-primary/50",
      )}
    >
      <div
        className={cn(
          "relative flex flex-1 items-center justify-center bg-gradient-to-b from-muted/60 to-muted/20",
          previewing && "from-primary/20",
        )}
      >
        <ItemIcon it={it} className="size-10 stroke-[1.25]" />
        {it.type === "model" && (
          <span className="absolute top-1.5 right-1.5 rounded bg-background/70 px-1 text-[10px] font-semibold tracking-wide text-muted-foreground uppercase">
            {it.model.ext}
          </span>
        )}
        <div className="absolute bottom-1.5 left-1.5 flex gap-1">
          {it.type === "container" && <ContainerBadges c={it.container} compact />}
          {it.type === "model" && <ModelBadges m={it.model} compact />}
        </div>
      </div>
      <div className="flex flex-col gap-0.5 border-t px-2 py-1.5">
        <span className="truncate text-xs font-medium" title={itemName(it)}>
          {itemName(it)}
        </span>
        <span className="truncate text-[11px] text-muted-foreground">{sub}</span>
      </div>
    </div>
  );
}

function LoadingRows({ grid }: { grid: boolean }) {
  if (grid) {
    return (
      <div className="grid grid-cols-[repeat(auto-fill,minmax(140px,1fr))] gap-2 p-2">
        {Array.from({ length: 12 }, (_, i) => (
          <Skeleton key={i} className="h-[152px]" />
        ))}
      </div>
    );
  }
  return (
    <div className="flex flex-col gap-2 p-3">
      {Array.from({ length: 14 }, (_, i) => (
        <div key={i} className="flex items-center gap-3">
          <Skeleton className="size-4" />
          <Skeleton className="h-4" style={{ width: `${40 + ((i * 37) % 45)}%` }} />
        </div>
      ))}
    </div>
  );
}

function ListError({ error }: { error: unknown }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 p-8 text-center">
      <TriangleAlert className="size-8 text-destructive" />
      <p className="text-sm font-medium">This location could not be loaded</p>
      <p className="max-w-sm text-xs text-muted-foreground">{errorMessage(error)}</p>
    </div>
  );
}

function EmptyState({ loc, hint }: { loc: string; hint: string }) {
  const libs = useLibraries().data;
  const openSettings = useUi((s) => s.openSettings);
  if (loc === "none") {
    const none = (libs ?? []).filter((l) => !l.isVault).length === 0;
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 p-8 text-center">
        <FolderSearch className="size-10 text-muted-foreground/60" />
        <p className="text-sm font-medium">
          {none ? "Add a library to get started" : "Pick a folder on the left"}
        </p>
        <p className="max-w-xs text-xs text-muted-foreground">
          {none
            ? "Point STL Xplorer at your downloads or model folders. Archives are read in place, nothing is unpacked by hand."
            : "Browse folders and archives, or press / to search everything."}
        </p>
        {none && (
          <Button size="sm" onClick={() => openSettings("libraries")}>
            Add a library
          </Button>
        )}
      </div>
    );
  }
  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 p-8 text-center">
      <SearchX className="size-8 text-muted-foreground/60" />
      <p className="text-sm text-muted-foreground">{hint}</p>
    </div>
  );
}
