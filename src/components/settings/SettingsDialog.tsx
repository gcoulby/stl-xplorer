import { useMutation, useQueryClient } from "@tanstack/react-query";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  Archive,
  Brush,
  CircleCheck,
  FolderOpen,
  FolderPlus,
  FolderX,
  Library as LibraryIcon,
  Pencil,
  Plus,
  RefreshCw,
  Rotate3d,
  ShieldCheck,
  SlidersHorizontal,
  Star,
  Tags as TagsIcon,
  Trash2,
  TriangleAlert,
  Wrench,
  ArrowDownToLine,
  X,
} from "lucide-react";
import { useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge, Label, Skeleton, Switch } from "@/components/ui/misc";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  Tip,
} from "@/components/ui/overlay";
import { TAG_COLOURS } from "@/components/browser/TagPicker";
import { usePacks } from "@/features/packs";
import {
  invalidateIndex,
  invalidateTags,
  qk,
  useCacheStats,
  useLibraries,
  useSettings,
  useSevenZip,
  useSlicerPresets,
  useSlicers,
  useTagSuggestions,
  useTags,
} from "@/features/queries";
import { toast } from "@/features/toast";
import { appVersion, useUpdater } from "@/features/updater";
import { api, errorMessage } from "@/lib/api";
import {
  cn,
  formatBytes,
  joinArgs,
  plural,
  rotate90,
  splitArgs,
  timeAgo,
  ZERO_ROTATION,
} from "@/lib/utils";
import { useUi, type SettingsTab, type Theme } from "@/store/ui";
import type { Rotation, Slicer, Tag } from "@/types";

const TABS: Array<{ id: SettingsTab; label: string; icon: React.ReactNode }> = [
  { id: "libraries", label: "Libraries", icon: <LibraryIcon /> },
  { id: "slicers", label: "Slicers", icon: <SlidersHorizontal /> },
  { id: "archives", label: "Archives", icon: <Archive /> },
  { id: "tags", label: "Tags", icon: <TagsIcon /> },
  { id: "orientation", label: "Orientation", icon: <Rotate3d /> },
  { id: "vault", label: "Vault", icon: <ShieldCheck /> },
  { id: "appearance", label: "Appearance", icon: <Brush /> },
  { id: "maintenance", label: "Maintenance", icon: <Wrench /> },
];

