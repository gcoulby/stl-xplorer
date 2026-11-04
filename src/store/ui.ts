import { create } from "zustand";
import { persist } from "zustand/middleware";

import type { PackProgress, ScanProgress } from "@/types";

export type SortKey = "name" | "size" | "modified" | "type";
export interface Sort {
  key: SortKey;
  desc: boolean;
}
/** File types the list can be narrowed to. */
export type TypeFilter = "stl" | "3mf" | "obj" | "image" | "pdf";
export interface Filters {
  types: TypeFilter[];
  tagIds: number[];
}
export interface Columns {
  type: boolean;
  size: boolean;
  modified: boolean;
  location: boolean;
  tags: boolean;
}

export type Location =
  | { kind: "none" }
  | { kind: "container"; containerId: number; subpath: string }
  | { kind: "search"; query: string }
  | { kind: "collection"; collectionId: number }
  | { kind: "tag"; tagId: number }
  | { kind: "tags" }
  | { kind: "problems" };

export type Theme = "dark" | "light" | "system";
export type ViewMode = "list" | "grid";
export type SettingsTab =
  | "libraries"
  | "slicers"
  | "archives"
  | "tags"
  | "orientation"
  | "vault"
  | "appearance"
  | "maintenance";

export function treeKey(containerId: number, subpath: string): string {
  return `${containerId}:${subpath}`;
}

interface UiState {
  location: Location;
  /** Where to go back to when search is cleared. */
  lastBrowse: Location;
  history: Location[];
  searchText: string;

  selection: string[];
  anchor: string | null;
  focused: string | null;
  /** Model shown in the viewer. Stays put while moving through folders. */
  previewModelId: number | null;

  expanded: string[];
  viewMode: ViewMode;
  /** Show every file below a folder or archive in one list instead of its subfolders. */
  flat: boolean;
  sort: Sort;
  filters: Filters;
  columns: Columns;
  /** Zipping in progress, for the status bar. */
  packJob: PackProgress | null;
  theme: Theme;

  settingsOpen: boolean;
  settingsTab: SettingsTab;
  paletteOpen: boolean;

  scans: Record<number, ScanProgress>;

  navigate: (loc: Location) => void;
  back: () => void;
  setSearchText: (q: string) => void;
  setSelection: (keys: string[], anchor?: string | null) => void;
  setFocused: (key: string | null) => void;
  setPreview: (id: number | null) => void;
  toggleExpanded: (key: string, open?: boolean) => void;
  setViewMode: (m: ViewMode) => void;
  setFlat: (flat: boolean) => void;
  setSort: (sort: Sort) => void;
  setFilters: (f: Partial<Filters>) => void;
  setColumns: (c: Partial<Columns>) => void;
  setPackJob: (p: PackProgress | null) => void;
  setTheme: (t: Theme) => void;
  openSettings: (tab?: SettingsTab) => void;
  closeSettings: () => void;
  setPaletteOpen: (open: boolean) => void;
  setScan: (p: ScanProgress) => void;
  clearScan: (libraryId: number) => void;
}

function sameLocation(a: Location, b: Location): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

export const useUi = create<UiState>()(
  persist(
    (set, get) => ({
      location: { kind: "none" },
      lastBrowse: { kind: "none" },
      history: [],
      searchText: "",
      selection: [],
      anchor: null,
      focused: null,
      previewModelId: null,
      expanded: [],
      viewMode: "list",
      flat: false,
      sort: { key: "name", desc: false },
      filters: { types: [], tagIds: [] },
      columns: { type: false, size: true, modified: true, location: true, tags: true },
      packJob: null,
      theme: "dark",
      settingsOpen: false,
      settingsTab: "libraries",
      paletteOpen: false,
      scans: {},

      navigate: (loc) => {
        const cur = get().location;
        if (sameLocation(cur, loc)) return;
        const isSearch = loc.kind === "search";
        set({
          location: loc,
          lastBrowse: isSearch ? get().lastBrowse : loc,
          history:
            cur.kind === "none" || cur.kind === "search" || isSearch
              ? get().history
              : [...get().history.slice(-49), cur],
          searchText: isSearch ? loc.query : "",
          selection: [],
          anchor: null,
          focused: null,
        });
      },
      back: () => {
        const h = get().history;
        const prev = h[h.length - 1];
        if (!prev) return;
        set({
          location: prev,
          lastBrowse: prev,
          history: h.slice(0, -1),
          searchText: "",
          selection: [],
          anchor: null,
          focused: null,
        });
      },
      setSearchText: (q) => {
        if (q.trim() === "") {
          set({
            searchText: q,
            location: get().lastBrowse,
            selection: [],
            focused: null,
          });
        } else {
          set({
            searchText: q,
            location: { kind: "search", query: q },
            selection: [],
            focused: null,
          });
        }
      },
      setSelection: (keys, anchor) =>
        set({ selection: keys, ...(anchor !== undefined ? { anchor } : {}) }),
      setFocused: (key) => set({ focused: key }),
      setPreview: (id) => set({ previewModelId: id }),
      toggleExpanded: (key, open) => {
        const cur = get().expanded;
        const isOpen = cur.includes(key);
        const want = open ?? !isOpen;
        if (want === isOpen) return;
        set({ expanded: want ? [...cur, key] : cur.filter((k) => k !== key) });
      },
      setViewMode: (m) => set({ viewMode: m }),
      setFlat: (flat) => set({ flat, selection: [], anchor: null, focused: null }),
      setSort: (sort) => set({ sort }),
      setFilters: (f) => set({ filters: { ...get().filters, ...f } }),
      setColumns: (c) => set({ columns: { ...get().columns, ...c } }),
      setPackJob: (p) => set({ packJob: p }),
      setTheme: (t) => set({ theme: t }),
      openSettings: (tab) => set({ settingsOpen: true, ...(tab ? { settingsTab: tab } : {}) }),
      closeSettings: () => set({ settingsOpen: false }),
      setPaletteOpen: (open) => set({ paletteOpen: open }),
      setScan: (p) => set({ scans: { ...get().scans, [p.libraryId]: p } }),
      clearScan: (libraryId) =>
        set({
          scans: Object.fromEntries(
            Object.entries(get().scans).filter(([id]) => Number(id) !== libraryId),
          ),
        }),
    }),
    {
      name: "stlx-ui",
      partialize: (s) => ({
        location: s.location.kind === "search" ? s.lastBrowse : s.location,
        lastBrowse: s.lastBrowse,
        expanded: s.expanded,
        viewMode: s.viewMode,
        flat: s.flat,
        sort: s.sort,
        filters: s.filters,
        columns: s.columns,
        theme: s.theme,
      }),
    },
  ),
);
