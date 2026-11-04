# STL Xplorer

A desktop app for browsing, previewing and printing 3D models straight out of the packs they
were downloaded in. Point it at your model folders and it lets you look inside zip, 7z and rar
archives (split ones too), spin any STL in a 3D viewer, flick through the preview images and
PDF guides that came with it, and send the files you want to your slicer. Nothing needs
unpacking by hand.

Runs on Windows. The code is written to work on macOS too, but that has not been tested yet.

## Download

Get the Windows installer from the [latest release](https://github.com/gcoulby/stl-xplorer/releases/latest). It includes 7-Zip, so 7z and rar archives work straight away. The installer is not code-signed yet, so Windows SmartScreen may warn about an unrecognised app. Choose **More info → Run anyway**.

## Why it exists

Model packs usually arrive as archives, often split into several parts. Seeing what was in one
used to mean unzipping it (and leaving gigabytes of loose files behind), then dragging STL after
STL into Bambu Studio to see what each one was. The tool I had for this, STL File Manager,
worked, but it looked dated and could not see inside archives.

STL Xplorer does that job:

- **No manual unpacking.** Archives show up as folders. Opening a model reads it straight from
  the archive into memory. Solid 7z and rar archives are unpacked once into a cache with a size
  limit, and old entries are cleared out automatically.
- **No slicer round trip.** Click a file and it appears in the viewer, sitting on a build plate
  the way it would land in the slicer, with its size in millimetres.
- **No lost downloads.** If your downloads folder cleans itself up, **Save** moves a file or
  pack into a vault folder that nothing ever deletes.

## Using it

### Libraries

Open **Settings → Libraries** (the cog, top right) and add the folders you keep models in, for
example your downloads folder and wherever you file finished packs. Each library is indexed in
the background. The status bar shows progress and you can keep browsing while it runs. After
that, startup is instant and only changed files are looked at again. A file watcher keeps
everything up to date while the app is open.

### Browsing

- The tree on the left shows your libraries, folders and archives. Right-click anything in it
  (or in the list) for its actions. Archives look like folders
  with an amber icon.
- The middle pane lists what is in the selected folder. Use **Folders** to step through
  subfolders one level at a time, or **All files** to see everything below that point in one
  list, with where each file sits.
- The address bar above the list shows where you are. Click a part of it to jump there, or
  click the empty space (or press **Ctrl L**) to type or paste a path, including folders
  inside an archive.
- **Filter** narrows the list to file types (STL, 3MF, OBJ, images, PDFs) or tags. **Sort**
  orders by name, size, last modified or type, and so does clicking a column heading. The
  columns button picks which columns the list shows.
- Switch between list and grid with the buttons at the top right of the list.
- Badges on archives tell you about them up front:
  - **Solid** means the first preview takes a moment while the pack is unpacked into the cache.
  - The **lock** means it is password protected.
  - An amber warning means part of a split archive is missing.

### Previewing

Click a model to see it in the viewer.

- **Moving the camera:** drag to orbit, right-drag to pan, scroll to zoom. Press **R** to
  reset the view.
- **Orientation:** the X, Y and Z buttons rotate the model 90° at a time. Use the save menu
  to keep the orientation for this model, for every model in the pack, or as the default for
  everything.
- **Images and PDFs** open in the same pane. **Open with default app** opens them in your usual
  image viewer or PDF reader.

### Sending to a slicer

Add your slicers in **Settings → Slicers**. Bambu Studio, OrcaSlicer and PrusaSlicer are
detected automatically if they are installed. Any other program can be added by pointing at its
executable. Mark one as the default.

Select one or more models (Ctrl-click, Shift-click, or Space) and press **Enter**, or use
**Open in …** in the bar at the bottom. Files inside archives are extracted to a temporary
cache first, only the ones you picked, and handed to the slicer in a single launch. Images and
PDFs in a selection are left out.

### Search, tags and collections

- **Search.** Press **/** to search file names, pack names, folders and tags as you type.
  **Ctrl K** opens the command palette.
- **Tags.** Right-click anything to tag it. The sidebar **Tags** section lists your tags, and
  the tags button there shows all of them with how many items each has.
- **Auto tags.** In **Settings → Tags**, give a tag some keywords and it is applied to every
  file whose folder, archive or file name contains one of them. New downloads are tagged as
  they are indexed.
  - Keywords are separated by commas.
  - A keyword ending in `*` matches the start of a word, so `ork*` also catches orks and orkz.
  - The suggestions list offers words that crop up across many of your folder names. Turn on
    **Create these tags automatically** and any word found in three or more folder or archive
    names becomes a tag by itself, including as new downloads are indexed. Dismiss a suggestion
    with × (or delete its tag) and it never comes back.
  - Tags you add by hand are never removed by a rule.
  - Switch off **Tag new files while indexing** to apply rules only when you press **Apply
    rules to everything now**.
- **Collections** group packs together. Right-click a folder or archive and choose **Add to
  collection**.

### Archives that need attention

If an archive cannot be read, the status bar says how many need attention. Click it for the
list. Opening one of them explains the problem and how to fix it. For example, a split archive
lists which parts are there, so you can download the missing ones. **Show in folder** takes you
to the files.

### Tidying up unpacked folders

- **Find unpacked copies** (Settings → Maintenance, the command palette, or right-click a
  folder) lists folders that sit next to an archive of the same name and whose every file is
  inside it. Tick the ones to remove and they go to the Recycle Bin. Folders with anything the
  archive lacks are shown but cannot be ticked.
- **Zip folder** (right-click one or more folders) writes `<folder>.zip` beside each folder. The
  zip is read back and compared with the folder before the folder is removed. If a zip with that
  name already exists and matches, it is used as it is. If it differs, both are left alone.
- **Delete** (right-click, the bin icon or the Delete key) sends files, folders and archives to
  the Recycle Bin after asking first. Split archives go with all their parts.

### The vault

**Save** (in the viewer, the selection bar or the right-click menu) moves a file into the vault.
If the file is inside an archive, the whole archive moves, with every part of a split archive
moving together. The vault location is set in **Settings → Vault**, and the vault always appears
as a library. Nothing in it is ever deleted automatically, and a failed move always leaves the
original where it was.

### Keyboard

| Key       | Action                              |
| --------- | ----------------------------------- |
| ↑ ↓ ← →   | Move through files                  |
| Space     | Add to or remove from the selection |
| Enter     | Open in the default slicer          |
| Backspace | Up one folder                       |
| Delete    | Delete (asks first)                 |
| Ctrl L    | Type a path                         |
| /         | Search                              |
| Ctrl K    | Command palette                     |
| R         | Reset the camera                    |

## Building from source

```bash
pnpm install
pnpm tauri dev      # run with hot reload
pnpm tauri build    # make an installer
```

Needs Node 20+, pnpm, and a Rust toolchain (1.89 or newer). The lockfile pins Tauri 2.11, the
last release that builds on Rust 1.89. After a `rustup update`, delete `src-tauri/Cargo.lock` to
move to newer versions, and keep the `@tauri-apps/*` npm packages on the same minor version as
the `tauri` crate.

### 7-Zip

Plain zips are read natively. 7z, rar and split archives go through 7-Zip. The app looks for
7-Zip in this order, and shows which one it is using under **Settings → Archives**:

1. A copy bundled with the app (`7zz`).
2. `7zz`, `7z` or `7za` on `PATH`.
3. A normal install (`C:\Program Files\7-Zip`, or Homebrew on macOS).

To ship 7-Zip inside the installer:

```bash
pnpm setup:7zip        # copies your installed 7-Zip into src-tauri/binaries
pnpm build:bundled     # tauri build with the sidecar and its licence
```

On Windows the sidecar is `7z.exe` together with `7z.dll` (`7za.exe` cannot read rar). On
macOS, use the standalone `7zz` from 7-zip.org: `pnpm setup:7zip /path/to/7zz`. 7-Zip is
licensed under the LGPL with the unRAR restriction. Its licence is in
[licences/7-Zip-License.txt](licences/7-Zip-License.txt).

### Releasing

Pushing a version tag builds the Windows installers on GitHub Actions (with 7-Zip bundled) and
publishes them as a release. Bump the version in `package.json`, `src-tauri/Cargo.toml` and
`src-tauri/tauri.conf.json` first.

```bash
git tag v0.1.0
git push origin v0.1.0
```

### Checks

```bash
pnpm typecheck && pnpm lint
cd src-tauri && cargo test && cargo clippy --all-targets -- -D warnings
```

The technical spec the app was built against is in [CLAUDE.md](CLAUDE.md).

## AI assistance

This project was built with substantial help from AI. I wrote the requirements and the design
brief in [CLAUDE.md](CLAUDE.md), and Claude, Anthropic's AI model, wrote most of the code through
Claude Code, working from that brief. I tested it against my own libraries and directed the
changes. The code has been reviewed and checked with automated tests, linting and type
checking. Even so, read it with that in mind before relying on it, and please report anything
that looks wrong.

## Licence

MIT. Copyright © 2026 Graham Coulby. See [LICENSE](LICENSE).