export function SettingsDialog() {
  const open = useUi((s) => s.settingsOpen);
  const tab = useUi((s) => s.settingsTab);
  const close = useUi((s) => s.closeSettings);
  const openSettings = useUi((s) => s.openSettings);
  return (
    <Dialog open={open} onOpenChange={(o) => !o && close()}>
      <DialogContent className="h-[min(640px,90vh)] gap-0 overflow-hidden p-0 sm:max-w-4xl">
        <Tabs
          value={tab}
          onValueChange={(v) => openSettings(v as SettingsTab)}
          orientation="vertical"
          className="flex h-full min-h-0"
        >
          <div className="flex w-48 shrink-0 flex-col gap-1 border-r bg-pane p-3">
            <DialogTitle className="px-2 pt-1 pb-3 text-base">Settings</DialogTitle>
            <DialogDescription className="sr-only">Application settings</DialogDescription>
            <TabsList className="flex h-auto flex-col items-stretch gap-0.5 bg-transparent p-0">
              {TABS.map((t) => (
                <TabsTrigger
                  key={t.id}
                  value={t.id}
                  className="justify-start gap-2 px-2 py-1.5 data-[state=active]:bg-accent data-[state=active]:shadow-none dark:data-[state=active]:bg-accent"
                >
                  {t.icon}
                  {t.label}
                </TabsTrigger>
              ))}
            </TabsList>
          </div>
          {/* Top padding keeps section actions clear of the dialog's close button. */}
          <div className="min-h-0 flex-1 overflow-y-auto px-6 pt-12 pb-6">
            <TabsContent value="libraries">
              <LibrariesTab />
            </TabsContent>
            <TabsContent value="slicers">
              <SlicersTab />
            </TabsContent>
            <TabsContent value="archives">
              <ArchivesTab />
            </TabsContent>
            <TabsContent value="tags">
              <TagsTab />
            </TabsContent>
            <TabsContent value="orientation">
              <OrientationTab />
            </TabsContent>
            <TabsContent value="vault">
              <VaultTab />
            </TabsContent>
            <TabsContent value="appearance">
              <AppearanceTab />
            </TabsContent>
            <TabsContent value="maintenance">
              <MaintenanceTab />
            </TabsContent>
          </div>
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}

function Section({
  title,
  description,
  action,
  children,
}: {
  title: string;
  description?: React.ReactNode;
  action?: React.ReactNode;
  children?: React.ReactNode;
}) {
  return (
    <section className="mb-8 last:mb-0">
      <div className="mb-3 flex items-start justify-between gap-4">
        <div>
          <h3 className="text-sm font-semibold">{title}</h3>
          {description && <p className="mt-1 text-xs text-muted-foreground">{description}</p>}
        </div>
        {action}
      </div>
      {children}
    </section>
  );
}

function TagsTab() {
  const qc = useQueryClient();
  const tags = useTags();
  const suggestions = useTagSuggestions(true);
  const [name, setName] = useState("");
  const [keywords, setKeywords] = useState("");
  const refresh = () => {
    invalidateTags(qc);
    void qc.invalidateQueries({ queryKey: qk.suggestions });
    void qc.invalidateQueries({ queryKey: qk.settings });
  };

  const create = async (tagName: string, kw: string) => {
    try {
      const all = tags.data ?? [];
      await api.createTag(tagName, TAG_COLOURS[all.length % TAG_COLOURS.length], kw);
      refresh();
      toast.success(`Tag “${tagName}” added`, kw ? "Matching files are tagged now." : undefined);
    } catch (e) {
      toast.error("Could not add tag", errorMessage(e));
    }
  };

  return (
    <>
      <Section
        title="Auto tags"
        action={<AutoTagSwitch />}
        description={
          <>
            A tag with keywords goes on every file whose folder, archive or file name contains one
            of them, and stays up to date as new files are indexed. Separate keywords with commas.
            End one with * to match the start of a word, so <code>ork*</code> also finds orks and
            orkz. Tags you add by hand are never removed.
          </>
        }
      >
        <form
          className="mb-3 flex gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            if (!name.trim()) return;
            void create(name.trim(), keywords.trim() || name.trim().toLowerCase());
            setName("");
            setKeywords("");
          }}
        >
          <Input
            placeholder="Tag name, e.g. Orks"
            value={name}
            onChange={(e) => setName(e.target.value)}
            className="w-44"
          />
          <Input
            placeholder="Keywords, e.g. ork*, goff, snakebite"
            value={keywords}
            onChange={(e) => setKeywords(e.target.value)}
            className="flex-1"
          />
          <Button type="submit" size="sm" disabled={!name.trim()}>
            <Plus /> Add
          </Button>
        </form>
        {tags.isLoading && <Skeleton className="h-16" />}
        {(tags.data ?? []).length > 0 && (
          <div className="flex flex-col divide-y rounded-lg border">
            {(tags.data ?? []).map((t) => (
              <TagRuleRow key={`${t.id}:${t.keywords ?? ""}`} tag={t} onChanged={refresh} />
            ))}
          </div>
        )}
      </Section>
      <Section
        title="Suggestions"
        description="Words that turn up in many of your folder and archive names. Click one to make it an auto tag, or dismiss it with ×."
        action={<SuggestionsSwitch />}
      >
        {suggestions.isLoading ? (
          <Skeleton className="h-16" />
        ) : (suggestions.data ?? []).length === 0 ? (
          <p className="text-xs text-muted-foreground">No suggestions right now.</p>
        ) : (
          <div className="flex flex-wrap gap-1.5">
            {(suggestions.data ?? []).map((s) => (
              <span
                key={s.word}
                className="inline-flex h-7 items-center rounded-full border text-xs hover:border-primary/50"
              >
                <button
                  type="button"
                  onClick={() =>
                    void create(s.word.charAt(0).toUpperCase() + s.word.slice(1), s.word)
                  }
                  className="inline-flex h-full items-center gap-1.5 rounded-l-full pr-1 pl-2.5 hover:bg-accent"
                >
                  <Plus className="size-3 text-muted-foreground" />
                  {s.word}
                  <span className="text-muted-foreground tabular-nums">{s.containers}</span>
                </button>
                <button
                  type="button"
                  aria-label={`Dismiss ${s.word}`}
                  onClick={() =>
                    void api
                      .dismissSuggestion(s.word)
                      .then(refresh)
                      .catch((e: unknown) => toast.error("Could not dismiss", errorMessage(e)))
                  }
                  className="inline-flex h-full items-center rounded-r-full pr-2 pl-1 text-muted-foreground hover:bg-accent hover:text-foreground"
                >
                  <X className="size-3" />
                </button>
              </span>
            ))}
          </div>
        )}
        <DismissedNote onChanged={refresh} />
      </Section>
    </>
  );
}

