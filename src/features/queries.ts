import { keepPreviousData, useQuery, type QueryClient } from "@tanstack/react-query";

import { api } from "@/lib/api";

export const qk = {
  libraries: ["libraries"] as const,
  browse: (containerId: number, subpath: string, flat = false) =>
    ["browse", containerId, subpath, flat] as const,
  problems: ["problems"] as const,
  problem: (id: number) => ["problems", id] as const,
  suggestions: ["tag-suggestions"] as const,
  model: (id: number) => ["model", id] as const,
  search: (q: string) => ["search", q] as const,
  tags: ["tags"] as const,
  tagged: (id: number) => ["tagged", id] as const,
  collections: ["collections"] as const,
  collectionItems: (id: number) => ["collection-items", id] as const,
  // Under "collection-items" so anything that refreshes members refreshes this too.
  collectionFiles: (id: number) => ["collection-items", id, "files"] as const,
  treeFilter: (q: string) => ["tree-filter", q] as const,
  slicers: ["slicers"] as const,
  presets: ["slicer-presets"] as const,
  settings: ["settings"] as const,
  sevenzip: ["sevenzip"] as const,
  cache: ["cache-stats"] as const,
  batch: (ids: number[]) => ["batch", ids] as const,
};

export const browseQuery = (containerId: number, subpath: string, flat = false) => ({
  queryKey: qk.browse(containerId, subpath, flat),
  queryFn: () => api.browse(containerId, subpath, flat),
  staleTime: 30_000,
});

export function useLibraries() {
  return useQuery({ queryKey: qk.libraries, queryFn: api.listLibraries });
}

export function useBrowse(containerId: number | null, subpath: string, flat = false) {
  return useQuery({
    ...browseQuery(containerId ?? -1, subpath, flat),
    enabled: containerId !== null,
    placeholderData: keepPreviousData,
  });
}

export function useModelDetail(id: number | null) {
  return useQuery({
    queryKey: qk.model(id ?? -1),
    queryFn: () => api.modelDetail(id ?? -1),
    enabled: id !== null,
  });
}

export function useSearch(query: string) {
  return useQuery({
    queryKey: qk.search(query.trim()),
    queryFn: () => api.search(query.trim()),
    enabled: query.trim().length > 0,
    placeholderData: keepPreviousData,
    staleTime: 10_000,
  });
}

export function useTags() {
  return useQuery({ queryKey: qk.tags, queryFn: api.listTags });
}

export function useTagged(id: number | null) {
  return useQuery({
    queryKey: qk.tagged(id ?? -1),
    queryFn: () => api.taggedItems(id ?? -1),
    enabled: id !== null,
  });
}

export function useProblemArchives(enabled = true) {
  return useQuery({ queryKey: qk.problems, queryFn: api.problemArchives, enabled });
}

export function useArchiveProblem(id: number | null) {
  return useQuery({
    queryKey: qk.problem(id ?? -1),
    queryFn: () => api.archiveProblem(id ?? -1),
    enabled: id !== null,
  });
}

export function useTagSuggestions(enabled: boolean) {
  return useQuery({ queryKey: qk.suggestions, queryFn: api.suggestTags, enabled });
}

export function useCollections() {
  return useQuery({ queryKey: qk.collections, queryFn: api.listCollections });
}

export function useCollectionItems(id: number | null) {
  return useQuery({
    queryKey: qk.collectionItems(id ?? -1),
    queryFn: () => api.collectionItems(id ?? -1),
    enabled: id !== null,
  });
}

export function useCollectionFiles(id: number | null) {
  return useQuery({
    queryKey: qk.collectionFiles(id ?? -1),
    queryFn: () => api.collectionFiles(id ?? -1),
    enabled: id !== null,
    placeholderData: keepPreviousData,
  });
}

export function useTreeFilter(query: string) {
  return useQuery({
    queryKey: qk.treeFilter(query),
    queryFn: () => api.treeFilter(query),
    enabled: query.length > 0,
    placeholderData: keepPreviousData,
    staleTime: 10_000,
  });
}

export function useSlicers() {
  return useQuery({ queryKey: qk.slicers, queryFn: api.listSlicers });
}

export function useSlicerPresets(enabled: boolean) {
  return useQuery({ queryKey: qk.presets, queryFn: api.detectSlicers, enabled });
}

export function useSettings() {
  return useQuery({ queryKey: qk.settings, queryFn: api.getSettings });
}

export function useSevenZip() {
  return useQuery({ queryKey: qk.sevenzip, queryFn: api.sevenzipInfo, staleTime: Infinity });
}

export function useCacheStats(enabled: boolean) {
  return useQuery({ queryKey: qk.cache, queryFn: api.cacheStats, enabled });
}

/** Everything that depends on indexed data. Called after scans and edits. */
export function invalidateIndex(qc: QueryClient) {
  for (const key of [
    "libraries",
    "batch",
    "browse",
    "model",
    "search",
    "tagged",
    "collection-items",
    "tree-filter",
    "settings",
    "problems",
    "tags",
  ]) {
    void qc.invalidateQueries({ queryKey: [key] });
  }
}

export function invalidateTags(qc: QueryClient) {
  for (const key of ["tags", "browse", "model", "search", "tagged", "collection-items"]) {
    void qc.invalidateQueries({ queryKey: [key] });
  }
}
