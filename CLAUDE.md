# CLAUDE.md

## What this is

A desktop app for browsing, previewing and launching 3D printing models (mostly STL). It replaces an existing tool called "STL File Manager", which works but looks bad and lacks key features.

The problem it solves: the user downloads lots of model packs, usually as zip, 7z or rar archives (often split into several parts), and currently has to unzip them (wasting disk space) and load each STL into Bambu Studio just to see what it is. This app should let them browse, preview and send files to a slicer without ever extracting anything by hand.

Single user, local only, no accounts, no network features.

## Stack

- **Shell:** Tauri v2 (Rust backend)
- **Frontend:** React, TypeScript (strict), Tailwind CSS, shadcn/ui
- **3D viewer:** three.js via `@react-three/fiber` and `@react-three/drei`, with `STLLoader` from three's examples
- **State:** Zustand for UI state, TanStack Query for anything that comes from Rust commands
- **Database:** SQLite via `rusqlite` (with the `bundled` feature), FTS5 for search
- **Archive reading:** the `zip` crate for plain zips, plus a bundled 7-Zip console binary (`7zz`) as a Tauri sidecar for 7z, rar and multi-part archives (see "Archive handling")
- **Directory walking:** `walkdir`; file watching with `notify`
- **Package manager:** pnpm

Do not add Electron, a Node backend, or an ORM. Keep the Rust side thin and boring.

## Requirements

### Must have (v1)

1. **File browser.** Navigate the folders the user has added as libraries. Tree on the left, grid or list on the right. Fast with thousands of entries (virtualise the lists).
2. **STL viewer.** Click a file, see it in a 3D pane straight away. Orbit, pan, zoom, reset camera. No need to open Bambu Studio to check what a file is.
3. **Open in slicer.** Select one or many files and send them to a chosen slicer in one launch.
4. **Default libraries.** A set of library folders that load on startup. Managed in settings (add, remove, rename, enable or disable).
5. **Custom slicers.** The user can register any slicer by name and executable path, with optional extra arguments. Bambu Studio, OrcaSlicer and PrusaSlicer should be offered as presets if found on the machine. One slicer is marked as default.
6. **Indexing.** On opening a library, scan it and discover its contents. Results go into SQLite so later opens are instant.

### Nice to have (build these, in this priority order)

1. **Read from archives. This is the most needed feature.** Supported: zip, 7z, rar (including RAR5), and multi-part archives (`.zip.001`, `.7z.001`, `.part1.rar`, `.r00`, `.z01` and similar). Archives appear as browsable folders. The user can preview any STL inside one without extracting it by hand. See "Archive handling" below.
2. **Search.** Full text over file names, pack names, folder paths and tags. Fast enough to run on every keystroke.
3. **Auto indexing.** Keep the SQLite database current in the background using a file watcher, plus a rescan on startup that only touches changed files.
4. **Tagging and grouping.** Tags on files and packs. Packs (a zip, or a folder the user marks as a pack) can be grouped into collections.
5. **Default rotation offset.** Most models are oriented wrong in the viewer. Store a rotation correction per model, per pack, and a global default. See "Orientation" below.
6. **Save from deletion.** The user's downloads folder auto-cleans. A "Save" action moves a file or zip into a protected folder the app controls. See "Vault" below.

### Out of scope unless asked

Creating or modifying archives, thumbnails, cloud sync, model editing or repair, print queue management, slicer profile management, mesh analysis. Do not build these speculatively.

## Architecture

```
src-tauri/
  src/
    main.rs
    commands/        # thin Tauri command handlers, one file per area
      library.rs
      index.rs
      mesh.rs
      slicer.rs
      vault.rs
      tags.rs
      search.rs
    db/
      mod.rs         # connection, migrations
      schema.sql
      queries.rs
    index/
      scanner.rs     # walks folders and zips, writes to db
      watcher.rs     # notify-based incremental updates
    archive/
      mod.rs         # ArchiveReader trait
      zip.rs         # native zip backend
      sevenzip.rs    # 7zz sidecar backend (7z, rar, split volumes)
      parts.rs       # detects and groups multi-part archives
      cache.rs       # extraction cache with size cap and LRU eviction
    protocol.rs      # custom URI scheme for mesh bytes
    slicer.rs        # process launching
src/
  components/
    browser/         # tree, file grid, breadcrumbs
    viewer/          # r3f canvas, controls, orientation widget
    layout/          # app shell, panes, command palette
    settings/
    ui/              # shadcn components live here
  features/          # hooks and logic grouped by feature
  lib/               # tauri invoke wrappers, utils
  store/
  types/             # TS types mirroring Rust structs
```