function SuggestionsSwitch() {
  const qc = useQueryClient();
  const settings = useSettings();
  const [busy, setBusy] = useState(false);
  const on = settings.data?.autotagSuggestions ?? false;
  return (
    <Label className="flex shrink-0 flex-col items-end gap-1 text-xs font-normal">
      <span className="flex items-center gap-2 whitespace-nowrap">
        Create these tags automatically
        <Switch
          checked={on}
          disabled={!settings.data || busy}
          onCheckedChange={(v) => {
            setBusy(true);
            void api
              .setAutotagSuggestions(v)
              .then((n) => {
                invalidateTags(qc);
                void qc.invalidateQueries({ queryKey: qk.settings });
                void qc.invalidateQueries({ queryKey: qk.suggestions });
                if (v) toast.success("Suggested tags created", `${plural(n, "file")} tagged.`);
              })
              .catch((e: unknown) => toast.error("Could not save", errorMessage(e)))
              .finally(() => setBusy(false));
          }}
        />
      </span>
      <span className="max-w-64 text-right text-[11px] text-muted-foreground">
        Words in {AUTO_MIN} or more names become tags as you index. Deleted or dismissed ones never
        come back.
      </span>
    </Label>
  );
}

/** Matches AUTO_MIN_CONTAINERS in autotag.rs. */
const AUTO_MIN = 3;

function DismissedNote({ onChanged }: { onChanged: () => void }) {
  const qc = useQueryClient();
  const n = useSettings().data?.dismissedSuggestions.length ?? 0;
  if (n === 0) return null;
  return (
    <p className="mt-3 text-xs text-muted-foreground">
      {plural(n, "word")} dismissed.{" "}
      <button
        type="button"
        className="text-primary hover:underline"
        onClick={() =>
          void api.restoreSuggestions().then(() => {
            void qc.invalidateQueries({ queryKey: qk.settings });
            onChanged();
          })
        }
      >
        Suggest them again
      </button>
    </p>
  );
}

function AutoTagSwitch() {
  const qc = useQueryClient();
  const settings = useSettings();
  const [busy, setBusy] = useState(false);
  const on = settings.data?.autotagOnIndex ?? true;
  return (
    <div className="flex shrink-0 flex-col items-end gap-1.5">
      <Label className="flex items-center gap-2 text-xs font-normal whitespace-nowrap">
        Tag new files while indexing
        <Switch
          checked={on}
          disabled={!settings.data}
          onCheckedChange={(v) =>
            void api
              .setAutotagOnIndex(v)
              .then(() => qc.invalidateQueries({ queryKey: qk.settings }))
              .catch((e: unknown) => toast.error("Could not save", errorMessage(e)))
          }
        />
      </Label>
      <Button
        size="xs"
        variant="outline"
        disabled={busy}
        onClick={() => {
          setBusy(true);
          void api
            .applyAutoTags()
            .then((n) => {
              invalidateTags(qc);
              toast.success(
                n === 0 ? "Auto tags are up to date" : `Updated tags on ${plural(n, "file")}`,
              );
            })
            .catch((e: unknown) => toast.error("Could not apply rules", errorMessage(e)))
            .finally(() => setBusy(false));
        }}
      >
        <RefreshCw className={busy ? "animate-spin" : undefined} /> Apply rules to everything now
      </Button>
    </div>
  );
}

