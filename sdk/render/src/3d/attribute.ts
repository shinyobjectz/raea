// A mesh's vertex attributes by Bevy's name, id and format (docs/control.feature FC2d): a mesh's own (Mesh3d
// `attributes`) and the ones a Material's vertex buffer reads (Material `vertex_attributes`).

/** A vertex attribute's format, as Bevy's VertexFormat names it. */
export type VertexFormat = "Float32" | "Float32x2" | "Float32x3" | "Float32x4" | "Uint32" | "Uint32x2" | "Uint32x3" | "Uint32x4" | "Sint32" | "Sint32x4";

/** A vertex attribute: its name, its unique id (a high random one for your own, as Bevy asks, so it sorts after the
 *  built-in ones) and its format. MeshVertexAttribute.new("BlendColor", 988540917, "Float32x4") is Bevy's
 *  MeshVertexAttribute::new; Mesh's own are ATTRIBUTE_POSITION and the rest.
 *  @bevy bevy_mesh::MeshVertexAttribute */
export interface MeshVertexAttribute { name: string; id: number; format: VertexFormat }

const FORMATS: VertexFormat[] = ["Float32", "Float32x2", "Float32x3", "Float32x4", "Uint32", "Uint32x2", "Uint32x3", "Uint32x4", "Sint32", "Sint32x4"];

export const MeshVertexAttribute = {
  new(name: string, id: number, format: VertexFormat): MeshVertexAttribute {
    if (!name || !Number.isSafeInteger(id) || id < 0) throw new TypeError(`MeshVertexAttribute: a name and a whole-number id, got ${JSON.stringify([name, id])}`);
    if (!FORMATS.includes(format)) throw new TypeError(`MeshVertexAttribute ${name}: format is one of ${FORMATS.join(", ")}, got ${JSON.stringify(format)}`);
    return { name, id, format };
  },
};

/** Bevy's Mesh::ATTRIBUTE_* (ids 0 to 5), as a Material's vertex buffer names them. */
export const ATTRIBUTE_POSITION = MeshVertexAttribute.new("Vertex_Position", 0, "Float32x3");
export const ATTRIBUTE_NORMAL = MeshVertexAttribute.new("Vertex_Normal", 1, "Float32x3");
export const ATTRIBUTE_UV_0 = MeshVertexAttribute.new("Vertex_Uv", 2, "Float32x2");
export const ATTRIBUTE_UV_1 = MeshVertexAttribute.new("Vertex_Uv_1", 3, "Float32x2");
export const ATTRIBUTE_TANGENT = MeshVertexAttribute.new("Vertex_Tangent", 4, "Float32x4");
export const ATTRIBUTE_COLOR = MeshVertexAttribute.new("Vertex_Color", 5, "Float32x4");

/** One attribute's values on a mesh: one a vertex, each a number or a list of the format's size. */
export type MeshAttribute = [MeshVertexAttribute, (number | number[])[]];