Rust owns the filesystem, database, archives and process launching. The frontend never touches files directly and never guesses paths. All data crosses the boundary through typed commands.

Keep TS types in `src/types` in sync with the Rust structs. If you add `ts-rs` or `specta` to generate them, fine, but pick one and use it everywhere.

## Data model

SQLite file lives in the Tauri app data directory. Use a simple migrations table and numbered SQL files. Sketch:

```sql
libraries(id, name, path, enabled, last_scanned_at)

-- A "container" is a real folder or an archive. kind is 'folder', 'zip', '7z' or 'rar'.
-- For multi-part archives, path is the entry point and part_paths holds every part (JSON array).
-- size and mtime are aggregated across all parts.
containers(id, library_id, parent_id, kind, path, part_paths, name, size, mtime, is_pack,
           solid, encrypted, status)  -- status: ok, missing_parts, corrupt, unsupported

-- A "model" is one mesh file, either on disk or inside a zip.
models(
  id, container_id, library_id,
  rel_path,          -- path on disk, or entry path inside the archive
  name, ext, size, mtime,
  rot_x, rot_y, rot_z,   -- null means inherit
  missing,               -- set when a rescan no longer finds it
  vaulted
)

tags(id, name, colour)
model_tags(model_id, tag_id)
container_tags(container_id, tag_id)

collections(id, name)
collection_items(collection_id, container_id)

slicers(id, name, exe_path, args, is_default)
settings(key, value)

models_fts  -- FTS5 over model name, container name, path, tags
```

Rules:

- Identify files by path plus size plus mtime for change detection. Do not hash whole files during scans, it is too slow for large libraries. Hash lazily only if duplicate detection is ever added.
- Rescans must be incremental. Skip anything whose size and mtime are unchanged.
- When a file disappears, set `missing = 1` rather than deleting the row, so tags and orientation survive a moved folder. Offer a "clean up missing" action.
- Index `.stl` first. Also pick up `.3mf` and `.obj` in the index, but only STL needs to render in v1.

## Archive handling

This is the highest value feature, so get it right. Supported formats: zip, 7z and rar (including RAR5), plus multi-part versions of all three.

### Backends

Put everything behind an `ArchiveReader` trait with `list()`, `read_entry()` and `extract_entries()`. The indexer and UI only know about the trait, so a backend can be swapped later.

- **zip:** native `zip` crate. The central directory gives random access, so reading one entry is cheap.
- **7z, rar and split volumes:** a bundled 7-Zip console binary run as a Tauri sidecar (`src-tauri/binaries/7zz-<target-triple>`). It reads all three formats and handles split volumes itself, so there is no part-joining code to write. Use `7zz l -slt -ba` for machine readable listings, `7zz e -so` to read one entry to stdout, and `7zz x -o<dir>` to extract chosen entries. Pass arguments as an array, never through a shell, and use `-spd` so entry names are matched literally rather than as wildcards.
- **Why a sidecar:** the pure Rust 7z crates do not cover rar, and the `unrar` crate brings awkward licensing and native linking. One binary covers everything. 7-Zip is LGPL with an unRAR restriction (it can read RAR but not be used to build a RAR compressor), which is fine here. Ship its licence file with the app.
- **Fallback:** if the sidecar is missing, look for `7zz` or `7z` on PATH and show which one is in use in settings.
- Limit concurrent sidecar processes (two is plenty) and kill the child if the user navigates away from a preview that is still loading.

### Multi-part archives

Recognise these patterns in `parts.rs`:

- `name.part1.rar`, `name.part01.rar`, `name.part001.rar` and so on (entry point is the first part)
- `name.rar` with `name.r00`, `name.r01`, ... (entry point is the `.rar`)
- `name.7z.001`, `name.7z.002`, ...
- `name.zip.001`, `name.zip.002`, ...
- `name.zip` with `name.z01`, `name.z02`, ... (spanned zip; entry point is the `.zip`, which holds the last segment)
- bare `name.001`, `name.002`, ... (let 7-Zip sniff the format)

