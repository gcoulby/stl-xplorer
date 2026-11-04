import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

import type { Rotation } from "@/types";

export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}

export function formatBytes(n: number): string {
  if (!Number.isFinite(n) || n <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.min(units.length - 1, Math.floor(Math.log(n) / Math.log(1024)));
  const v = n / 1024 ** i;
  return `${v >= 100 || i === 0 ? v.toFixed(0) : v.toFixed(1)} ${units[i]}`;
}

const dateFmt = new Intl.DateTimeFormat("en-GB", {
  day: "numeric",
  month: "short",
  year: "numeric",
});

export function formatDate(secs: number): string {
  if (!secs) return "";
  return dateFmt.format(new Date(secs * 1000));
}

export function timeAgo(secs: number | null): string {
  if (!secs) return "never";
  const d = Date.now() / 1000 - secs;
  if (d < 60) return "just now";
  if (d < 3600) return `${Math.floor(d / 60)} min ago`;
  if (d < 86400) return `${Math.floor(d / 3600)} h ago`;
  return formatDate(secs);
}

export function plural(n: number, one: string, many = `${one}s`): string {
  return `${n.toLocaleString("en-GB")} ${n === 1 ? one : many}`;
}

export const ZERO_ROTATION: Rotation = { x: 0, y: 0, z: 0 };

export function sameRotation(a: Rotation, b: Rotation): boolean {
  const n = (v: number) => ((Math.round(v) % 360) + 360) % 360;
  return n(a.x) === n(b.x) && n(a.y) === n(b.y) && n(a.z) === n(b.z);
}

export function rotate90(r: Rotation, axis: keyof Rotation): Rotation {
  return { ...r, [axis]: (r[axis] + 90) % 360 };
}

/** Split a user-typed argument string, honouring double quotes. */
export function splitArgs(s: string): string[] {
  const out: string[] = [];
  const re = /"([^"]*)"|(\S+)/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(s)) !== null) out.push(m[1] ?? m[2] ?? "");
  return out;
}

export function joinArgs(args: string[]): string {
  return args.map((a) => (/\s/.test(a) ? `"${a}"` : a)).join(" ");
}

export function isTypingTarget(t: EventTarget | null): boolean {
  if (!(t instanceof HTMLElement)) return false;
  return t.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(t.tagName);
}