function TagRuleRow({ tag, onChanged }: { tag: Tag; onChanged: () => void }) {
  const [draft, setDraft] = useState(tag.keywords ?? "");
  const dirty = draft.trim() !== (tag.keywords ?? "");
  const save = async () => {
    if (!dirty) return;
    try {
      await api.updateTag(tag.id, { keywords: draft.trim() });
      onChanged();
    } catch (e) {
      toast.error("Could not save keywords", errorMessage(e));
    }
  };
  return (
    <div className="flex items-center gap-3 px-3 py-2">
      <span className="size-2.5 shrink-0 rounded-full" style={{ backgroundColor: tag.colour }} />
      <div className="flex w-36 shrink-0 flex-col">
        <span className="truncate text-sm font-medium">{tag.name}</span>
        <span className="text-[11px] text-muted-foreground tabular-nums">
          {plural(tag.usage, "item")}
          {tag.autoUsage > 0 && ` · ${tag.autoUsage.toLocaleString("en-GB")} auto`}
        </span>
      </div>
      <form
        className="flex min-w-0 flex-1 gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          void save();
        }}
      >
        <Input
          value={draft}
          placeholder="No keywords: tagged by hand only"
          onChange={(e) => setDraft(e.target.value)}
          className="h-8"
        />
        {dirty && (
          <Button type="submit" size="xs">
            Save
          </Button>
        )}
      </form>
      <Tip label="Delete tag">
        <Button
          variant="ghost"
          size="icon-xs"
          onClick={() =>
            void api
              .deleteTag(tag.id)
              .then(onChanged)
              .catch((e: unknown) => toast.error("Could not delete tag", errorMessage(e)))
          }
        >
          <Trash2 />
        </Button>
      </Tip>
    </div>
  );
}

async function pickFolder(): Promise<string | null> {
  const res = await openDialog({ directory: true, multiple: false });
  return typeof res === "string" ? res : null;
}

async function pickFile(): Promise<string | null> {
  const res = await openDialog({ directory: false, multiple: false });
  return typeof res === "string" ? res : null;
}

function LibrariesTab() {
  const qc = useQueryClient();
  const libs = useLibraries();
  const scans = useUi((s) => s.scans);
  const [editing, setEditing] = useState<number | null>(null);
  const [name, setName] = useState("");
  const refresh = () => invalidateIndex(qc);

  const add = async () => {
    try {
      const path = await pickFolder();
      if (!path) return;
      const lib = await api.addLibrary(path);
      toast.success(`Added ${lib.name}`, "Indexing has started in the background.");
      refresh();
    } catch (e) {
      toast.error("Could not add library", errorMessage(e));
    }
  };

  const update = async (id: number, patch: { name?: string; enabled?: boolean }) => {
    try {
      await api.updateLibrary(id, patch);
      refresh();
    } catch (e) {
      toast.error("Could not update library", errorMessage(e));
    }
  };

  return (
    <Section
      title="Libraries"
      description="Folders that load on startup. Archives inside them show up as folders you can browse."
      action={
        <Button size="sm" onClick={() => void add()}>
          <FolderPlus /> Add library
        </Button>
      }
    >
      {libs.isLoading && <Skeleton className="h-16" />}
      <div className="flex flex-col divide-y rounded-lg border">
        {(libs.data ?? []).map((l) => (
          <div key={l.id} className="flex items-center gap-3 px-3 py-2.5">
            <div className="min-w-0 flex-1">
              {editing === l.id ? (
                <form
                  className="flex gap-2"
                  onSubmit={(e) => {
                    e.preventDefault();
                    void update(l.id, { name });
                    setEditing(null);
                  }}
                >
                  <Input
                    autoFocus
                    value={name}
                    onChange={(e) => setName(e.target.value)}
                    className="h-7"
                  />
                  <Button type="submit" size="xs">
                    Save
                  </Button>
                </form>
              ) : (
                <div className="flex items-center gap-2">
                  {l.isVault && <ShieldCheck className="size-4 text-success" />}
                  <span className="truncate text-sm font-medium">{l.name}</span>
                  {l.isVault && <Badge variant="success">Vault</Badge>}
                  {!l.pathExists && (
                    <Badge variant="warning">
                      <TriangleAlert /> Folder missing
                    </Badge>
                  )}
                </div>
              )}
              <p className="truncate text-xs text-muted-foreground selectable" title={l.path}>
                {l.path}
              </p>
              <p className="text-[11px] text-muted-foreground">
                {plural(l.modelCount, "model")} · scanned {timeAgo(l.lastScannedAt)}
                {scans[l.id] && " · scanning now"}
              </p>
            </div>
            <Tip label="Rename">
              <Button
                variant="ghost"
                size="icon-xs"
                onClick={() => {
                  setEditing(l.id);
                  setName(l.name);
                }}
              >
                <Pencil />
              </Button>
            </Tip>
            <Tip label="Rescan">
              <Button variant="ghost" size="icon-xs" onClick={() => void api.rescanLibrary(l.id)}>
                <RefreshCw className={cn(scans[l.id] && "animate-spin")} />
              </Button>
            </Tip>
            <Tip label={l.isVault ? "The vault is always on" : l.enabled ? "Disable" : "Enable"}>
              <span>
                <Switch
                  checked={l.enabled}
                  disabled={l.isVault}
                  onCheckedChange={(v) => void update(l.id, { enabled: v })}
                />
              </span>
            </Tip>
            <Tip
              label={
                l.isVault
                  ? "The vault cannot be removed"
                  : "Remove from the list (files are not touched)"
              }
            >
              <span>
                <Button
                  variant="ghost"
                  size="icon-xs"
                  disabled={l.isVault}
                  onClick={() =>
                    void api
                      .removeLibrary(l.id)
                      .then(refresh)
                      .catch((e: unknown) => toast.error("Could not remove", errorMessage(e)))
                  }
                >
                  <Trash2 />
                </Button>
              </span>
            </Tip>
          </div>
        ))}
      </div>
    </Section>
  );
}

