//! Structs that cross the Tauri boundary. Mirrored by hand in `src/types/index.ts`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Library {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub enabled: bool,
    pub is_vault: bool,
    pub last_scanned_at: Option<i64>,
    pub root_container_id: Option<i64>,
    pub model_count: i64,
    pub issue_count: i64,
    pub path_exists: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Rotation {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerInfo {
    pub id: i64,
    pub library_id: i64,
    pub parent_id: Option<i64>,
    pub kind: String,
    pub name: String,
    pub path: String,
    pub entry_path: Option<String>,
    pub part_count: i64,
    pub size: i64,
    pub mtime: i64,
    pub is_pack: bool,
    pub solid: bool,
    pub encrypted: bool,
    pub status: String,
    pub status_detail: Option<String>,
    pub missing: bool,
    pub vaulted: bool,
    pub original_path: Option<String>,
    pub model_count: i64,
    pub issue_count: i64,
    pub has_children: bool,
    pub tag_ids: Vec<i64>,
    pub rotation: Option<Rotation>,
    /// Every indexed file below this container, images and PDFs included.
    pub file_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: i64,
    pub container_id: i64,
    pub library_id: i64,
    pub name: String,
    pub ext: String,
    pub rel_path: String,
    pub size: i64,
    pub mtime: i64,
    pub encrypted: bool,
    pub missing: bool,
    pub vaulted: bool,
    pub in_archive: bool,
    pub original_path: Option<String>,
    pub tag_ids: Vec<i64>,
    pub rotation: Option<Rotation>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualDir {
    pub name: String,
    pub path: String,
    pub model_count: i64,
    pub file_count: i64,
    pub has_children: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Crumb {
    pub container_id: i64,
    pub subpath: String,
    pub name: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowseResult {
    pub container: ContainerInfo,
    pub subpath: String,
    pub breadcrumbs: Vec<Crumb>,
    pub dirs: Vec<VirtualDir>,
    pub containers: Vec<ContainerInfo>,
    pub models: Vec<ModelInfo>,
    /// Set in flat mode: every file below this point, with where each one sits.
    pub flat: bool,
    /// One entry per model in flat mode, relative to the browsed location. Empty otherwise.
    pub locations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedRotation {
    pub rotation: Rotation,
    /// model, pack, global or none
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDetail {
    pub model: ModelInfo,
    pub container: ContainerInfo,
    pub pack: Option<ContainerInfo>,
    pub breadcrumbs: Vec<Crumb>,
    pub rotation: ResolvedRotation,
    pub library_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub id: i64,
    pub name: String,
    pub colour: String,
    pub usage: i64,
    /// How many of `usage` came from the keyword rule rather than by hand.
    pub auto_usage: i64,
    /// Comma separated keywords. A trailing `*` matches word prefixes.
    pub keywords: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagSuggestion {
    pub word: String,
    /// Folders and archives whose name contains the word.
    pub containers: i64,
    pub models: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProblemArchive {
    pub container: ContainerInfo,
    pub library_name: String,
    /// Folder the archive sits in.
    pub folder: String,
    /// File names of the parts that were found.
    pub parts: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub id: i64,
    pub name: String,
    pub item_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Slicer {
    pub id: i64,
    pub name: String,
    pub exe_path: String,
    pub args: Vec<String>,
    pub is_default: bool,
    pub one_per_file: bool,
    pub path_exists: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlicerPreset {
    pub name: String,
    pub exe_path: String,
    pub already_added: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchReport {
    pub launches: usize,
    pub files: usize,
    pub per_file: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub model: ModelInfo,
    pub container_name: String,
    pub container_kind: String,
    pub location: String,
    pub library_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaggedItems {
    pub containers: Vec<ContainerInfo>,
    pub models: Vec<ModelInfo>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub library_id: i64,
    pub library_name: String,
    pub phase: String,
    pub done: u64,
    pub total: u64,
    pub current: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanFinished {
    pub library_id: i64,
    pub library_name: String,
    pub changed: u64,
    pub models: u64,
    pub errors: u64,
    pub deferred: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnpackProgress {
    pub container_id: i64,
    pub percent: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SevenZipInfo {
    pub path: Option<String>,
    pub source: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheStats {
    pub solid_bytes: u64,
    pub solid_archives: u64,
    pub solid_cap_bytes: u64,
    pub extracted_bytes: u64,
    pub memory_bytes: u64,
    pub cache_dir: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub vault_path: String,
    pub solid_cache_cap_bytes: u64,
    pub global_rotation: Option<Rotation>,
    pub missing_models: i64,
    pub missing_containers: i64,
    pub data_dir: String,
    /// Apply auto-tag rules to new and changed files while indexing.
    pub autotag_on_index: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultReport {
    pub moved: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteReport {
    pub deleted: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnpackedCopy {
    pub folder: ContainerInfo,
    pub archive: ContainerInfo,
    pub files: u64,
    /// Files found in the archive at the same size. Equal to `files` when safe to delete.
    pub matched: u64,
    pub bytes: u64,
    pub mismatched: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZipReport {
    pub zipped: Vec<String>,
    /// Folders whose zip already existed and matched.
    pub already: Vec<String>,
    pub deleted: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackProgress {
    pub label: String,
    pub done: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedLocation {
    pub container_id: i64,
    pub subpath: String,
}
