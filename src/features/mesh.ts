import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import type * as THREE from "three";
import { STLLoader } from "three/examples/jsm/loaders/STLLoader.js";

import { api, meshUrl } from "@/lib/api";
import type { AppErrorPayload, UnpackProgress } from "@/types";

export type MeshState =
  | { status: "idle" }
  | { status: "loading"; unpack: number | null }
  | { status: "ready"; geometry: THREE.BufferGeometry; bytes: number }
  | { status: "unsupported"; ext: string }
  | { status: "error"; kind: string; message: string };

type Fetched =
  | { status: "unpacking"; percent: number }
  | { status: "ready"; geometry: THREE.BufferGeometry; bytes: number }
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

/** Fetch and parse a model's mesh through the mesh:// scheme. */
export function useMesh(
  modelId: number | null,
  ext: string | null,
  containerId: number | null,
): MeshState {
  // Results are tagged with the model they belong to, so a stale one is never shown.
  const [result, setResult] = useState<{ id: number; value: Fetched } | null>(null);
  const wanted = modelId !== null && ext === "stl" ? modelId : null;

  useEffect(() => {
    if (wanted === null) return;
    const ctrl = new AbortController();
    let geometry: THREE.BufferGeometry | null = null;
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
        geometry = new STLLoader().parse(buf);
        geometry.computeBoundingBox();
        geometry.center();
        if (!geometry.getAttribute("normal")) geometry.computeVertexNormals();
        put({ status: "ready", geometry, bytes: buf.byteLength });
      } catch (e) {
        if (ctrl.signal.aborted) return;
        put({
          status: "error",
          kind: "parse",
          message:
            e instanceof Error
              ? `Could not read this STL: ${e.message}`
              : "Could not read this STL",
        });
      }
    })();

    return () => {
      live = false;
      ctrl.abort();
      void unlisten.then((off) => off());
      geometry?.dispose();
    };
  }, [wanted, containerId]);

  // Nothing is being previewed any more: stop any 7-Zip process still working for us.
  useEffect(() => {
    if (modelId === null) void api.cancelPreview().catch(() => undefined);
  }, [modelId]);

  if (modelId === null || ext === null) return { status: "idle" };
  if (ext !== "stl") return { status: "unsupported", ext };
  if (!result || result.id !== modelId) return { status: "loading", unpack: null };
  const v = result.value;
  if (v.status === "unpacking") return { status: "loading", unpack: v.percent };
  return v;
}