const EMPTY_FORM = { name: "", exePath: "", args: "", onePerFile: false };

function SlicersTab() {
  const qc = useQueryClient();
  const slicers = useSlicers();
  const presets = useSlicerPresets(true);
  const [form, setForm] = useState(EMPTY_FORM);
  const [editId, setEditId] = useState<number | null>(null);
  const set = (list: Slicer[]) => {
    qc.setQueryData(qk.slicers, list);
    void qc.invalidateQueries({ queryKey: qk.presets });
  };
  const save = useMutation({
    mutationFn: () => {
      const input = {
        name: form.name,
        exePath: form.exePath,
        args: splitArgs(form.args),
        onePerFile: form.onePerFile,
      };
      return editId === null ? api.addSlicer(input) : api.updateSlicer(editId, input);
    },
    onSuccess: (list) => {
      set(list);
      setForm(EMPTY_FORM);
      setEditId(null);
    },
    onError: (e) => toast.error("Could not save slicer", errorMessage(e)),
  });

  return (
    <>
      <Section
        title="Slicers"
        description="Files you send open in one launch. The starred slicer is used for Enter and double-click."
      >
        <div className="flex flex-col divide-y rounded-lg border">
          {(slicers.data ?? []).length === 0 && (
            <p className="px-3 py-4 text-sm text-muted-foreground">
              No slicers yet. Add a detected one below, or enter a path.
            </p>
          )}
          {(slicers.data ?? []).map((s) => (
            <div key={s.id} className="flex items-center gap-3 px-3 py-2.5">
              <Tip label={s.isDefault ? "Default slicer" : "Make default"}>
                <Button
                  variant="ghost"
                  size="icon-xs"
                  onClick={() => void api.setDefaultSlicer(s.id).then(set)}
                >
                  <Star className={cn(s.isDefault && "fill-warning text-warning")} />
                </Button>
              </Tip>
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span className="text-sm font-medium">{s.name}</span>
                  {!s.pathExists && <Badge variant="warning">Not found</Badge>}
                  {s.onePerFile && <Badge variant="outline">One file per launch</Badge>}
                </div>
                <p className="truncate text-xs text-muted-foreground selectable" title={s.exePath}>
                  {s.exePath}
                  {s.args.length > 0 && <span className="ml-1 font-mono">{joinArgs(s.args)}</span>}
                </p>
              </div>
              <Button
                variant="ghost"
                size="icon-xs"
                onClick={() => {
                  setEditId(s.id);
                  setForm({
                    name: s.name,
                    exePath: s.exePath,
                    args: joinArgs(s.args),
                    onePerFile: s.onePerFile,
                  });
                }}
              >
                <Pencil />
              </Button>
              <Button
                variant="ghost"
                size="icon-xs"
                onClick={() => void api.deleteSlicer(s.id).then(set)}
              >
                <Trash2 />
              </Button>
            </div>
          ))}
        </div>
      </Section>
      {(presets.data ?? []).some((p) => !p.alreadyAdded) && (
        <Section title="Found on this computer">
          <div className="flex flex-wrap gap-2">
            {(presets.data ?? [])
              .filter((p) => !p.alreadyAdded)
              .map((p) => (
                <Button
                  key={p.exePath}
                  variant="outline"
                  size="sm"
                  onClick={() =>
                    void api
                      .addSlicer({ name: p.name, exePath: p.exePath, args: [], onePerFile: false })
                      .then(set)
                      .catch((e: unknown) => toast.error("Could not add slicer", errorMessage(e)))
                  }
                >
                  <Plus /> {p.name}
                </Button>
              ))}
          </div>
        </Section>
      )}
      <Section title={editId === null ? "Add a slicer" : "Edit slicer"}>
        <form
          className="grid gap-3"
          onSubmit={(e) => {
            e.preventDefault();
            save.mutate();
          }}
        >
          <div className="grid gap-1.5">
            <Label htmlFor="sl-name">Name</Label>
            <Input
              id="sl-name"
              value={form.name}
              onChange={(e) => setForm({ ...form, name: e.target.value })}
              placeholder="e.g. Bambu Studio"
            />
          </div>
          <div className="grid gap-1.5">
            <Label htmlFor="sl-path">Program</Label>
            <div className="flex gap-2">
              <Input
                id="sl-path"
                value={form.exePath}
                onChange={(e) => setForm({ ...form, exePath: e.target.value })}
                placeholder="Path to the slicer's .exe or .app"
              />
              <Button
                type="button"
                variant="outline"
                onClick={() =>
                  void pickFile().then((p) => p && setForm((f) => ({ ...f, exePath: p })))
                }
              >
                <FolderOpen /> Browse
              </Button>
            </div>
          </div>
          <div className="grid gap-1.5">
            <Label htmlFor="sl-args">Extra arguments (optional)</Label>
            <Input
              id="sl-args"
              className="font-mono"
              value={form.args}
              onChange={(e) => setForm({ ...form, args: e.target.value })}
              placeholder="e.g. --single-instance"
            />
            <p className="text-[11px] text-muted-foreground">
              Placed before the file paths. Use quotes for values with spaces.
            </p>
          </div>
          <label className="flex items-center gap-2 text-sm">
            <Switch
              checked={form.onePerFile}
              onCheckedChange={(v) => setForm({ ...form, onePerFile: v })}
            />
            Launch once per file (for slicers that only accept one file)
          </label>
          <div className="flex gap-2">
            <Button
              type="submit"
              disabled={!form.name.trim() || !form.exePath.trim() || save.isPending}
            >
              {editId === null ? "Add slicer" : "Save changes"}
            </Button>
            {editId !== null && (
              <Button
                type="button"
                variant="ghost"
                onClick={() => {
                  setEditId(null);
                  setForm(EMPTY_FORM);
                }}
              >
                Cancel
              </Button>
            )}
          </div>
        </form>
      </Section>
    </>
  );
}

