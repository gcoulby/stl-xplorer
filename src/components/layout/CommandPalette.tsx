import { useQueryClient } from "@tanstack/react-query";
import {
  Eraser,
  FolderPlus,
  FolderX,
  LayoutGrid,
  Layers,
  List,
  Moon,
  RefreshCw,
  Settings,
  ShieldCheck,
  SlidersHorizontal,
  Sun,
} from "lucide-react";
import { useDeferredValue, useState } from "react";

import { ModelIcon } from "@/components/browser/ItemVisuals";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandShortcut,
} from "@/components/ui/command";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/overlay";
import { usePacks } from "@/features/packs";
import { useCollections, useLibraries, useSearch } from "@/features/queries";
import { toast } from "@/features/toast";
import { api, errorMessage } from "@/lib/api";
import { useUi } from "@/store/ui";

export function CommandPalette() {
  const qc = useQueryClient();
  const open = useUi((s) => s.paletteOpen);
  const setOpen = useUi((s) => s.setPaletteOpen);
  const [q, setQ] = useState("");
  const deferred = useDeferredValue(q);
  const hits = useSearch(deferred.trim().length >= 2 ? deferred : "");
  const libs = useLibraries().data ?? [];
  const collections = useCollections().data ?? [];
  const ui = useUi.getState();

  const run = (fn: () => void) => {
    setOpen(false);
    setQ("");
    fn();
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        setOpen(o);
        if (!o) setQ("");
      }}
    >
      <DialogContent hideClose className="overflow-hidden p-0 sm:max-w-xl">
        <DialogTitle className="sr-only">Command palette</DialogTitle>
        <DialogDescription className="sr-only">Search models or run a command</DialogDescription>
        <Command shouldFilter={false} loop>
          <CommandInput
            value={q}
            onValueChange={setQ}
            placeholder="Search models or type a command…"
          />
          <CommandList>
            <CommandEmpty>Nothing found.</CommandEmpty>
            {(hits.data?.length ?? 0) > 0 && deferred.trim().length >= 2 && (
              <CommandGroup heading="Models">
                {hits.data!.slice(0, 8).map((h) => (
                  <CommandItem
                    key={h.model.id}
                    value={`m${h.model.id}`}
                    onSelect={() =>
                      run(() => {
                        ui.setPreview(h.model.id);
                        ui.navigate({ kind: "search", query: deferred });
                      })
                    }
                  >
                    <ModelIcon ext={h.model.ext} />
                    <span className="truncate">{h.model.name}</span>
                    <span className="ml-auto max-w-[45%] truncate text-xs text-muted-foreground">
                      {h.location}
                    </span>
                  </CommandItem>
                ))}
                {(hits.data?.length ?? 0) > 8 && (
                  <CommandItem
                    value="all-results"
                    onSelect={() => run(() => ui.setSearchText(deferred))}
                  >
                    Show all {hits.data!.length} results
                  </CommandItem>
                )}
              </CommandGroup>
            )}
            <CommandGroup heading="Go to">
              {libs
                .filter((l) => l.enabled && l.rootContainerId !== null)
                .filter((l) => matches(q, l.name))
                .map((l) => (
                  <CommandItem
                    key={`lib${l.id}`}
                    value={`lib${l.id}`}
                    onSelect={() =>
                      run(() =>
                        ui.navigate({
                          kind: "container",
                          containerId: l.rootContainerId!,
                          subpath: "",
                        }),
                      )
                    }
                  >
                    {l.isVault ? <ShieldCheck /> : <FolderPlus className="opacity-0" />}
                    {l.name}
                  </CommandItem>
                ))}
              {collections
                .filter((c) => matches(q, c.name))
                .map((c) => (
                  <CommandItem
                    key={`col${c.id}`}
                    value={`col${c.id}`}
                    onSelect={() =>
                      run(() => ui.navigate({ kind: "collection", collectionId: c.id }))
                    }
                  >
                    <Layers /> {c.name}
                  </CommandItem>
                ))}
            </CommandGroup>
            <CommandGroup heading="Actions">
              {[
                {
                  id: "rescan",
                  label: "Rescan all libraries",
                  icon: <RefreshCw />,
                  act: () =>
                    void api
                      .rescanAll()
                      .catch((e: unknown) => toast.error("Rescan failed", errorMessage(e))),
                },
                {
                  id: "unpacked",
                  label: "Find unpacked copies of archives",
                  icon: <FolderX />,
                  act: () => usePacks.getState().findUnpacked({ id: null, name: "your libraries" }),
                },
                {
                  id: "addlib",
                  label: "Add a library",
                  icon: <FolderPlus />,
                  act: () => ui.openSettings("libraries"),
                },
                {
                  id: "slicers",
                  label: "Manage slicers",
                  icon: <SlidersHorizontal />,
                  act: () => ui.openSettings("slicers"),
                },
                {
                  id: "settings",
                  label: "Open settings",
                  icon: <Settings />,
                  act: () => ui.openSettings(),
                  kbd: "Ctrl ,",
                },
                {
                  id: "list",
                  label: "List view",
                  icon: <List />,
                  act: () => ui.setViewMode("list"),
                },
                {
                  id: "grid",
                  label: "Grid view",
                  icon: <LayoutGrid />,
                  act: () => ui.setViewMode("grid"),
                },
                { id: "dark", label: "Dark theme", icon: <Moon />, act: () => ui.setTheme("dark") },
                {
                  id: "light",
                  label: "Light theme",
                  icon: <Sun />,
                  act: () => ui.setTheme("light"),
                },
                {
                  id: "clear",
                  label: "Clear archive cache",
                  icon: <Eraser />,
                  act: () =>
                    void api
                      .clearCache()
                      .then(() => {
                        toast.success("Archive cache cleared");
                        void qc.invalidateQueries({ queryKey: ["cache-stats"] });
                      })
                      .catch((e: unknown) =>
                        toast.error("Could not clear the cache", errorMessage(e)),
                      ),
                },
              ]
                .filter((a) => matches(q, a.label))
                .map((a) => (
                  <CommandItem key={a.id} value={a.id} onSelect={() => run(a.act)}>
                    {a.icon}
                    {a.label}
                    {a.kbd && <CommandShortcut>{a.kbd}</CommandShortcut>}
                  </CommandItem>
                ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </DialogContent>
    </Dialog>
  );
}

function matches(q: string, label: string): boolean {
  const t = q.trim().toLowerCase();
  if (!t) return true;
  return t.split(/\s+/).every((w) => label.toLowerCase().includes(w));
}
