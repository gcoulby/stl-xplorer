// Mirrors src-tauri/src/types.rs. Keep the two in step.

export interface Library {
  id: number;
  name: string;
  path: string;
  enabled: boolean;
  isVault: boolean;
  lastScannedAt: number | null;
  rootContainerId: number | null;
  modelCount: number;
  issueCount: number;
  pathExists: boolean;
}

export interface Rotation {
  x: number;
  y: number;
  z: number;
}

export type ContainerKind = "folder" | "zip" | "7z" | "rar";
export type ContainerStatus = "ok" | "missing_parts" | "corrupt" | "unsupported";

export interface ContainerInfo {
  id: number;
  libraryId: number;
  parentId: number | null;
  kind: ContainerKind;
  name: string;
  path: string;
  entryPath: string | null;
  partCount: number;
  size: number;
  mtime: number;
  isPack: boolean;
  solid: boolean;
  encrypted: boolean;
  status: ContainerStatus;
  statusDetail: string | null;
  missing: boolean;
  vaulted: boolean;
  originalPath: string | null;
  modelCount: number;
  issueCount: number;
  hasChildren: boolean;
  tagIds: number[];
  rotation: Rotation | null;
  /** Every indexed file below this container, images and PDFs included. */
  fileCount: number;
}

export interface ModelInfo {
  id: number;
  containerId: number;
  libraryId: number;
  name: string;
  ext: string;
  relPath: string;
  size: number;
  mtime: number;
  encrypted: boolean;
  missing: boolean;
  vaulted: boolean;
  inArchive: boolean;
  originalPath: string | null;
  tagIds: number[];
  rotation: Rotation | null;
}

export interface VirtualDir {
  name: string;
  path: string;
  modelCount: number;
  fileCount: number;
  hasChildren: boolean;
}

export interface Crumb {
  containerId: number;
  subpath: string;
  name: string;
  kind: ContainerKind | "dir";
}

export interface BrowseResult {
  container: ContainerInfo;
  subpath: string;
  breadcrumbs: Crumb[];
  dirs: VirtualDir[];
  containers: ContainerInfo[];
  models: ModelInfo[];
  flat: boolean;
  /** Flat mode only: where each model sits, relative to the browsed location. */
  locations: string[];
}

export type RotationSource = "model" | "pack" | "global" | "none";

export interface ResolvedRotation {
  rotation: Rotation;
  source: RotationSource;
}

export interface ModelDetail {
  model: ModelInfo;
  container: ContainerInfo;
  pack: ContainerInfo | null;
  breadcrumbs: Crumb[];
  rotation: ResolvedRotation;
  libraryName: string;
}

export interface Tag {
  id: number;
  name: string;
  colour: string;
  usage: number;
  autoUsage: number;
  keywords: string | null;
}

export interface TagSuggestion {
  word: string;
  containers: number;
  models: number;
}

export interface ProblemArchive {
  container: ContainerInfo;
  libraryName: string;
  folder: string;
  parts: string[];
}

export interface Collection {
  id: number;
  name: string;
  itemCount: number;
}

export interface Slicer {
  id: number;
  name: string;
  exePath: string;
  args: string[];
  isDefault: boolean;
  onePerFile: boolean;
  pathExists: boolean;
}

export interface SlicerInput {
  name: string;
  exePath: string;
  args: string[];
  onePerFile: boolean;
}

export interface SlicerPreset {
  name: string;
  exePath: string;
  alreadyAdded: boolean;
}

export interface BatchItem {
  model: ModelInfo;
  /** Library › folder › archive › inner folder. */
  location: string;
  containerId: number;
  subpath: string;
}

export interface LaunchReport {
  launches: number;
  files: number;
  perFile: boolean;
  message: string | null;
}

export interface SearchHit {
  model: ModelInfo;
  containerName: string;
  containerKind: ContainerKind;
  location: string;
  libraryName: string;
}

export interface TaggedItems {
  containers: ContainerInfo[];
  models: ModelInfo[];
}

export interface ScanProgress {
  libraryId: number;
  libraryName: string;
  phase: "discovering" | "indexing" | "finishing";
  done: number;
  total: number;
  current: string | null;
}

export interface ScanFinished {
  libraryId: number;
  libraryName: string;
  changed: number;
  models: number;
  errors: number;
  deferred: boolean;
  error: string | null;
}

export interface UnpackProgress {
  containerId: number;
  percent: number;
}

export interface SevenZipInfo {
  path: string | null;
  source: string;
  version: string | null;
}

export interface CacheStats {
  solidBytes: number;
  solidArchives: number;
  solidCapBytes: number;
  extractedBytes: number;
  memoryBytes: number;
  cacheDir: string;
}

export interface AppSettings {
  vaultPath: string;
  solidCacheCapBytes: number;
  globalRotation: Rotation | null;
  missingModels: number;
  missingContainers: number;
  dataDir: string;
  autotagOnIndex: boolean;
  autotagSuggestions: boolean;
  dismissedSuggestions: string[];
}

export interface VaultReport {
  moved: string[];
  errors: string[];
}

export interface DeleteReport {
  deleted: string[];
  errors: string[];
}

export interface UnpackedCopy {
  folder: ContainerInfo;
  archive: ContainerInfo;
  files: number;
  matched: number;
  bytes: number;
  mismatched: string[];
}

export interface ZipReport {
  zipped: string[];
  already: string[];
  deleted: string[];
  errors: string[];
}

export interface PackProgress {
  label: string;
  done: number;
  total: number;
}

export interface ResolvedLocation {
  containerId: number;
  subpath: string;
}

export interface AppErrorPayload {
  kind: string;
  message: string;
}
