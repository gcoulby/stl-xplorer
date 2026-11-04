import { useQueries, useQueryClient } from "@tanstack/react-query";
import { useVirtualizer } from "@tanstack/react-virtual";
import {
  ChevronRight,
  EyeOff,
  FolderOpen,
  FolderSearch,
  FolderX,
  Pencil,
  FolderPlus,
  Layers,
  LoaderCircle,
  MoreHorizontal,
  Plus,
  RefreshCw,
  Settings2,
  ShieldCheck,
  Tags as TagsIcon,
  TriangleAlert,
} from "lucide-react";
import { useMemo, useRef, useState } from "react";

import { ItemMenuContent } from "@/components/browser/ItemMenu";
import { ArchiveDirIcon, ContainerIcon } from "@/components/browser/ItemVisuals";
import { Button } from "@/components/ui/button";
import { TAG_COLOURS } from "@/components/browser/TagPicker";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
  ContextMenuTrigger,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/menu";
import { Skeleton } from "@/components/ui/misc";
import { Tip } from "@/components/ui/overlay";
import { containerKey, dirKey, type ListItem } from "@/features/listing";
import { usePacks } from "@/features/packs";
import {
  browseQuery,
  invalidateIndex,
  invalidateTags,
  qk,
  useCollections,
  useLibraries,
  useTags,
} from "@/features/queries";
import { describeSelection, usePrompt } from "@/features/selection";
import { toast } from "@/features/toast";
import { api, errorMessage } from "@/lib/api";
import { cn } from "@/lib/utils";
import { treeKey, useUi } from "@/store/ui";
import type { BrowseResult, ContainerInfo, Library } from "@/types";

interface TreeRow {
  key: string;
  depth: number;
  label: string;
  containerId: number;
  subpath: string;
  kind: "library" | "folder" | "archive" | "dir" | "loading";
  expandable: boolean;
  library?: Library;
  container?: ContainerInfo;
  count?: number;
  issues?: number;
}

const TREE_ROW_H = 28;

function parseTreeKey(k: string): [number, string] {
  const i = k.indexOf(":");
  return [Number(k.slice(0, i)), k.slice(i + 1)];
}

