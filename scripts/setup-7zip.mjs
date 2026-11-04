// Copies a local 7-Zip into src-tauri/binaries so it can ship as a sidecar.
//
//   pnpm setup:7zip            # uses an installed 7-Zip, or 7zz on PATH
//   pnpm setup:7zip <path>     # uses the given 7z / 7zz executable
//
// Windows needs 7z.exe and 7z.dll side by side (7za.exe cannot read rar). macOS and Linux use
// the standalone 7zz from https://www.7-zip.org/download.html, which reads rar on its own.
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, chmodSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const binDir = join(root, "src-tauri", "binaries");
const licenceDir = join(root, "licences");

const triple = execFileSync("rustc", ["-vV"], { encoding: "utf8" })
  .split("\n")
  .find((l) => l.startsWith("host:"))
  ?.slice(5)
  .trim();
if (!triple) throw new Error("Could not read the Rust target triple from `rustc -vV`");

const isWin = process.platform === "win32";
const candidates = process.argv[2]
  ? [process.argv[2]]
  : isWin
    ? [
        join(process.env.ProgramFiles ?? "C:\\Program Files", "7-Zip", "7z.exe"),
        join(process.env["ProgramFiles(x86)"] ?? "C:\\Program Files (x86)", "7-Zip", "7z.exe"),
      ]
    : ["/opt/homebrew/bin/7zz", "/usr/local/bin/7zz", "/usr/bin/7zz"];

const exe = candidates.find((p) => existsSync(p));
if (!exe) {
  console.error("No 7-Zip found. Install it, or pass the path to 7z.exe / 7zz.");
  process.exit(1);
}

mkdirSync(binDir, { recursive: true });
const target = join(binDir, `7zz-${triple}${isWin ? ".exe" : ""}`);
copyFileSync(exe, target);
if (!isWin) chmodSync(target, 0o755);
console.log(`sidecar   ${exe} -> ${target}`);

if (isWin) {
  const dll = join(dirname(exe), "7z.dll");
  if (!existsSync(dll)) {
    console.error(`7z.dll not found next to ${exe}; rar and 7z support needs it.`);
    process.exit(1);
  }
  copyFileSync(dll, join(binDir, "7z.dll"));
  console.log(`library   ${dll} -> ${join(binDir, "7z.dll")}`);
}

const licence = join(dirname(exe), "License.txt");
if (existsSync(licence)) {
  mkdirSync(licenceDir, { recursive: true });
  copyFileSync(licence, join(licenceDir, "7-Zip-License.txt"));
  console.log(`licence   ${licence}`);
}
console.log("\nBuild with the sidecar:  pnpm build:bundled");