Rules:

- Group parts before indexing. A set appears as one container. Individual parts never show up as separate items in the browser.
- Pass the entry point to 7zz. It finds the other parts in the same folder, so all parts must sit together.
- Check that every expected part is present. If there is a gap, set `status = 'missing_parts'` and show "Missing part 3 of 5" on the container. Do not try to preview it, and do not hide it either.
- Change detection uses the size and mtime of every part. When a missing part turns up, the watcher should re-index the set.
- Parts often arrive one at a time into the downloads folder. Do not index a set until its files have been quiet for about 10 seconds, and skip in-progress download files (`.crdownload`, `.part`, `.tmp`, `.download`). Note that a plain `.part` extension is a browser download in progress, while `.part1.rar` is a real archive part. Do not confuse the two.
- Anything that moves or deletes a set (the vault, cleanup) acts on every part together.

### Indexing

- Read headers only. Never decompress file data during indexing. Record entry names, sizes, whether the archive is solid, and whether it is encrypted.
- Nested archives: handle one level deep for zip. For 7z and rar it is optional.
- Index `.stl` first, then `.3mf` and `.obj`.

### Preview

- Serve mesh bytes through the custom URI scheme (`mesh://<model_id>`), not JSON commands. The viewer fetches the URL and hands the buffer to `STLLoader.parse`.
- Keep an in-memory LRU cache of recently decompressed meshes, capped by total bytes (about 256 MB).
- **Non-solid archives** (all zips, some 7z and rar): decompress just the selected entry into memory and stream it to the viewer. Nothing touches the disk.
- **Solid archives** (common for 7z and rar): reading one file means decompressing everything before it in the solid block, so a 2 GB pack could take many seconds per click. Handle it like this:
  - On the first preview in a solid archive, extract all mesh files from it (and only those) into a bounded cache at `<app cache>/archives/<container_id>/`. Show progress in the viewer ("Unpacking pack, 40%").
  - Serve every later preview from that cache.
  - Cap the total cache size (default 5 GB, editable in settings) and evict whole archives least recently used first. Add a "Clear cache" button in settings.
  - Mark solid archives with a small badge so the first-click delay is not a surprise.
  - This keeps disk use bounded and automatic, which is the point. The user's complaint is about manually unpacked folders that never get cleaned up.

### Open in slicer

Slicers need real files. Extract only the selected entries to `<app cache>/extracted/<hash>/` (or reuse the solid cache if they are already there), then launch. Keep original file names and resolve collisions with a numeric suffix. Remove anything older than 7 days on startup. Never extract a whole archive when only some entries were selected.

### Errors and safety

- Corrupt, unsupported or incomplete archives set `status` on the container and show an error on that item. They must never crash a scan or block the rest of the library.
- Encrypted archives: set `encrypted = 1` and show a lock badge. Password prompts are out of scope for v1.
- Non-UTF-8 names: ask 7zz for UTF-8 output (`-sccUTF-8`). For native zip reads, treat names as CP437 only when the UTF-8 flag is unset.
- Path traversal: sanitise every entry path before extracting. Reject absolute paths and any entry containing `..`. Skip symlinks.
- Zip bombs and absurd entries: cap the size of any single in-memory preview (about 500 MB) and abort beyond that.

## Orientation

Most downloaded models are not oriented for printing, and the viewer should reflect how they will land on the bed.

- Resolve rotation in this order: model override, then pack override, then global default. First non-null wins.
- Rotation is stored as Euler angles in degrees, multiples of 90 in the UI. A small widget in the viewer toolbar rotates by 90 degrees per click on each axis, with a "save for this model", "save for this pack" and "reset" choice.
- Z is up, matching slicers. three.js defaults to Y up, so set the camera and scene up vector to Z rather than rotating every mesh.
- Add an "auto orient" button later if wanted: pick the orientation whose largest flat face sits on the bed. Not for v1.
- Show a build plate grid under the model and a bounding box size readout in mm.

## Vault (save from deletion)

The user's downloads folder is cleaned automatically, so files they care about need to move somewhere safe.

