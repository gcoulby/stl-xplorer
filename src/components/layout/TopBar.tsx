import { ArrowLeft, Command, Moon, Search, Settings, Sun, X } from "lucide-react";
import { useRef } from "react";

import { Button } from "@/components/ui/button";
import { Kbd } from "@/components/ui/misc";
import { Tip } from "@/components/ui/overlay";
import { useUi } from "@/store/ui";

export function TopBar() {
  const searchText = useUi((s) => s.searchText);
  const setSearchText = useUi((s) => s.setSearchText);
  const history = useUi((s) => s.history);
  const back = useUi((s) => s.back);
  const theme = useUi((s) => s.theme);
  const setTheme = useUi((s) => s.setTheme);
  const openSettings = useUi((s) => s.openSettings);
  const setPaletteOpen = useUi((s) => s.setPaletteOpen);
  const inputRef = useRef<HTMLInputElement>(null);
  const text = searchText;
  // FTS5 prefix queries are quick enough to run on every keystroke.
  const onChange = (v: string) => setSearchText(v);

  const dark = theme !== "light";

  return (
    <div
      className="grid h-12 shrink-0 grid-cols-[1fr_minmax(0,46rem)_1fr] items-center gap-3 border-b bg-pane px-3"
      data-tauri-drag-region
    >
      <div className="flex min-w-0 items-center gap-2" data-tauri-drag-region>
        <div className="flex items-center gap-2 pr-2" data-tauri-drag-region>
          <img src="/logo.svg" alt="" className="size-6" draggable={false} />
          <span className="text-sm font-semibold tracking-tight" data-tauri-drag-region>
            STL Xplorer
          </span>
        </div>
        <Tip label="Back (Alt+←)">
          <Button variant="ghost" size="icon-sm" disabled={history.length === 0} onClick={back}>
            <ArrowLeft />
          </Button>
        </Tip>
      </div>
      <div className="relative w-full">
        <Search className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" />
        <input
          ref={inputRef}
          data-search-input
          value={text}
          onChange={(e) => onChange(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              onChange("");
              inputRef.current?.blur();
            } else if (e.key === "ArrowDown" || e.key === "Enter") {
              e.preventDefault();
              document.querySelector<HTMLElement>("[data-file-list]")?.focus();
            }
          }}
          spellCheck={false}
          placeholder="Search models, packs, folders and tags"
          className="h-8 w-full rounded-md border border-input bg-background/60 pr-16 pl-8 text-sm outline-none placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/40 dark:bg-input/30"
        />
        <div className="absolute top-1/2 right-2 flex -translate-y-1/2 items-center gap-1">
          {text ? (
            <button
              type="button"
              onClick={() => onChange("")}
              className="rounded p-0.5 text-muted-foreground hover:text-foreground"
              aria-label="Clear search"
            >
              <X className="size-3.5" />
            </button>
          ) : (
            <Kbd>/</Kbd>
          )}
        </div>
      </div>
      <div className="flex items-center justify-end gap-1" data-tauri-drag-region>
        <Tip label="Commands (Ctrl+K)">
          <Button variant="ghost" size="icon-sm" onClick={() => setPaletteOpen(true)}>
            <Command />
          </Button>
        </Tip>
        <Tip label={dark ? "Light theme" : "Dark theme"}>
          <Button variant="ghost" size="icon-sm" onClick={() => setTheme(dark ? "light" : "dark")}>
            {dark ? <Sun /> : <Moon />}
          </Button>
        </Tip>
        <Tip label="Settings (Ctrl+,)">
          <Button variant="ghost" size="icon-sm" onClick={() => openSettings()}>
            <Settings />
          </Button>
        </Tip>
      </div>
    </div>
  );
}
