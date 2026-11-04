import { Grid, OrbitControls } from "@react-three/drei";
import { Canvas, useThree } from "@react-three/fiber";
import { forwardRef, useEffect, useImperativeHandle, useMemo, useRef, useState } from "react";
import * as THREE from "three";

import { useUi } from "@/store/ui";
import type { Rotation } from "@/types";

type OrbitControlsImpl = React.ComponentRef<typeof OrbitControls>;

// Slicers are Z up. Set this before any camera or object is created rather than rotating
// every mesh.
THREE.Object3D.DEFAULT_UP.set(0, 0, 1);

const DEG = Math.PI / 180;
/** Bambu X1/P1 plate, drawn for scale. */
const PLATE = 256;

export interface Dimensions {
  x: number;
  y: number;
  z: number;
  triangles: number;
}

export interface StlCanvasHandle {
  resetCamera: () => void;
}

function cssVar(name: string, fallback: string): string {
  const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return v || fallback;
}

/** Bounding box after rotating. Exact for quarter turns, which is all the UI offers. */
function rotatedBox(geometry: THREE.BufferGeometry, rotation: Rotation): THREE.Box3 {
  if (!geometry.boundingBox) geometry.computeBoundingBox();
  const box = geometry.boundingBox?.clone() ?? new THREE.Box3();
  const m = new THREE.Matrix4().makeRotationFromEuler(
    new THREE.Euler(rotation.x * DEG, rotation.y * DEG, rotation.z * DEG, "XYZ"),
  );
  return box.applyMatrix4(m);
}

function Model({
  geometry,
  rotation,
  wireframe,
}: {
  geometry: THREE.BufferGeometry;
  rotation: Rotation;
  wireframe: boolean;
}) {
  const box = useMemo(() => rotatedBox(geometry, rotation), [geometry, rotation]);
  const theme = useUi((s) => s.theme);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const colour = useMemo(() => cssVar("--mesh", "#7cb8e8"), [theme]);
  return (
    // Sit the rotated model on the bed: lowest point at z = 0, centred in x and y.
    <group position={[0, 0, -box.min.z]}>
      <mesh
        geometry={geometry}
        rotation={[rotation.x * DEG, rotation.y * DEG, rotation.z * DEG]}
        castShadow={false}
      >
        <meshStandardMaterial
          color={colour}
          roughness={0.55}
          metalness={0.05}
          wireframe={wireframe}
          side={THREE.DoubleSide}
        />
      </mesh>
    </group>
  );
}

function Plate({ extent }: { extent: number }) {
  const theme = useUi((s) => s.theme);
  const colours = useMemo(
    () => ({
      cell: cssVar("--grid-cell", "#2a2f3b"),
      section: cssVar("--grid-section", "#3d4456"),
    }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [theme],
  );
  const outline = useMemo(() => {
    const h = PLATE / 2;
    const pts = [
      new THREE.Vector3(-h, -h, 0),
      new THREE.Vector3(h, -h, 0),
      new THREE.Vector3(h, h, 0),
      new THREE.Vector3(-h, h, 0),
      new THREE.Vector3(-h, -h, 0),
    ];
    return new THREE.BufferGeometry().setFromPoints(pts);
  }, []);
  useEffect(() => () => outline.dispose(), [outline]);
  return (
    <>
      <Grid
        rotation={[Math.PI / 2, 0, 0]}
        position={[0, 0, -0.01]}
        args={[10, 10]}
        cellSize={10}
        cellThickness={0.6}
        sectionSize={50}
        sectionThickness={1}
        cellColor={colours.cell}
        sectionColor={colours.section}
        infiniteGrid
        fadeDistance={Math.max(600, extent * 6)}
        fadeStrength={1.5}
      />
      <lineLoop geometry={outline}>
        <lineBasicMaterial color={colours.section} transparent opacity={0.9} />
      </lineLoop>
      <axesHelper args={[Math.max(15, extent * 0.15)]} />
    </>
  );
}

function CameraRig({
  box,
  fitToken,
  controls,
}: {
  box: THREE.Box3 | null;
  fitToken: number;
  controls: React.RefObject<OrbitControlsImpl | null>;
}) {
  const get = useThree((s) => s.get);
  useEffect(() => {
    const { camera: cam, invalidate } = get();
    const camera = cam as THREE.PerspectiveCamera;
    const size = box ? box.getSize(new THREE.Vector3()) : new THREE.Vector3(100, 100, 40);
    const radius = Math.max(size.length() / 2, 5);
    const target = new THREE.Vector3(0, 0, size.z / 2);
    const fov = (camera.fov ?? 35) * DEG;
    const dist = (radius / Math.sin(fov / 2)) * 1.15;
    const dir = new THREE.Vector3(0.9, -1.25, 0.85).normalize();
    camera.position.copy(target.clone().add(dir.multiplyScalar(dist)));
    camera.near = Math.max(0.05, dist / 200);
    camera.far = dist * 200;
    camera.updateProjectionMatrix();
    const c = controls.current;
    if (c) {
      c.target.copy(target);
      c.update();
    } else {
      camera.lookAt(target);
    }
    invalidate();
  }, [box, fitToken, controls, get]);
  return null;
}

export const StlCanvas = forwardRef<
  StlCanvasHandle,
  {
    geometry: THREE.BufferGeometry | null;
    rotation: Rotation;
    wireframe: boolean;
    onDimensions?: (d: Dimensions | null) => void;
  }
>(function StlCanvas({ geometry, rotation, wireframe, onDimensions }, ref) {
  const controls = useRef<OrbitControlsImpl | null>(null);
  const [fitToken, setFitToken] = useState(0);

  const box = useMemo(
    () => (geometry ? rotatedBox(geometry, rotation) : null),
    [geometry, rotation],
  );

  useEffect(() => {
    if (!geometry || !box) {
      onDimensions?.(null);
      return;
    }
    const s = box.getSize(new THREE.Vector3());
    const count = geometry.index
      ? geometry.index.count
      : (geometry.getAttribute("position")?.count ?? 0);
    onDimensions?.({ x: s.x, y: s.y, z: s.z, triangles: Math.round(count / 3) });
  }, [geometry, box, onDimensions]);

  useImperativeHandle(ref, () => ({
    resetCamera: () => setFitToken((t) => t + 1),
  }));

  const extent = box ? box.getSize(new THREE.Vector3()).length() : 200;

  return (
    <Canvas
      frameloop="demand"
      dpr={[1, 2]}
      gl={{ antialias: true, alpha: true, powerPreference: "high-performance" }}
      camera={{ fov: 35, position: [180, -240, 170], near: 0.5, far: 50000, up: [0, 0, 1] }}
      onCreated={({ gl }) => gl.setClearColor(0x000000, 0)}
    >
      <hemisphereLight args={["#ffffff", "#3b4252", 1.25]} />
      <directionalLight position={[120, -180, 260]} intensity={1.7} />
      <directionalLight position={[-200, 160, 90]} intensity={0.55} />
      <directionalLight position={[0, 0, -200]} intensity={0.25} />
      <Plate extent={extent} />
      {geometry && <Model geometry={geometry} rotation={rotation} wireframe={wireframe} />}
      <CameraRig box={box} fitToken={fitToken} controls={controls} />
      <OrbitControls
        ref={controls}
        makeDefault
        enableDamping
        dampingFactor={0.12}
        screenSpacePanning
        zoomSpeed={1.1}
      />
    </Canvas>
  );
});
