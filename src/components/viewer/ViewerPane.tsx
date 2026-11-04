import { useQueryClient } from "@tanstack/react-query";
import {
  Box,
  ChevronDown,
  Crosshair,
  ExternalLink,
  Grid3x3,
  Lock,
  MousePointerClick,
  RotateCcw,
  Save,
  Send,
  ShieldCheck,
  Tag as TagIcon,
  TriangleAlert,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import { Breadcrumbs } from "@/components/browser/Breadcrumbs";
import { VaultBadge } from "@/components/browser/ItemVisuals";
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
import { Badge, Kbd, Progress, Skeleton } from "@/components/ui/misc";
import { Popover, PopoverContent, PopoverTrigger, Tip } from "@/components/ui/overlay";
import { StlCanvas, type Dimensions, type StlCanvasHandle } from "@/components/viewer/StlCanvas";
import { useOpenInSlicer, useVaultSave } from "@/features/actions";
import { usePreview, type PreviewState } from "@/features/mesh";
import { useModelDetail, useSlicers, useTags } from "@/features/queries";
import { toast } from "@/features/toast";
import { api, errorMessage } from "@/lib/api";
import { cn, fileKind, formatBytes, rotate90, sameRotation, ZERO_ROTATION } from "@/lib/utils";
import { useUi } from "@/store/ui";
import type { Rotation, RotationSource } from "@/types";

const SOURCE_LABEL: Record<RotationSource, string> = {
  model: "Saved for this model",
  pack: "Pack default",
  global: "Global default",
  none: "As exported",
};

export function ViewerPane() {
  const qc = useQueryClient();
  const modelId = useUi((s) => s.previewModelId);
  const detailQ = useModelDetail(modelId);
  const detail = detailQ.data && detailQ.data.model.id === modelId ? detailQ.data : undefined;
  const mesh = usePreview(
    detail?.model.id ?? null,
    detail?.model.ext ?? null,
    detail?.model.containerId ?? null,
  );
  const isMeshFile = detail ? fileKind(detail.model.ext) === "mesh" : true;
  const canvas = useRef<StlCanvasHandle>(null);
  const [dims, setDims] = useState<Dimensions | null>(null);
  const [wireframe, setWireframe] = useState(false);
  const [draft, setDraft] = useState<Rotation>(ZERO_ROTATION);

  const resolved = detail?.rotation.rotation ?? ZERO_ROTATION;
  const resolvedKey = `${detail?.model.id}:${resolved.x},${resolved.y},${resolved.z}`;
  // Reset the draft whenever the model or its saved orientation changes.
  const [draftFor, setDraftFor] = useState("");
  if (draftFor !== resolvedKey) {
    setDraftFor(resolvedKey);
    setDraft(resolved);
  }

  const onDims = useCallback((d: Dimensions | null) => setDims(d), []);
  const dirty = detail ? !sameRotation(draft, resolved) : false;

  const save = async (scope: "model" | "pack" | "global", r: Rotation | null) => {
    if (!detail) return;
    try {
      if (scope === "model") await api.setModelRotation([detail.model.id], r);
      else if (scope === "pack" && detail.pack) await api.setPackRotation(detail.pack.id, r);
      else if (scope === "global") await api.setGlobalRotation(r);
      await qc.invalidateQueries({ queryKey: ["model"] });
      void qc.invalidateQueries({ queryKey: ["browse"] });
      void qc.invalidateQueries({ queryKey: ["settings"] });
      if (r) {
        toast.success(
          scope === "model"
            ? "Orientation saved for this model"
            : scope === "pack"
              ? `Orientation saved for ${detail.pack?.name ?? "the pack"}`
              : "Global default orientation saved",
        );
      }
    } catch (e) {
      toast.error("Could not save orientation", errorMessage(e));
    }
  };

  // Viewer shortcuts work wherever focus is, unless the user is typing.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      if (t && (t.isContentEditable || ["INPUT", "TEXTAREA"].includes(t.tagName))) return;
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      if (e.key === "r" || e.key === "R") {
        if (e.shiftKey) setDraft((d) => rotate90(d, "x"));
        else canvas.current?.resetCamera();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  if (modelId === null) return <ViewerEmpty />;

  return (
    <div className="flex h-full min-w-0 flex-col bg-pane">
      <ViewerHeader detail={detail} loading={detailQ.isLoading} />
      {!isMeshFile ? (
        <div className="relative min-h-0 flex-1 bg-viewer">
          {mesh.status === "file" && <FilePreview state={mesh} name={detail?.model.name ?? ""} />}
          <MeshOverlay state={mesh} />
        </div>
      ) : (
        <div className="relative min-h-0 flex-1 bg-viewer">
          <StlCanvas
            ref={canvas}
            geometry={mesh.status === "ready" ? mesh.geometry : null}
            rotation={draft}
            wireframe={wireframe}
            onDimensions={onDims}
          />
          <MeshOverlay state={mesh} />
          {mesh.status === "ready" && (
            <div className="pointer-events-none absolute bottom-3 left-3 flex flex-col gap-1">
              {dims && (
                <div className="rounded-md border bg-background/80 px-2.5 py-1.5 font-mono text-[11px] text-muted-foreground tabular-nums shadow-sm backdrop-blur">
                  <span className="text-red-400">X</span> {dims.x.toFixed(1)} ·{" "}
                  <span className="text-green-400">Y</span> {dims.y.toFixed(1)} ·{" "}
                  <span className="text-sky-400">Z</span> {dims.z.toFixed(1)} mm
                  <span className="ml-2 opacity-70">
                    {dims.triangles.toLocaleString("en-GB")} triangles
                  </span>
                </div>
              )}
            </div>
          )}
          <div className="absolute top-3 right-3 flex flex-col items-end gap-2">
            <div className="flex items-center gap-0.5 rounded-lg border bg-background/85 p-1 shadow-sm backdrop-blur">
              <Tip label="Reset camera (R)">
                <Button
                  variant="ghost"
                  size="icon-xs"
                  onClick={() => canvas.current?.resetCamera()}
                >
                  <Crosshair />
                </Button>
              </Tip>
              <Tip label="Wireframe">
                <Button
                  variant="ghost"
                  size="icon-xs"
                  className={cn(wireframe && "bg-accent text-primary")}
                  onClick={() => setWireframe((w) => !w)}
                >
                  <Grid3x3 />
                </Button>
              </Tip>
            </div>
            {detail && (
              <OrientationControls
                draft={draft}
                setDraft={setDraft}
                dirty={dirty}
                source={detail.rotation.source}
                hasModelOverride={detail.model.rotation !== null}
                packName={detail.pack?.name ?? null}
                hasPackOverride={Boolean(detail.pack?.rotation)}
                onSave={save}
                onRevert={() => setDraft(resolved)}
              />
            )}
          </div>
        </div>
      )}
      {detail && <ViewerFooter modelId={detail.model.id} />}
    </div>
  );
}

function ViewerHeader({
  detail,
  loading,
}: {
  detail: ReturnType<typeof useModelDetail>["data"];
  loading: boolean;
}) {
  if (!detail) {
    return (
      <div className="flex h-11 shrink-0 items-center gap-2 border-b px-3">
        {loading ? <Skeleton className="h-4 w-48" /> : null}
      </div>
    );
  }
  const m = detail.model;
  return (
    <div className="flex h-11 shrink-0 items-center gap-2 border-b px-3">
      <div className="flex min-w-0 flex-1 flex-col justify-center">
        <div className="flex min-w-0 items-center gap-2">
          <span className="truncate text-sm font-medium selectable" title={m.name}>
            {m.name}
          </span>
          <Badge variant="outline" className="uppercase">
            {m.ext}
          </Badge>
          {m.vaulted && <VaultBadge compact />}
          {detail.container.solid && (
            <Tip label="From a solid archive. Later previews from this pack open straight from the cache.">
              <Badge variant="outline">Solid</Badge>
            </Tip>
          )}
        </div>
        <Breadcrumbs crumbs={detail.breadcrumbs} compact className="-ml-1 text-xs" />
      </div>
    </div>
  );
}

function OrientationControls({
  draft,
  setDraft,
  dirty,
  source,
  hasModelOverride,
  packName,
  hasPackOverride,
  onSave,
  onRevert,
}: {
  draft: Rotation;
  setDraft: (r: Rotation) => void;
  dirty: boolean;
  source: RotationSource;
  hasModelOverride: boolean;
  packName: string | null;
  hasPackOverride: boolean;
  onSave: (scope: "model" | "pack" | "global", r: Rotation | null) => void;
  onRevert: () => void;
}) {
  return (
    <div className="flex flex-col items-end gap-1 rounded-lg border bg-background/85 p-1 shadow-sm backdrop-blur">
      <div className="flex items-center gap-0.5">
        {(["x", "y", "z"] as const).map((axis) => (
          <Tip key={axis} label={`Rotate 90° around ${axis.toUpperCase()}`}>
            <Button
              variant="ghost"
              size="xs"
              className="w-12 justify-between font-mono"
              onClick={() => setDraft(rotate90(draft, axis))}
            >
              <span
                className={cn(
                  "font-semibold",
                  axis === "x" ? "text-red-400" : axis === "y" ? "text-green-400" : "text-sky-400",
                )}
              >
                {axis.toUpperCase()}
              </span>
              <span className="text-muted-foreground tabular-nums">{Math.round(draft[axis])}°</span>
            </Button>
          </Tip>
        ))}
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant={dirty ? "default" : "ghost"} size="xs" className="ml-0.5">
              <Save />
              <ChevronDown className="size-3" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-60">
            <DropdownMenuLabel>Save this orientation</DropdownMenuLabel>
            <DropdownMenuItem onSelect={() => onSave("model", draft)}>
              For this model
            </DropdownMenuItem>
            {packName && (
              <DropdownMenuItem onSelect={() => onSave("pack", draft)}>
                <span className="truncate">For every model in {packName}</span>
              </DropdownMenuItem>
            )}
            <DropdownMenuItem onSelect={() => onSave("global", draft)}>
              As the global default
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuLabel>Reset</DropdownMenuLabel>
            {dirty && (
              <DropdownMenuItem onSelect={onRevert}>
                <RotateCcw /> Discard changes
              </DropdownMenuItem>
            )}
            <DropdownMenuItem disabled={!hasModelOverride} onSelect={() => onSave("model", null)}>
              Clear this model&apos;s orientation
            </DropdownMenuItem>
            {packName && (
              <DropdownMenuItem disabled={!hasPackOverride} onSelect={() => onSave("pack", null)}>
                Clear the pack orientation
              </DropdownMenuItem>
            )}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      <span className="px-1.5 pb-0.5 text-[10px] text-muted-foreground">
        {dirty ? "Unsaved orientation" : SOURCE_LABEL[source]}
      </span>
    </div>
  );
}

function FilePreview({
  state,
  name,
}: {
  state: Extract<PreviewState, { status: "file" }>;
  name: string;
}) {
  if (state.kind === "pdf") {
    return <iframe src={state.url} title={name} className="absolute inset-0 size-full border-0" />;
  }
  return (
    <div className="absolute inset-0 flex items-center justify-center p-4">
      <img
        src={state.url}
        alt={name}
        className="max-h-full max-w-full rounded-md object-contain shadow-lg"
        draggable={false}
      />
    </div>
  );
}

function MeshOverlay({ state }: { state: PreviewState }) {
  if (state.status === "loading") {
    return (
      <div className="pointer-events-none absolute inset-0 flex items-center justify-center">
        <div className="flex w-64 flex-col items-center gap-3 rounded-xl border bg-background/85 p-4 shadow-lg backdrop-blur">
          {state.unpack !== null ? (
            <>
              <span className="text-sm font-medium">
                Unpacking pack, {Math.round(state.unpack)}%
              </span>
              <Progress value={state.unpack} />
              <span className="text-center text-xs text-muted-foreground">
                Solid archives unpack once. Every other model in this pack will then open instantly.
              </span>
            </>
          ) : (
            <>
              <Box className="size-6 animate-pulse text-primary" />
              <span className="text-sm text-muted-foreground">Loading…</span>
            </>
          )}
        </div>
      </div>
    );
  }
  if (state.status === "unsupported") {
    return (
      <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 text-center">
        <Box className="size-10 text-muted-foreground/50" />
        <p className="text-sm font-medium">No preview for .{state.ext} files yet</p>
        <p className="max-w-xs text-xs text-muted-foreground">
          You can still send it to a slicer, tag it or save it to the vault.
        </p>
      </div>
    );
  }
  if (state.status === "error") {
    const Icon = state.kind === "encrypted" ? Lock : TriangleAlert;
    return (
      <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 p-6 text-center">
        <Icon
          className={cn(
            "size-9",
            state.kind === "encrypted" ? "text-muted-foreground" : "text-warning",
          )}
        />
        <p className="text-sm font-medium">
          {state.kind === "missing_parts"
            ? "Part of this archive is missing"
            : state.kind === "encrypted"
              ? "This archive is password protected"
              : state.kind === "too_large"
                ? "Too large to preview"
                : "Could not preview this file"}
        </p>
        <p className="max-w-sm text-xs text-muted-foreground">{state.message}</p>
      </div>
    );
  }
  return null;
}

function ViewerFooter({ modelId }: { modelId: number }) {
  const detail = useModelDetail(modelId).data;
  const slicers = useSlicers().data ?? [];
  const tags = useTags().data ?? [];
  const open = useOpenInSlicer();
  const vault = useVaultSave();
  const openSettings = useUi((s) => s.openSettings);
  if (!detail) return null;
  const m = detail.model;
  const def = slicers.find((s) => s.isDefault);
  const isMeshFile = fileKind(m.ext) === "mesh";
  const modelTags = m.tagIds
    .map((id) => tags.find((t) => t.id === id))
    .filter((t): t is NonNullable<typeof t> => Boolean(t));

  return (
    <div className="flex shrink-0 flex-col gap-2 border-t px-3 py-2.5">
      <div className="flex items-center gap-2">
        <div className="flex min-w-0 flex-1 flex-wrap items-center gap-1">
          {modelTags.map((t) => (
            <span
              key={t.id}
              className="inline-flex h-5 items-center gap-1 rounded-full border px-2 text-[11px]"
            >
              <span className="size-1.5 rounded-full" style={{ backgroundColor: t.colour }} />
              {t.name}
            </span>
          ))}
          <Popover>
            <PopoverTrigger asChild>
              <Button
                variant="ghost"
                size="xs"
                className="h-5 px-1.5 text-[11px] text-muted-foreground"
              >
                <TagIcon className="size-3" />
                {modelTags.length === 0 ? "Add tag" : "Edit"}
              </Button>
            </PopoverTrigger>
            <PopoverContent align="start" className="w-64">
              <TagPicker modelIds={[m.id]} containerIds={[]} currentTagIds={[m.tagIds]} />
            </PopoverContent>
          </Popover>
        </div>
        <span className="shrink-0 text-xs text-muted-foreground tabular-nums">
          {formatBytes(m.size)}
        </span>
      </div>
      <div className="flex items-center gap-1.5">
        {!isMeshFile ? (
          <Button
            size="sm"
            className="flex-1"
            disabled={m.encrypted}
            onClick={() =>
              void api
                .openExternally(m.id)
                .catch((e: unknown) => toast.error("Could not open the file", errorMessage(e)))
            }
          >
            <ExternalLink />
            Open with default app
          </Button>
        ) : (
          <Button
            size="sm"
            className="flex-1"
            disabled={m.encrypted || open.isPending}
            onClick={() => {
              if (!def) {
                toast.info("No slicer set up yet", "Add one in Settings → Slicers.");
                openSettings("slicers");
                return;
              }
              open.mutate({ modelIds: [m.id] });
            }}
          >
            <Send />
            {def ? `Open in ${def.name}` : "Set up a slicer"}
            <Kbd className="ml-1 border-primary-foreground/30 bg-primary-foreground/15 text-primary-foreground">
              ↵
            </Kbd>
          </Button>
        )}
        {!m.vaulted && (
          <Tip label="Move into the vault so the downloads clean-up cannot delete it">
            <Button
              variant="outline"
              size="sm"
              disabled={vault.isPending}
              onClick={() => vault.mutate({ modelIds: [m.id], containerIds: [] })}
            >
              <ShieldCheck />
              Save
            </Button>
          </Tip>
        )}
      </div>
      {m.originalPath && (
        <p className="truncate text-[11px] text-muted-foreground" title={m.originalPath}>
          Saved from {m.originalPath}
        </p>
      )}
    </div>
  );
}

function ViewerEmpty() {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-4 bg-viewer p-8 text-center">
      <div className="flex size-14 items-center justify-center rounded-2xl border bg-background/60">
        <MousePointerClick className="size-6 text-muted-foreground" />
      </div>
      <div className="flex flex-col gap-1">
        <p className="text-sm font-medium">Select a file to preview it</p>
        <p className="max-w-xs text-xs text-muted-foreground">
          Models, images and PDFs inside zip, 7z and rar archives preview without unpacking
          anything.
        </p>
      </div>
      <div className="grid grid-cols-[auto_1fr] items-center gap-x-3 gap-y-1.5 text-left text-xs text-muted-foreground">
        <Kbd>↑ ↓</Kbd> <span>Move through files</span>
        <Kbd>Space</Kbd> <span>Add to selection</span>
        <Kbd>Enter</Kbd> <span>Open in default slicer</span>
        <Kbd>/</Kbd> <span>Search</span>
        <Kbd>Ctrl K</Kbd> <span>Commands</span>
      </div>
    </div>
  );
}
