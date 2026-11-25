// raea cloud's pure parts, checked without a container: node test/units.ts
// (gpu.ts reads vulkaninfo's summary and picks the device; offload.ts plans which project a render sends and how its
// paths read on the worker)

import assert from "node:assert/strict"
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs"
import os from "node:os"
import path from "node:path"
import { parseSummary, pick } from "../server/gpu.ts"
import { plan, projectRoot } from "../server/offload.ts"
import { shellEnv } from "../server/shell-os.ts"

const L4 = `Devices:
========
GPU0:
	apiVersion         = 1.3.289
	driverVersion      = 580.95.5.0
	vendorID           = 0x10de
	deviceID           = 0x27b8
	deviceType         = PHYSICAL_DEVICE_TYPE_DISCRETE_GPU
	deviceName         = NVIDIA L4
	driverID           = DRIVER_ID_NVIDIA_PROPRIETARY
	driverName         = NVIDIA
	driverInfo         = 580.95.05
GPU1:
	apiVersion         = 1.4.305
	vendorID           = 0x10005
	deviceType         = PHYSICAL_DEVICE_TYPE_CPU
	deviceName         = llvmpipe (LLVM 19.1.7, 256 bits)
	driverName         = llvmpipe
	driverInfo         = Mesa 25.0.7-2 (LLVM 19.1.7)
`
const gpus = parseSummary(L4)
assert.equal(gpus.length, 2)
assert.deepEqual(gpus[0], { name: "NVIDIA L4", type: "discrete", driver: "NVIDIA 580.95.05", vendor: "0x10de" })
assert.equal(gpus[1].type, "cpu")
assert.deepEqual(pick(gpus), { renders: "gpu", name: "NVIDIA L4" })
assert.deepEqual(pick([gpus[1]]), { renders: "cpu", name: "llvmpipe (LLVM 19.1.7, 256 bits)" })
assert.deepEqual(pick([]), { renders: "cpu", name: "llvmpipe" })

const root = mkdtempSync(path.join(os.tmpdir(), "raea-units-"))
try {
  const proj = path.join(root, "work", "card")
  mkdirSync(path.join(proj, "scenes"), { recursive: true })
  writeFileSync(path.join(proj, "package.json"), "{}")
  writeFileSync(path.join(proj, "scenes", "edit.tsx"), "")
  writeFileSync(path.join(proj, "words.json"), "[]")
  writeFileSync(path.join(root, "work", "elsewhere.json"), "[]")
  const work = path.join(root, "work")

  // the composition's project is the nearest folder above it with a project's files
  assert.equal(projectRoot("card/scenes/edit.tsx", work, root), proj)
  // from the workspace root: paths made relative to the project, the flags' values left alone
  assert.deepEqual(plan(["render", "card/scenes/edit.tsx", "-o", "card/out/x.mp4", "--frames", "30"], work), {
    root: proj,
    args: ["render", "scenes/edit.tsx", "-o", "out/x.mp4", "--frames", "30"],
    moves: [],
  })
  // an output outside the project is written under .raea-cloud-out and moved after
  const p = plan(["render", "scenes/edit.tsx", "-o", "/tmp/x.mp4", "--words", "words.json"], proj)
  assert.ok(typeof p !== "string")
  assert.deepEqual(p.args, ["render", "scenes/edit.tsx", "-o", ".raea-cloud-out/0-x.mp4", "--words", "words.json"])
  assert.deepEqual(p.moves, [{ from: ".raea-cloud-out/0-x.mp4", to: "/tmp/x.mp4" }])
  // an input outside the project can't go
  assert.match(plan(["render", "scenes/edit.tsx", "-o", "x.mp4", "--words", "../elsewhere.json"], proj) as string, /outside the project/)
  // no composition: nothing to send
  assert.match(plan(["render", "--help"], proj) as string, /names no composition/)
  // RAEA_CLOUD_GPU_ROOT widens what's sent
  assert.equal((plan(["render", "card/scenes/edit.tsx", "-o", "x.mp4", "--words", "elsewhere.json"], work, ".") as { root: string }).root, work)
} finally {
  rmSync(root, { recursive: true, force: true })
}

// the box's shell on Windows: its own folder first on PATH, whatever case Windows gave the name; untouched elsewhere
assert.deepEqual(shellEnv({ Path: "C:\\w" }, true, "C:\\g\\usr\\bin\\sh.exe"), { Path: "C:\\g\\usr\\bin;C:\\w" })
assert.deepEqual(shellEnv({ PATH: "C:\\w" }, true, "bash"), { PATH: "C:\\w" })
assert.deepEqual(shellEnv({ PATH: "/usr/bin" }, false, "/bin/bash"), { PATH: "/usr/bin" })
console.log("units: ok")