- Vault location is set in settings, defaulting to a folder in the user's documents directory.
- "Save" moves the file (or the whole archive, if the file is inside one; for a multi-part archive, every part moves together and a failure on any part rolls the whole set back) into the vault. Preserve the original name; on collision, append a number.
- Use `rename` first. If it fails because the vault is on a different drive, copy, verify size, then delete the original.
- Record the original path in the DB so the user can see where something came from.
- The vault is registered as a library automatically and cannot be removed from the list.
- Vaulted items show a clear badge. Never auto delete anything in the vault.
- Moving a file must never lose it. If any step fails, leave the original in place and report the error.

## Slicer launching

- Launch with `std::process::Command` (or `tauri-plugin-shell` if scoped properly). Detach the process so closing this app does not close the slicer.
- Arguments: `<user args> <file1> <file2> ...`. Paths are passed as separate arguments, never joined into one shell string. No shell interpolation anywhere.
- On macOS, launch `.app` bundles with `open -a <app> --args ...`, or `open -a <app> <files>` where that works better.
- Detect presets by checking standard install locations for Bambu Studio, OrcaSlicer and PrusaSlicer. Allow manual path entry for everything else. Validate that the path exists before saving.
- Multiple files in one launch is a must. If a slicer behaves badly with many arguments, fall back to one launch per file and say so in the UI.

## UI guidelines

The whole reason this exists is that the old tool looks bad. Treat design as a real requirement.

- Use shadcn/ui components and Tailwind. Dark theme first, with a light theme and system option.
- Three pane layout: library tree, file list, viewer. Panes are resizable (`react-resizable-panels`) and remember their sizes.
- A command palette (`cmdk`) on Ctrl/Cmd+K for search and actions.
- Keyboard first: arrow keys move through files, space toggles selection, Enter opens in the default slicer, `/` focuses search.
- Multi-select with shift and ctrl/cmd click. A selection bar shows the count and the "Open in slicer" action.
- Empty states, loading skeletons and error states for every list. No blank panes.
- Show indexing progress in a status bar, not a blocking modal.
- UI text in British English (colour, organise, catalogue).

## Conventions

- TypeScript strict mode, no `any`. Prefer small components and plain hooks.
- Rust: no `unwrap()` outside tests. Commands return `Result<T, AppError>` with a serialisable error type the UI can display.
- Heavy work (scanning, zip reads) runs off the main thread, using `tokio` tasks or `spawn_blocking`. The UI must never freeze during an index.
- Emit progress with Tauri events, not by polling.
- Format with `prettier` and `cargo fmt`. Lint with `eslint` and `cargo clippy`. Fix warnings, don't silence them.
- Do not add a dependency without a reason. Say why in the commit message.
- Write tests for the parts that can quietly go wrong: archive path sanitising, part grouping and missing part detection, incremental scan logic, orientation resolution, vault moves. UI tests are optional.
- Keep comments for why, not what.

## Commands

```bash
pnpm install
pnpm tauri dev        # run the app with hot reload
pnpm tauri build      # production build
pnpm lint
pnpm typecheck
cd src-tauri && cargo test
cd src-tauri && cargo clippy -- -D warnings
```

## Build order

Work in this order and get each step usable before starting the next.

1. Scaffold Tauri v2 with React, TS, Tailwind and shadcn. App shell with the three panes.
2. Libraries in settings, SQLite schema and migrations, folder scanner, file browser over indexed data.
3. STL viewer with the custom URI scheme for plain files on disk. Z up, grid, orbit controls.
4. Slicers in settings and open-in-slicer for a multi-selection of loose files.
5. **Zip support**: the `ArchiveReader` trait, native zip backend, browse zips, preview from memory, extract to cache for slicer launches.
6. **7z, rar and multi-part**: the 7zz sidecar backend, part grouping and missing part detection, solid archive cache with size cap.
7. Search with FTS5 and the command palette.
8. File watcher and incremental rescans, including the quiet period for half-downloaded parts.
9. Tags, packs and collections.
10. Orientation offsets.
11. Vault.
12. Polish pass on design, keyboard shortcuts and error states.

## Things to ask the user rather than guess

- Which operating system(s) it needs to run on. Assume Windows and macOS until told otherwise, and keep path handling cross-platform.
- Whether `.3mf` previews matter. Bambu users often download 3MF, so this is a likely follow-up.
- How large the libraries are (rough file counts). This decides how much effort goes into index performance.
- Whether any archives are password protected. v1 shows a lock badge and skips them.
- Which part naming schemes the user actually meets. Add patterns to `parts.rs` as real examples turn up.
