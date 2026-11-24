// A set as blender/lightbake.py left it (docs/bake.feature BK0): one <Model> of its meshes, each with its Cycles
// lightmap and the exposure that puts the bake at raea's brightness, from the .lightmaps.json lightbake.py wrote. A
// mesh lightbake.py left unbaked (a metal) is drawn as raea draws it. `raea bake light --compare` makes media/baked/.
import { staticFile } from "raea";
import { Model } from "raea/3d";
import { Lightmap, StandardMaterial } from "raea/pbr";

export type BakeInfo = { meshes: { mesh: string; material: string[]; map: string; lightmap_exposure: number }[] };

export function Baked({ dir, info }: { dir: string; info: BakeInfo }) {
  return (
    <Model src={staticFile(`${dir}/set.glb`)}>
      {info.meshes.map((m) => (
        <Lightmap key={`l-${m.mesh}`} mesh={m.mesh} image={staticFile(`${dir}/lightmaps/${m.map.split("/").pop()}`)} />
      ))}
      {info.meshes.flatMap((m) => m.material.map((mat) => <StandardMaterial key={`m-${mat}`} material={mat} lightmap_exposure={m.lightmap_exposure} />))}
    </Model>
  );
}
