import { convertFileSrc, invoke } from "@tauri-apps/api/core";

import type {
  AppErrorPayload,
  AppSettings,
  BrowseResult,
  CacheStats,
  Collection,
  DeleteReport,
  ContainerInfo,
  LaunchReport,
  Library,
  ModelDetail,
  Rotation,
  SearchHit,
  SevenZipInfo,
  Slicer,
  SlicerInput,
  SlicerPreset,
  ProblemArchive,
  ResolvedLocation,
  UnpackedCopy,
  ZipReport,
  Tag,
  TagSuggestion,
  TaggedItems,
  VaultReport,
} from "@/types";

export class ApiError extends Error {
  kind: string;
  constructor(payload: AppErrorPayload) {
    super(payload.message);
    this.kind = payload.kind;
  }
}

function isPayload(e: unknown): e is AppErrorPayload {
  return typeof e === "object" && e !== null && "message" in e && "kind" in e;
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    if (isPayload(e)) throw new ApiError(e);
    throw new ApiError({ kind: "other", message: String(e) });
  }
}

export function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (isPayload(e)) return e.message;
  return String(e);
}

/** URL the viewer fetches mesh bytes from. Tauri maps the scheme per platform. */
export function meshUrl(modelId: number): string {
  return convertFileSrc(String(modelId), "mesh");
}

export const api = {
  listLibraries: () => call<Library[]>("list_libraries"),
  addLibrary: (path: string, name?: string) => call<Library>("add_library", { path, name }),
  updateLibrary: (id: number, patch: { name?: string; enabled?: boolean }) =>
    call<Library>("update_library", { id, ...patch }),
  removeLibrary: (id: number) => call<null>("remove_library", { id }),
  rescanLibrary: (id: number) => call<null>("rescan_library", { id }),
  rescanAll: () => call<null>("rescan_all"),
  runningScans: () => call<number[]>("running_scans"),

  browse: (containerId: number, subpath: string, flat = false) =>
    call<BrowseResult>("browse", { containerId, subpath, flat }),
  problemArchives: () => call<ProblemArchive[]>("problem_archives"),
  archiveProblem: (id: number) => call<ProblemArchive>("archive_problem", { id }),
  revealInFolder: (target: { containerId?: number; modelId?: number }) =>
    call<null>("reveal_in_folder", target),
  openExternally: (modelId: number) => call<null>("open_externally", { modelId }),
  openProjectPage: (page: "home" | "releases" | "issues" | "licence") =>
    call<null>("open_project_page", { page }),
  deleteItems: (modelIds: number[], containerIds: number[]) =>
    call<DeleteReport>("delete_items", { modelIds, containerIds }),
  findUnpacked: (scope: number | null) => call<UnpackedCopy[]>("find_unpacked", { scope }),
  deleteUnpacked: (folderIds: number[]) => call<DeleteReport>("delete_unpacked", { folderIds }),
  zipFolders: (containerIds: number[], deleteAfter: boolean) =>
    call<ZipReport>("zip_folders", { containerIds, deleteAfter }),
  resolveLocation: (path: string) => call<ResolvedLocation>("resolve_location", { path }),
  setAutotagOnIndex: (enabled: boolean) => call<null>("set_autotag_on_index", { enabled }),
  applyAutoTags: () => call<number>("apply_auto_tags"),
  setAutotagSuggestions: (enabled: boolean) => call<number>("set_autotag_suggestions", { enabled }),
  dismissSuggestion: (word: string) => call<null>("dismiss_suggestion", { word }),
  restoreSuggestions: () => call<null>("restore_suggestions"),
  modelDetail: (id: number) => call<ModelDetail>("model_detail", { id }),
  containerDetail: (id: number) => call<ContainerInfo>("container_detail", { id }),
  setModelRotation: (modelIds: number[], rotation: Rotation | null) =>
    call<null>("set_model_rotation", { modelIds, rotation }),
  setPackRotation: (containerId: number, rotation: Rotation | null) =>
    call<null>("set_pack_rotation", { containerId, rotation }),
  setPack: (containerIds: number[], isPack: boolean) =>
    call<null>("set_pack", { containerIds, isPack }),
  cancelPreview: () => call<null>("cancel_preview"),

  search: (query: string, limit?: number) => call<SearchHit[]>("search", { query, limit }),

  listSlicers: () => call<Slicer[]>("list_slicers"),
  detectSlicers: () => call<SlicerPreset[]>("detect_slicers"),
  addSlicer: (input: SlicerInput) => call<Slicer[]>("add_slicer", { input }),
  updateSlicer: (id: number, input: SlicerInput) => call<Slicer[]>("update_slicer", { id, input }),
  deleteSlicer: (id: number) => call<Slicer[]>("delete_slicer", { id }),
  setDefaultSlicer: (id: number) => call<Slicer[]>("set_default_slicer", { id }),
  openInSlicer: (modelIds: number[], slicerId?: number) =>
    call<LaunchReport>("open_in_slicer", { modelIds, slicerId }),

  listTags: () => call<Tag[]>("list_tags"),
  createTag: (name: string, colour?: string, keywords?: string) =>
    call<Tag>("create_tag", { name, colour, keywords }),
  updateTag: (id: number, patch: { name?: string; colour?: string; keywords?: string }) =>
    call<Tag[]>("update_tag", { id, ...patch }),
  suggestTags: () => call<TagSuggestion[]>("suggest_tags"),
  deleteTag: (id: number) => call<Tag[]>("delete_tag", { id }),
  tagItems: (tagId: number, modelIds: number[], containerIds: number[], add: boolean) =>
    call<null>("tag_items", { tagId, modelIds, containerIds, add }),
  taggedItems: (tagId: number) => call<TaggedItems>("tagged_items", { tagId }),

  listCollections: () => call<Collection[]>("list_collections"),
  createCollection: (name: string) => call<Collection>("create_collection", { name }),
  renameCollection: (id: number, name: string) =>
    call<Collection[]>("rename_collection", { id, name }),
  deleteCollection: (id: number) => call<Collection[]>("delete_collection", { id }),
  collectionAdd: (id: number, containerIds: number[], add: boolean) =>
    call<null>("collection_add", { id, containerIds, add }),
  collectionItems: (id: number) => call<ContainerInfo[]>("collection_items", { id }),

  vaultSave: (modelIds: number[], containerIds: number[]) =>
    call<VaultReport>("vault_save", { modelIds, containerIds }),

  getSettings: () => call<AppSettings>("get_settings"),
  setVaultPath: (path: string) => call<null>("set_vault_path", { path }),
  setSolidCacheCap: (bytes: number) => call<null>("set_solid_cache_cap", { bytes }),
  setGlobalRotation: (rotation: Rotation | null) => call<null>("set_global_rotation", { rotation }),
  sevenzipInfo: () => call<SevenZipInfo>("sevenzip_info"),
  cacheStats: () => call<CacheStats>("cache_stats"),
  clearCache: () => call<null>("clear_cache"),
  cleanUpMissing: () => call<number>("clean_up_missing"),
};