function ArchivesTab() {
  const qc = useQueryClient();
  const sz = useSevenZip();
  const settings = useSettings();
  const stats = useCacheStats(true);

  return (
    <>
      <Section
        title="7-Zip"
        description="Used for 7z, rar and multi-part archives. Plain zips are read natively."
      >
        {sz.data && (
          <div className="flex items-start gap-3 rounded-lg border p-3">
            {sz.data.path ? (
              <CircleCheck className="mt-0.5 size-4 text-success" />
            ) : (
              <TriangleAlert className="mt-0.5 size-4 text-warning" />
            )}
            <div className="min-w-0 text-sm">
              <p className="font-medium">
                {sz.data.path ? (sz.data.version ?? "7-Zip") : "7-Zip was not found"}
              </p>
              <p className="text-xs text-muted-foreground">{sz.data.source}</p>
              {sz.data.path && (
                <p className="truncate text-xs text-muted-foreground selectable">{sz.data.path}</p>
              )}
              {!sz.data.path && (
                <p className="mt-1 text-xs text-muted-foreground">
                  Install 7-Zip (or put 7zz on PATH) and restart STL Xplorer. Zip files work without
                  it.
                </p>
              )}
            </div>
          </div>
        )}
      </Section>
      <Section
        title="Solid archive cache"
        description="Solid 7z and rar packs are unpacked once on first preview. Models only, never the whole archive. The oldest packs are removed when the cache is full."
      >
        <div className="flex flex-col gap-4">
          {settings.data && (
            <CacheCapForm
              key={settings.data.solidCacheCapBytes}
              bytes={settings.data.solidCacheCapBytes}
            />
          )}
          {stats.data && (
            <div className="grid grid-cols-3 gap-2">
              <Stat
                label="Solid packs"
                value={`${formatBytes(stats.data.solidBytes)}`}
                sub={plural(stats.data.solidArchives, "pack")}
              />
              <Stat
                label="Opened in slicers"
                value={formatBytes(stats.data.extractedBytes)}
                sub="cleared after 7 days"
              />
              <Stat
                label="In memory"
                value={formatBytes(stats.data.memoryBytes)}
                sub="recent previews"
              />
            </div>
          )}
          <div>
            <Button
              variant="outline"
              onClick={() =>
                void api.clearCache().then(() => {
                  void qc.invalidateQueries({ queryKey: qk.cache });
                  toast.success("Archive cache cleared");
                })
              }
            >
              <Trash2 /> Clear cache
            </Button>
          </div>
        </div>
      </Section>
    </>
  );
}

