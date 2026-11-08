// docs/sdk.feature S1b (M1): a Bevy-named entry point, re-exporting what today lives in the files named. Sources move
// into it at M5; until then the old subpaths and this one export the same values.
// raea/mesh (bevy_mesh, bevy_shape): shapes and solids.
export { Sphere, Cylinder, Torus, Cone, Capsule, BufferMesh, type SphereProps, type CylinderProps, type TorusProps, type ConeProps, type CapsuleProps, type BufferMeshProps, type Animate3d } from "./3d";
export * from "@raea/render/solid";
export { Mesh3d, ConicalFrustum, type Mesh3dProps, type ConicalFrustumProps } from "@raea/render/bevy3d";
export { Cuboid, Capsule3d, Plane3d, type CuboidProps, type Capsule3dProps, type Plane3dProps } from "./3d";
// a mesh's own vertex attributes (Mesh3d attributes) and the ones a Material's vertex buffer reads (vertex_attributes)
export { MeshVertexAttribute, ATTRIBUTE_POSITION, ATTRIBUTE_NORMAL, ATTRIBUTE_UV_0, ATTRIBUTE_UV_1, ATTRIBUTE_TANGENT, ATTRIBUTE_COLOR, type VertexFormat, type MeshAttribute } from "./3d";
export { MeshSequence, type MeshSequenceProps } from "@raea/physics/physics";
export { Mesh2d, type Mesh2dProps } from "@raea/render/bevy3d";
// one mesh or glTF drawn at many placements, batched by the GPU, and the seeded placements for it
export { Instances, scatter, type Instance, type InstancesProps, type PlanBox, type ScatterOptions } from "./3d";
