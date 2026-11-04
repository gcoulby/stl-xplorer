import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import type * as THREE from "three";
import { STLLoader } from "three/examples/jsm/loaders/STLLoader.js";

import { api, meshUrl } from "@/lib/api";
import { fileKind } from "@/lib/utils";
import type { AppErrorPayload, UnpackProgress } from "@/types";

export type PreviewState =
  | { status: "idle" }
  | { status: "loading"; unpack: number | null }
  | { status: "ready"; geometry: THREE.BufferGeometry; bytes: number }
  | { status: "file"; url: string; kind: "image" | "pdf"; bytes: number }
  | { status: "unsupported"; ext: string }
  | { status: "error"; kind: string; message: string };

type Fetched =
  | { status: "unpacking"; percent: number }
  | { status: "ready"; geometry: THREE.BufferGeometry; bytes: number }
  | { status: "file"; url: string; kind: "image" | "pdf"; bytes: number }
  | { status: "error"; kind: string; message: string };

async function readError(res: Response): Promise<AppErrorPayload> {
  try {
    const body = (await res.json()) as AppErrorPayload;
    if (typeof body.message === "string") return body;
  } catch {
    // not JSON; fall through
  }
  return { kind: "other", message: `Preview failed (${res.status})` };
}

const MIME: Record<string, string> = {
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  webp: "image/webp",
  gif: "image/gif",
  bmp: "image/bmp",
  pdf: "application/pdf",
};

function previewable(ext: string): boolean {
  return ext === "stl" || fileKind(ext) !== "mesh";
}

function parseStl(buf: ArrayBuffer): THREE.BufferGeometry {
  const geometry = new STLLoader().parse(buf);
  // Many exporters write zero or wrong facet normals, which renders the model black or
  // patchy. Work them out from the triangles instead of trusting the file.
  geometry.deleteAttribute("normal");
  geometry.computeVertexNormals();
  geometry.computeBoundingBox();
  geometry.center();
  return geometry;
}

/**
 * Fetch a file through the mesh:// scheme. STL comes back as geometry, images and PDFs as a
 * blob URL the viewer can show directly.
 */
export function usePreview(
  modelId: number | null,
  ext: string | null,
  containerId: number | null,
): PreviewState {
  // Results are tagged with the file they belong to, so a stale one is never shown.
  const [result, setResult] = useState<{ id: number; value: Fetched } | null>(null);
  const wanted = modelId !== null && ext !== null && previewable(ext) ? modelId : null;

  useEffect(() => {
    if (wanted === null || ext === null) return;
    const ctrl = new AbortController();
    let geometry: THREE.BufferGeometry | null = null;
    let url: string | null = null;
    let live = true;
    const put = (value: Fetched) => {
      if (live) setResult({ id: wanted, value });
    };

    // Solid archives unpack once on first preview; show how far along that is.
    const unlisten = listen<UnpackProgress>("unpack-progress", (e) => {
      if (e.payload.containerId === containerId && e.payload.percent < 100) {
        put({ status: "unpacking", percent: e.payload.percent });
      }
    });

    void (async () => {
      try {
        const res = await fetch(meshUrl(wanted), { signal: ctrl.signal });
        if (!res.ok) {
          const err = await readError(res);
          if (err.kind !== "cancelled") put({ status: "error", ...err });
          return;
        }
        const buf = await res.arrayBuffer();
        if (!live) return;
        const kind = fileKind(ext);
        if (kind === "mesh") {
          geometry = parseStl(buf);
          put({ status: "ready", geometry, bytes: buf.byteLength });
        } else {
          url = URL.createObjectURL(new Blob([buf], { type: MIME[ext] ?? "" }));
          put({ status: "file", url, kind, bytes: buf.byteLength });
        }
      } catch (e) {
        if (ctrl.signal.aborted) return;
        put({
          status: "error",
          kind: "parse",
          message:
            e instanceof Error
              ? `Could not read this file: ${e.message}`
              : "Could not read this file",
        });
      }
    })();

    return () => {
      live = false;
      ctrl.abort();
      void unlisten.then((off) => off());
      geometry?.dispose();
      if (url) URL.revokeObjectURL(url);
    };
  }, [wanted, ext, containerId]);

  // Nothing is being previewed any more: stop any 7-Zip process still working for us.
  useEffect(() => {
    if (modelId === null) void api.cancelPreview().catch(() => undefined);
  }, [modelId]);

  if (modelId === null || ext === null) return { status: "idle" };
  if (!previewable(ext)) return { status: "unsupported", ext };
  if (!result || result.id !== modelId) return { status: "loading", unpack: null };
  const v = result.value;
  if (v.status === "unpacking") return { status: "loading", unpack: v.percent };
  return v;
}