function CacheCapForm({ bytes }: { bytes: number }) {
  const qc = useQueryClient();
  const [gb, setGb] = useState(String(Math.round((bytes / 1024 ** 3) * 10) / 10));
  return (
    <div className="flex items-end gap-2">
      <div className="grid gap-1.5">
        <Label htmlFor="cap">Maximum size (GB)</Label>
        <Input
          id="cap"
          type="number"
          min={0.25}
          step={0.5}
          value={gb}
          onChange={(e) => setGb(e.target.value)}
          className="w-32"
        />
      </div>
      <Button
        variant="outline"
        onClick={() => {
          const v = Number(gb);
          if (!Number.isFinite(v) || v <= 0) return;
          void api.setSolidCacheCap(Math.round(v * 1024 ** 3)).then(() => {
            void qc.invalidateQueries({ queryKey: qk.settings });
            void qc.invalidateQueries({ queryKey: qk.cache });
            toast.success("Cache limit saved");
          });
        }}
      >
        Save
      </Button>
    </div>
  );
}

function Stat({ label, value, sub }: { label: string; value: string; sub: string }) {
  return (
    <div className="rounded-lg border p-3">
      <p className="text-[11px] text-muted-foreground">{label}</p>
      <p className="text-lg font-semibold tabular-nums">{value}</p>
      <p className="text-[11px] text-muted-foreground">{sub}</p>
    </div>
  );
}

function RotationPicker({ value, onChange }: { value: Rotation; onChange: (r: Rotation) => void }) {
  return (
    <div className="flex gap-2">
      {(["x", "y", "z"] as const).map((axis) => (
        <Button
          key={axis}
          variant="outline"
          className="w-24 justify-between font-mono"
          onClick={() => onChange(rotate90(value, axis))}
        >
          <span
            className={
              axis === "x" ? "text-red-400" : axis === "y" ? "text-green-400" : "text-sky-400"
            }
          >
            {axis.toUpperCase()}
          </span>
          {Math.round(value[axis])}°
        </Button>
      ))}
    </div>
  );
}

function OrientationTab() {
  const settings = useSettings();
  if (!settings.data) return <Skeleton className="h-24" />;
  const g = settings.data.globalRotation;
  return <OrientationForm key={g ? `${g.x},${g.y},${g.z}` : "none"} initial={g} />;
}

function OrientationForm({ initial }: { initial: Rotation | null }) {
  const qc = useQueryClient();
  const [r, setR] = useState<Rotation>(initial ?? ZERO_ROTATION);
  const save = (rot: Rotation | null) =>
    void api.setGlobalRotation(rot).then(() => {
      void qc.invalidateQueries({ queryKey: qk.settings });
      void qc.invalidateQueries({ queryKey: ["model"] });
      toast.success(rot ? "Global default saved" : "Global default cleared");
    });
  return (
    <Section
      title="Global default orientation"
      description="Applied to every model without its own or its pack's orientation. Set per model or per pack from the viewer toolbar. Each click turns 90°."
    >
      <div className="flex flex-col gap-3">
        <RotationPicker value={r} onChange={setR} />
        <div className="flex gap-2">
          <Button onClick={() => save(r)}>Save default</Button>
          <Button variant="ghost" onClick={() => save(null)} disabled={!initial}>
            Clear
          </Button>
        </div>
        <p className="text-xs text-muted-foreground">
          Order of precedence: model, then pack, then this default.
        </p>
      </div>
    </Section>
  );
}

function VaultTab() {
  const qc = useQueryClient();
  const settings = useSettings();
  const change = async () => {
    try {
      const p = await pickFolder();
      if (!p) return;
      await api.setVaultPath(p);
      invalidateIndex(qc);
      toast.success(
        "Vault folder changed",
        "Items already in the old vault stay listed as a normal library.",
      );
    } catch (e) {
      toast.error("Could not change the vault", errorMessage(e));
    }
  };
  return (
    <Section
      title="Vault"
      description="Save moves a file, or the whole archive it came from, out of your auto-cleaning downloads into this folder. Nothing in the vault is ever deleted automatically."
    >
      <div className="flex items-center gap-3 rounded-lg border p-3">
        <ShieldCheck className="size-5 shrink-0 text-success" />
        <p className="min-w-0 flex-1 truncate text-sm selectable" title={settings.data?.vaultPath}>
          {settings.data?.vaultPath ?? "…"}
        </p>
        <Button variant="outline" size="sm" onClick={() => void change()}>
          Change…
        </Button>
      </div>
      <p className="mt-3 text-xs text-muted-foreground">
        Multi-part archives move together; if any part fails, every part goes back where it was.
      </p>
    </Section>
  );
}

