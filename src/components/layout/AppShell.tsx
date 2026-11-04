import { useEffect } from "react";
import { Group, Panel, Separator, useDefaultLayout } from "react-resizable-panels";

import { FileList } from "@/components/browser/FileList";
import { Sidebar } from "@/components/browser/Sidebar";
import { CommandPalette } from "@/components/layout/CommandPalette";
import { DeleteDialog } from "@/components/layout/DeleteDialog";
import { PackDialogs } from "@/components/layout/PackDialogs";
import { PromptDialog } from "@/components/layout/PromptDialog";
import { StatusBar } from "@/components/layout/StatusBar";
import { Toaster } from "@/components/layout/Toaster";
import { TopBar } from "@/components/layout/TopBar";
import { SettingsDialog } from "@/components/settings/SettingsDialog";
import { ViewerPane } from "@/components/viewer/ViewerPane";
import { isTypingTarget } from "@/lib/utils";
import { useUi } from "@/store/ui";

function useGlobalShortcuts() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const ui = useUi.getState();
      const mod = (e.ctrlKey || e.metaKey) && !e.altKey;
      // Match the physical key too, so non-QWERTY layouts and odd key values still work.
      if (mod && (e.code === "KeyK" || e.key.toLowerCase() === "k")) {
        e.preventDefault();
        e.stopPropagation();
        if (e.repeat) return;
        ui.setPaletteOpen(!ui.paletteOpen);
        return;
      }
      if (mod && (e.key === "," || e.code === "Comma")) {
        e.preventDefault();
        ui.openSettings();
        return;
      }
      if (mod && (e.code === "KeyL" || e.key.toLowerCase() === "l")) {
        e.preventDefault();
        document
          .querySelector<HTMLButtonElement>("[data-address-bar] button[aria-label='Edit path']")
          ?.click();
        return;
      }
      if (isTypingTarget(e.target)) return;
      if (e.key === "/") {
        e.preventDefault();
        document.querySelector<HTMLInputElement>("[data-search-input]")?.focus();
        return;
      }
      if (e.altKey && e.key === "ArrowLeft") {
        e.preventDefault();
        ui.back();
      }
    };
    // Capture phase, so a focused widget that handles keys itself cannot swallow these.
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);
}

export function AppShell() {
  useGlobalShortcuts();
  const layout = useDefaultLayout({ id: "stlx-main", storage: localStorage });
  return (
    <div className="flex h-full flex-col">
      <TopBar />
      <Group
        orientation="horizontal"
        className="min-h-0 flex-1"
        defaultLayout={layout.defaultLayout}
        onLayoutChanged={layout.onLayoutChanged}
      >
        <Panel id="tree" defaultSize="20" minSize={180} maxSize="40">
          <Sidebar />
        </Panel>
        <Separator className="w-px bg-border outline-none" />
        <Panel id="list" defaultSize="40" minSize={280}>
          <FileList />
        </Panel>
        <Separator className="w-px bg-border outline-none" />
        <Panel id="viewer" defaultSize="40" minSize={300}>
          <ViewerPane />
        </Panel>
      </Group>
      <StatusBar />
      <CommandPalette />
      <SettingsDialog />
      <PromptDialog />
      <DeleteDialog />
      <PackDialogs />
      <Toaster />
    </div>
  );
}