function useTreeRows(libs: Library[]): TreeRow[] {
  const expanded = useUi((s) => s.expanded);
  const results = useQueries({
    queries: expanded.map((k) => {
      const [cid, sub] = parseTreeKey(k);
      return browseQuery(cid, sub);
    }),
  });
  const data = new Map<string, BrowseResult>();
  expanded.forEach((k, i) => {
    const d = results[i]?.data;
    if (d) data.set(k, d);
  });
  const dataKey = results.map((r) => r.dataUpdatedAt).join(",");

  return useMemo(() => {
    const rows: TreeRow[] = [];
    const open = new Set(expanded);
    const addChildren = (cid: number, sub: string, depth: number, guard: number) => {
      if (guard > 64) return;
      const d = data.get(treeKey(cid, sub));
      if (!d) {
        rows.push({
          key: `loading:${cid}:${sub}`,
          depth,
          label: "",
          containerId: cid,
          subpath: sub,
          kind: "loading",
          expandable: false,
        });
        return;
      }
      for (const dir of d.dirs) {
        const key = treeKey(cid, dir.path);
        rows.push({
          key,
          depth,
          label: dir.name,
          containerId: cid,
          subpath: dir.path,
          kind: "dir",
          expandable: dir.hasChildren,
          count: dir.modelCount,
        });
        if (open.has(key)) addChildren(cid, dir.path, depth + 1, guard + 1);
      }
      for (const c of d.containers) {
        const key = treeKey(c.id, "");
        rows.push({
          key,
          depth,
          label: c.name,
          containerId: c.id,
          subpath: "",
          kind: c.kind === "folder" ? "folder" : "archive",
          expandable: c.hasChildren,
          container: c,
          count: c.modelCount,
          issues: c.issueCount,
        });
        if (open.has(key) && c.hasChildren) addChildren(c.id, "", depth + 1, guard + 1);
      }
    };
    for (const lib of libs) {
      if (lib.rootContainerId === null) {
        rows.push({
          key: `lib:${lib.id}`,
          depth: 0,
          label: lib.name,
          containerId: -1,
          subpath: "",
          kind: "library",
          expandable: false,
          library: lib,
        });
        continue;
      }
      const key = treeKey(lib.rootContainerId, "");
      rows.push({
        key,
        depth: 0,
        label: lib.name,
        containerId: lib.rootContainerId,
        subpath: "",
        kind: "library",
        expandable: true,
        library: lib,
        count: lib.modelCount,
        issues: lib.issueCount,
      });
      if (open.has(key)) addChildren(lib.rootContainerId, "", 1, 0);
    }
    return rows;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [libs, expanded, dataKey]);
}

export function Sidebar() {
  return (
    <div className="flex h-full min-w-0 flex-col bg-pane">
      <LibraryTree />
      <CollectionsSection />
      <TagsSection />
    </div>
  );
}

function SectionHeader({ title, children }: { title: string; children?: React.ReactNode }) {
  return (
    <div className="flex h-8 shrink-0 items-center justify-between px-3">
      <span className="text-[11px] font-semibold tracking-wider text-muted-foreground uppercase">
        {title}
      </span>
      <div className="flex items-center gap-0.5">{children}</div>
    </div>
  );
}

function LibraryTree() {
  const libsQ = useLibraries();
  const libs = useMemo(() => (libsQ.data ?? []).filter((l) => l.enabled), [libsQ.data]);
  const rows = useTreeRows(libs);
  const loc = useUi((s) => s.location);
  const scans = useUi((s) => s.scans);
  const { navigate, toggleExpanded, openSettings } = useUi.getState();
  const expanded = useUi((s) => s.expanded);
  const ref = useRef<HTMLDivElement>(null);
  const [cursor, setCursor] = useState<string | null>(null);
  const [menuRow, setMenuRow] = useState<TreeRow | null>(null);
  const virt = useVirtualizer({
    count: rows.length,
    getScrollElement: () => ref.current,
    estimateSize: () => TREE_ROW_H,
    overscan: 16,
  });

  const activeKey = loc.kind === "container" ? treeKey(loc.containerId, loc.subpath) : null;

  const go = (r: TreeRow) => {
    if (r.kind === "loading" || r.containerId < 0) return;
    navigate({ kind: "container", containerId: r.containerId, subpath: r.subpath });
    if (r.expandable) toggleExpanded(r.key, true);
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    const i = rows.findIndex((r) => r.key === (cursor ?? activeKey));
    const move = (d: number) => {
      e.preventDefault();
      const next = rows[Math.max(0, Math.min(rows.length - 1, (i < 0 ? 0 : i) + d))];
      if (next && next.kind !== "loading") {
        setCursor(next.key);
        virt.scrollToIndex(rows.indexOf(next));
      }
    };
    const r = i >= 0 ? rows[i] : undefined;
    switch (e.key) {
      case "ArrowDown":
        return move(1);
      case "ArrowUp":
        return move(-1);
      case "ArrowRight":
        e.preventDefault();
        if (r?.expandable) toggleExpanded(r.key, true);
        return;
      case "ArrowLeft":
        e.preventDefault();
        if (r && expanded.includes(r.key)) toggleExpanded(r.key, false);
        return;
      case "Enter":
        e.preventDefault();
        if (r) go(r);
        return;
    }
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <SectionHeader title="Libraries">
        <Tip label="Rescan all">
          <Button
            variant="ghost"
            size="icon-xs"
            className="size-6"
            onClick={() =>
              void api
                .rescanAll()
                .catch((e: unknown) => toast.error("Rescan failed", errorMessage(e)))
            }
          >
            <RefreshCw
              className={cn("size-3.5", Object.keys(scans).length > 0 && "animate-spin")}
            />
          </Button>
        </Tip>
        <Tip label="Add library">
          <Button
            variant="ghost"
            size="icon-xs"
            className="size-6"
            onClick={() => openSettings("libraries")}
          >
            <FolderPlus className="size-3.5" />
          </Button>
        </Tip>
      </SectionHeader>
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div
            ref={ref}
            tabIndex={0}
            onKeyDown={onKeyDown}
            onBlur={() => setCursor(null)}
            onContextMenu={(e) => {
              const key = (e.target as HTMLElement)
                .closest<HTMLElement>("[data-row-key]")
                ?.getAttribute("data-row-key");
              const row = rows.find((r) => r.key === key && r.kind !== "loading");
              // No row under the pointer: no menu at all, not even the browser's.
              if (!row) {
                e.preventDefault();
                return;
              }
              setMenuRow(row);
            }}
            className="min-h-0 flex-1 overflow-y-auto px-1.5 pb-2 outline-none"
          >
            {libsQ.isLoading ? (
              <div className="flex flex-col gap-2 p-2">
                {[60, 45, 70].map((w) => (
                  <Skeleton key={w} className="h-5" style={{ width: `${w}%` }} />
                ))}
              </div>
            ) : libs.length === 0 ? (
              <div className="px-2 py-4 text-xs text-muted-foreground">
                No libraries yet.{" "}
                <button
                  className="text-primary hover:underline"
                  onClick={() => openSettings("libraries")}
                >
                  Add one
                </button>
              </div>
            ) : (
              <div style={{ height: virt.getTotalSize(), position: "relative" }}>
                {virt.getVirtualItems().map((vr) => {
                  const r = rows[vr.index];
                  if (!r) return null;
                  return (
                    <TreeRowView
                      key={r.key}
                      r={r}
                      top={vr.start}
                      active={r.key === activeKey}
                      cursor={r.key === cursor}
                      open={expanded.includes(r.key)}
                      scanning={r.library ? Boolean(scans[r.library.id]) : false}
                      onToggle={() => toggleExpanded(r.key)}
                      onGo={() => go(r)}
                    />
                  );
                })}
              </div>
            )}
          </div>
        </ContextMenuTrigger>
        {menuRow && <TreeMenu row={menuRow} onGo={() => go(menuRow)} />}
      </ContextMenu>
    </div>
  );
}

function rowItem(r: TreeRow): ListItem | null {
  if (r.container)
    return { key: containerKey(r.container.id), type: "container", container: r.container };
  if (r.kind === "dir")
    return {
      key: dirKey(r.containerId, r.subpath),
      type: "dir",
      containerId: r.containerId,
      dir: {
        name: r.label,
        path: r.subpath,
        modelCount: r.count ?? 0,
        fileCount: r.count ?? 0,
        hasChildren: r.expandable,
      },
    };
  return null;
}

/** Right-click menu for a tree row: the same actions as the file list, plus library ones. */
function TreeMenu({ row, onGo }: { row: TreeRow; onGo: () => void }) {
  const item = rowItem(row);
  if (row.library) return <LibraryMenu lib={row.library} onGo={onGo} />;
  if (!item) return null;
  return <ItemMenuContent sel={describeSelection([item])} onActivate={onGo} />;
}

function LibraryMenu({ lib, onGo }: { lib: Library; onGo: () => void }) {
  const qc = useQueryClient();
  const ask = usePrompt((s) => s.ask);
  const openSettings = useUi((s) => s.openSettings);
  const findUnpacked = usePacks((s) => s.findUnpacked);
  const root = lib.rootContainerId;
  const fail = (title: string) => (e: unknown) => toast.error(title, errorMessage(e));
  return (
    <ContextMenuContent className="w-60">
      <ContextMenuLabel className="truncate">{lib.name}</ContextMenuLabel>
      {root !== null && (
        <ContextMenuItem onSelect={onGo}>
          <FolderOpen /> Open
        </ContextMenuItem>
      )}
      <ContextMenuItem onSelect={() => void api.rescanLibrary(lib.id).catch(fail("Rescan failed"))}>
        <RefreshCw /> Rescan
      </ContextMenuItem>
      {root !== null && (
        <ContextMenuItem
          onSelect={() =>
            void api.revealInFolder({ containerId: root }).catch(fail("Could not show it"))
          }
        >
          <FolderSearch /> Show in folder
        </ContextMenuItem>
      )}
      {root !== null && (
        <ContextMenuItem onSelect={() => findUnpacked({ id: root, name: lib.name })}>
          <FolderX /> Find unpacked copies…
        </ContextMenuItem>
      )}
      <ContextMenuSeparator />
      <ContextMenuItem
        onSelect={() =>
          ask({
            title: "Rename library",
            initial: lib.name,
            onSubmit: (name) =>
              void api
                .updateLibrary(lib.id, { name })
                .then(() => invalidateIndex(qc))
                .catch(fail("Could not rename")),
          })
        }
      >
        <Pencil /> Rename…
      </ContextMenuItem>
      {!lib.isVault && (
        <ContextMenuItem
          onSelect={() =>
            void api
              .updateLibrary(lib.id, { enabled: false })
              .then(() => {
                invalidateIndex(qc);
                toast.info(`${lib.name} hidden`, "Turn it back on in Settings → Libraries.");
              })
              .catch(fail("Could not hide it"))
          }
        >
          <EyeOff /> Hide from the sidebar
        </ContextMenuItem>
      )}
      <ContextMenuItem onSelect={() => openSettings("libraries")}>
        <Settings2 /> Library settings…
      </ContextMenuItem>
    </ContextMenuContent>
  );
}

function TreeRowView({
  r,
  top,
  active,
  cursor,
  open,
  scanning,
  onToggle,
  onGo,
}: {
  r: TreeRow;
  top: number;
  active: boolean;
  cursor: boolean;
  open: boolean;
  scanning: boolean;
  onToggle: () => void;
  onGo: () => void;
}) {
  if (r.kind === "loading") {
    return (
      <div
        className="absolute inset-x-0 flex items-center"
        style={{ top, height: TREE_ROW_H, paddingLeft: 8 + r.depth * 14 + 18 }}
      >
        <Skeleton className="h-3.5 w-24" />
      </div>
    );
  }
  const lib = r.library;
  return (
    <div
      onClick={onGo}
      data-row-key={r.key}
      className={cn(
        "group absolute inset-x-0 flex cursor-default items-center gap-1.5 rounded-md pr-2 text-[13px]",
        "hover:bg-accent/70",
        active && "bg-primary/15 text-foreground hover:bg-primary/20",
        cursor && "ring-1 ring-primary/50",
        r.kind === "library" && "font-medium",
      )}
      style={{ top, height: TREE_ROW_H, paddingLeft: 4 + r.depth * 14 }}
    >
      <button
        type="button"
        tabIndex={-1}
        onClick={(e) => {
          e.stopPropagation();
          onToggle();
        }}
        className={cn(
          "flex size-4 shrink-0 items-center justify-center rounded text-muted-foreground hover:text-foreground",
          !r.expandable && "invisible",
        )}
        aria-label={open ? "Collapse" : "Expand"}
      >
        <ChevronRight className={cn("size-3.5 transition-transform", open && "rotate-90")} />
      </button>
      {r.kind === "library" ? (
        lib?.isVault ? (
          <ShieldCheck className="size-4 shrink-0 text-success" />
        ) : (
          <ContainerIcon container={{ kind: "folder", isPack: false, vaulted: false }} library />
        )
      ) : r.kind === "dir" ? (
        <ArchiveDirIcon />
      ) : r.container ? (
        <ContainerIcon container={r.container} />
      ) : null}
      <span className="min-w-0 flex-1 truncate">{r.label}</span>
      {scanning && (
        <LoaderCircle className="size-3.5 shrink-0 animate-spin text-muted-foreground" />
      )}
      {lib && !lib.pathExists && (
        <Tip label="Folder not found. Is the drive connected?">
          <TriangleAlert className="size-3.5 shrink-0 text-warning" />
        </Tip>
      )}
      {(r.issues ?? 0) > 0 && (
        <Tip label={`${r.issues} archive${r.issues === 1 ? "" : "s"} need attention`}>
          <span className="size-1.5 shrink-0 rounded-full bg-warning" />
        </Tip>
      )}
      {r.count !== undefined && r.count > 0 && (
        <span className="shrink-0 text-[11px] text-muted-foreground tabular-nums">
          {r.count.toLocaleString("en-GB")}
        </span>
      )}
    </div>
  );
}

function CollectionsSection() {
  const qc = useQueryClient();
  const cols = useCollections().data ?? [];
  const loc = useUi((s) => s.location);
  const navigate = useUi((s) => s.navigate);
  const ask = usePrompt((s) => s.ask);
  const create = () =>
    ask({
      title: "New collection",
      placeholder: "e.g. Dungeon tiles",
      onSubmit: (name) =>
        void api
          .createCollection(name)
          .then(() => qc.invalidateQueries({ queryKey: qk.collections }))
          .catch((e: unknown) => toast.error("Could not create collection", errorMessage(e))),
    });
  return (
    <div className="max-h-[30%] shrink-0 overflow-y-auto border-t pb-1">
      <SectionHeader title="Collections">
        <Tip label="New collection">
          <Button variant="ghost" size="icon-xs" className="size-6" onClick={create}>
            <Plus className="size-3.5" />
          </Button>
        </Tip>
      </SectionHeader>
      {cols.length === 0 && (
        <p className="px-3 pb-2 text-xs text-muted-foreground">
          Group packs into collections from their right-click menu.
        </p>
      )}
      <div className="px-1.5">
        {cols.map((c) => {
          const active = loc.kind === "collection" && loc.collectionId === c.id;
          return (
            <div
              key={c.id}
              onClick={() => navigate({ kind: "collection", collectionId: c.id })}
              className={cn(
                "group flex h-7 cursor-default items-center gap-2 rounded-md px-2 text-[13px] hover:bg-accent/70",
                active && "bg-primary/15",
              )}
            >
              <Layers className="size-4 shrink-0 text-violet-400" />
              <span className="flex-1 truncate">{c.name}</span>
              <span className="text-[11px] text-muted-foreground tabular-nums group-hover:hidden">
                {c.itemCount}
              </span>
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <button
                    type="button"
                    onClick={(e) => e.stopPropagation()}
                    className="hidden size-5 items-center justify-center rounded hover:bg-background group-hover:flex"
                    aria-label="Collection options"
                  >
                    <MoreHorizontal className="size-3.5" />
                  </button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                  <DropdownMenuItem
                    onSelect={() =>
                      ask({
                        title: "Rename collection",
                        initial: c.name,
                        onSubmit: (name) =>
                          void api
                            .renameCollection(c.id, name)
                            .then(() => qc.invalidateQueries({ queryKey: qk.collections })),
                      })
                    }
                  >
                    Rename…
                  </DropdownMenuItem>
                  <DropdownMenuSeparator />
                  <DropdownMenuItem
                    variant="destructive"
                    onSelect={() =>
                      void api.deleteCollection(c.id).then(() => {
                        void qc.invalidateQueries({ queryKey: qk.collections });
                        if (active) navigate({ kind: "none" });
                      })
                    }
                  >
                    Delete collection
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
          );
        })}
      </div>
    </div>
  );
}

function TagsSection() {
  const qc = useQueryClient();
  const tags = useTags().data ?? [];
  const loc = useUi((s) => s.location);
  const navigate = useUi((s) => s.navigate);
  const toggleTag = useUi((s) => s.toggleTag);
  const openSettings = useUi((s) => s.openSettings);
  const ask = usePrompt((s) => s.ask);
  return (
    <div className="max-h-[25%] shrink-0 overflow-y-auto border-t pb-2">
      <SectionHeader title="Tags">
        <Tip label="All tags">
          <Button
            variant="ghost"
            size="icon-xs"
            className={cn("size-6", loc.kind === "tags" && "bg-accent")}
            onClick={() => navigate({ kind: "tags" })}
          >
            <TagsIcon className="size-3.5" />
          </Button>
        </Tip>
        <Tip label="Auto tag rules">
          <Button
            variant="ghost"
            size="icon-xs"
            className="size-6"
            onClick={() => openSettings("tags")}
          >
            <Settings2 className="size-3.5" />
          </Button>
        </Tip>
      </SectionHeader>
      {tags.length === 0 && (
        <p className="px-3 text-xs text-muted-foreground">
          Tag files from their right-click menu, or{" "}
          <button className="text-primary hover:underline" onClick={() => openSettings("tags")}>
            tag them from their folder names
          </button>
          .
        </p>
      )}
      <div className="flex flex-wrap gap-1 px-3">
        {tags.map((t) => {
          const active = loc.kind === "tag" && loc.tagId === t.id;
          return (
            <ContextMenu key={t.id}>
              <ContextMenuTrigger asChild>
                <button
                  type="button"
                  aria-pressed={active}
                  onClick={() => toggleTag(t.id)}
                  className={cn(
                    "inline-flex h-6 items-center gap-1.5 rounded-full border px-2 text-xs hover:bg-accent",
                    active && "border-primary/60 bg-primary/15",
                  )}
                >
                  <span className="size-2 rounded-full" style={{ backgroundColor: t.colour }} />
                  {t.name}
                  <span className="text-muted-foreground tabular-nums">{t.usage}</span>
                </button>
              </ContextMenuTrigger>
              <ContextMenuContent>
                <ContextMenuItem
                  onSelect={() =>
                    ask({
                      title: "Rename tag",
                      initial: t.name,
                      onSubmit: (name) =>
                        void api
                          .updateTag(t.id, { name })
                          .then(() => invalidateTags(qc))
                          .catch((e: unknown) =>
                            toast.error("Could not rename tag", errorMessage(e)),
                          ),
                    })
                  }
                >
                  Rename…
                </ContextMenuItem>
                <ContextMenuSub>
                  <ContextMenuSubTrigger>Colour</ContextMenuSubTrigger>
                  <ContextMenuSubContent>
                    {TAG_COLOURS.map((c) => (
                      <ContextMenuItem
                        key={c}
                        onSelect={() =>
                          void api.updateTag(t.id, { colour: c }).then(() => invalidateTags(qc))
                        }
                      >
                        <span className="size-3 rounded-full" style={{ backgroundColor: c }} />
                        {c === t.colour ? "Current" : ""}
                      </ContextMenuItem>
                    ))}
                  </ContextMenuSubContent>
                </ContextMenuSub>
                <ContextMenuSeparator />
                <ContextMenuItem
                  variant="destructive"
                  onSelect={() =>
                    void api.deleteTag(t.id).then(() => {
                      invalidateTags(qc);
                      if (active) navigate({ kind: "none" });
                    })
                  }
                >
                  Delete tag
                </ContextMenuItem>
              </ContextMenuContent>
            </ContextMenu>
          );
        })}
      </div>
    </div>
  );
}