function AppearanceTab() {
  const theme = useUi((s) => s.theme);
  const setTheme = useUi((s) => s.setTheme);
  const options: Array<{ id: Theme; label: string }> = [
    { id: "dark", label: "Dark" },
    { id: "light", label: "Light" },
    { id: "system", label: "Match system" },
  ];
  return (
    <Section title="Theme">
      <div className="flex gap-2">
        {options.map((o) => (
          <Button
            key={o.id}
            variant={theme === o.id ? "default" : "outline"}
            onClick={() => setTheme(o.id)}
          >
            {o.label}
          </Button>
        ))}
      </div>
    </Section>
  );
}

function UpdatesSection() {
  const { status, autoCheck, setAutoCheck, check, setDialogOpen } = useUpdater();
  const [version, setVersion] = useState<string | null>(null);
  useEffect(() => {
    void appVersion().then(setVersion);
  }, []);
  const checking = status.kind === "checking";
  return (
    <Section
      title="Updates"
      description={`You have version ${version ?? "…"}. New versions come from the project's GitHub releases.`}
      action={
        <Label className="flex shrink-0 items-center gap-2 text-xs font-normal whitespace-nowrap">
          Check on startup
          <Switch checked={autoCheck} onCheckedChange={setAutoCheck} />
        </Label>
      }
    >
      <div className="flex items-center gap-3">
        {status.kind === "available" || status.kind === "downloading" ? (
          <Button onClick={() => setDialogOpen(true)}>
            <ArrowDownToLine /> Install {status.update.version}
          </Button>
        ) : (
          <Button variant="outline" disabled={checking} onClick={() => void check()}>
            <RefreshCw className={cn(checking && "animate-spin")} /> Check for updates
          </Button>
        )}
        <p className="text-xs text-muted-foreground">
          {status.kind === "checking"
            ? "Checking…"
            : status.kind === "current"
              ? "You're on the latest version."
              : status.kind === "available"
                ? `Version ${status.update.version} is ready to install.`
                : status.kind === "error"
                  ? status.message
                  : ""}
        </p>
      </div>
    </Section>
  );
}

function MaintenanceTab() {
  const qc = useQueryClient();
  const settings = useSettings();
  const missing = (settings.data?.missingModels ?? 0) + (settings.data?.missingContainers ?? 0);
  return (
    <>
      <UpdatesSection />
      <Section
        title="Missing files"
        description="When files disappear they are kept in the index, so tags and orientation come back if the folder returns. Clean up to forget them for good."
      >
        <div className="flex items-center gap-3">
          <p className="text-sm">
            {settings.data
              ? missing === 0
                ? "Nothing missing."
                : `${plural(settings.data.missingModels, "model")} and ${plural(settings.data.missingContainers, "folder or archive", "folders or archives")} are missing.`
              : "…"}
          </p>
          <Button
            variant="outline"
            size="sm"
            disabled={missing === 0}
            onClick={() =>
              void api.cleanUpMissing().then((n) => {
                invalidateIndex(qc);
                toast.success(`Forgot ${plural(n, "missing item")}`);
              })
            }
          >
            Clean up missing
          </Button>
        </div>
      </Section>
      <Section
        title="Unpacked copies"
        description="Folders that duplicate an archive sitting next to them. STL Xplorer previews straight from archives, so these only take up space."
      >
        <Button
          variant="outline"
          onClick={() => usePacks.getState().findUnpacked({ id: null, name: "your libraries" })}
        >
          <FolderX /> Find unpacked copies
        </Button>
      </Section>
      <Section title="Rescan" description="Rescans only touch files whose size or date changed.">
        <Button variant="outline" onClick={() => void api.rescanAll()}>
          <RefreshCw /> Rescan all libraries
        </Button>
      </Section>
      <Section title="Data">
        <p className="text-xs text-muted-foreground selectable">
          Index stored in {settings.data?.dataDir}
        </p>
      </Section>
    </>
  );
}
