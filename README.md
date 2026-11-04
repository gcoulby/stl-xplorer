# STL Xplorer

Browse, preview and launch 3D printing models, including files inside zip, 7z and rar archives
(split and multi-part too), without unpacking anything by hand. See [CLAUDE.md](CLAUDE.md) for the
full spec.

## Run it

```bash
pnpm install
pnpm tauri dev
```

Needs Node 20+, pnpm, and a Rust toolchain (1.89 or newer). The lockfile pins Tauri 2.11, the
last release that builds on Rust 1.89. After a `rustup update`, delete `src-tauri/Cargo.lock` to
move to newer versions; keep the `@tauri-apps/*` npm packages on the same minor version as the
`tauri` crate.

## 7-Zip

Plain zips are read natively. 7z, rar and multi-part archives go through 7-Zip. The app looks for
7-Zip in this order, and shows which one it uses under Settings → Archives:

1. A bundled sidecar next to the executable (`7zz`).
2. `7zz`, `7z` or `7za` on `PATH`.
3. A normal install (`C:\Program Files\7-Zip`, Homebrew).

To ship 7-Zip inside the installer:

```bash
pnpm setup:7zip        # copies your installed 7-Zip into src-tauri/binaries
pnpm build:bundled     # tauri build with the sidecar and its licence
```

On Windows the sidecar is `7z.exe` together with `7z.dll` (7za.exe cannot read rar). On macOS,
use the standalone `7zz` from 7-zip.org: `pnpm setup:7zip /path/to/7zz`.

## Checks

```bash
pnpm typecheck && pnpm lint
cd src-tauri && cargo test && cargo clippy --all-targets -- -D warnings
```
