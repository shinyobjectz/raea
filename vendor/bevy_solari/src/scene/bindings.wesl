import bevy_pbr::render::pbr_functions::calculate_tbn_mikktspace;
import bevy_render::maths::affine3_to_square;

enable wgpu_ray_query;
enable wgpu_binding_array;

struct InstanceGeometryIds {
    vertex_buffer_id: u32,
    vertex_buffer_offset: u32,
    index_buffer_id: u32,
    index_buffer_offset: u32,
    triangle_count: u32,
}

struct VertexBuffer { vertices: array<PackedVertex> }

struct IndexBuffer { indices: array<u32> }

// raea: the vertex colour after the tangent (Bevy lays a mesh's attributes out by id: position, normal, uv, tangent,
// colour). A mesh without one is traced white (raea-render-raytrace solari.rs traceable)
struct PackedVertex {
    a: vec4<f32>,
    b: vec4<f32>,
    tangent: vec4<f32>,
    color: vec4<f32>,
}

struct Vertex {
    position: vec3<f32>,
    normal: vec3<f32>,
    uv: vec2<f32>,
    tangent: vec4<f32>,
    color: vec4<f32>,
}

fn unpack_vertex(packed: PackedVertex) -> Vertex {
    var vertex: Vertex;
    vertex.position = packed.a.xyz;
    vertex.normal = vec3(packed.a.w, packed.b.xy);
    vertex.uv = packed.b.zw;
    vertex.tangent = packed.tangent;
    vertex.color = packed.color;
    return vertex;
}

struct Material {
    normal_map_texture_id: u32,
    base_color_texture_id: u32,
    emissive_texture_id: u32,
    metallic_roughness_texture_id: u32,

    base_color: vec3<f32>,
    perceptual_roughness: f32,
    emissive: vec3<f32>,
    metallic: f32,
    // raea: assets.rs GpuMaterial's alpha, its mode (ALPHA_* below) and Mask's cutoff
    alpha: f32,
    alpha_mode: u32,
    alpha_cutoff: f32,
    reflectance: f32,
    // raea: StandardMaterial's uv_transform: columns x (xy) and y (zw), then the translation
    uv_transform_xy: vec4<f32>,
    uv_transform_t: vec2<f32>,
    // raea: StandardMaterial's diffuse_transmission: the share of the diffuse light let through to the far side
    diffuse_transmission: f32,
    // raea: MATERIAL_* (assets.rs)
    flags: u32,
}

// raea: StandardMaterial's double_sided (assets.rs MATERIAL_DOUBLE_SIDED)
const MATERIAL_DOUBLE_SIDED = 1u;

const TEXTURE_MAP_NONE = 0xFFFFFFFFu;

// raea: kept in step with assets.rs ALPHA_*
const ALPHA_OPAQUE = 0u;
const ALPHA_MASK = 1u;
const ALPHA_BLEND = 2u;
const ALPHA_PREMULTIPLIED = 3u;
const ALPHA_ADD = 4u;

// raea: a mesh's UVs as its material's textures read them (the raster passes' uv_transform)
fn material_uv(material: Material, uv: vec2<f32>) -> vec2<f32> {
    return mat2x2(material.uv_transform_xy.xy, material.uv_transform_xy.zw) * uv + material.uv_transform_t.xy;
}

// raea: a material's alpha at a point: its base colour's times its base colour texture's
fn material_alpha(material: Material, uv: vec2<f32>) -> f32 {
    if material.base_color_texture_id == TEXTURE_MAP_NONE { return material.alpha; }
    return material.alpha * textureSampleLevel(textures[material.base_color_texture_id], samplers[material.base_color_texture_id], uv, 0.0).a;
}

// raea: a normal-mapped normal that faces away from the ray that sees it (a grazing view of a bumpy plane) bent back
// just far enough to face it, so the surface isn't shaded from behind (black streaks toward the horizon)
fn facing_normal(world_normal: vec3<f32>, geometric_world_normal: vec3<f32>, wo: vec3<f32>) -> vec3<f32> {
    let min_cos = 0.1;
    let cos_theta = dot(world_normal, wo);
    if cos_theta >= min_cos || dot(geometric_world_normal, wo) <= 0.0 { return world_normal; }
    // the normal plus enough of wo that, once normalized, its cosine to wo is min_cos
    let away = world_normal - cos_theta * wo;
    if dot(away, away) < 1e-8 { return geometric_world_normal; }
    let tangent = normalize(away);
    return normalize(min_cos * wo + sqrt(1.0 - min_cos * min_cos) * tangent);
}

// raea: a hit seen from its back (a leaf's underside, a curtain from the window's side) shaded as its front: both its
// normals turned toward the ray, as raster turns a double-sided material's. A ray found a back face black before (its
// BRDF has nothing below the normal), in the path tracer and in Solari alike. Light passes a thin surface by its
// material's diffuse_transmission (brdf.wesl), never by which side was hit. Emission stays one-sided, as light sampling
// has it: a caller checks the side before the turn. Only a two-sided hit (back_lit) is shaded from behind; a caller
// ends a path at any other back face, the inside of a closed solid, as before
fn face_hit(hit: ResolvedRayHitFull, wo: vec3<f32>) -> ResolvedRayHitFull {
    var faced = hit;
    if dot(faced.geometric_world_normal, wo) < 0.0 {
        faced.geometric_world_normal = -faced.geometric_world_normal;
        faced.world_normal = -faced.world_normal;
    }
    faced.world_normal = facing_normal(faced.world_normal, faced.geometric_world_normal, wo);
    return faced;
}

// raea: where a ray leaving a surface at p starts: off the side it leaves by, along offset_normal (the geometric normal
// at a hit). A ray below the shading normal has passed through (diffuse transmission) and starts off the far side, or
// it would stop on the surface it left
fn offset_ray_origin(p: vec3<f32>, offset_normal: vec3<f32>, shading_normal: vec3<f32>, direction: vec3<f32>) -> vec3<f32> {
    return p + offset_normal * select(-RAY_T_MIN, RAY_T_MIN, dot(shading_normal, direction) >= 0.0);
}

// raea: the share of light a premultiplied surface's clear coat reflects at an angle (Schlick, F0 from its reflectance)
fn coat_albedo(reflectance: f32, cos_theta: f32) -> f32 {
    let f0 = 0.16 * reflectance * reflectance;
    return f0 + (1.0 - f0) * pow(1.0 - saturate(cos_theta), 5.0);
}

const MIRROR_ROUGHNESS_THRESHOLD = 0.001f;

struct LightSource {
    kind: u32, // 1 bit for kind, 31 bits for extra data
    id: u32,
}

const LIGHT_SOURCE_KIND_EMISSIVE_MESH = 0u;
const LIGHT_SOURCE_KIND_DIRECTIONAL = 1u;
// raea: an opening the environment map lights the scene through (SkyPortals); id: its index in environment_map_light
const LIGHT_SOURCE_KIND_SKY_PORTAL = 3u;

struct DirectionalLight {
    direction_to_light: vec3<f32>,
    cos_theta_max: f32,
    luminance: vec3<f32>,
    inverse_pdf: f32,
}

struct EnvironmentMapLight {
    light_from_world: mat3x3<f32>,
    intensity: f32,
    // raea: bind_group.rs GpuEnvironmentMapLight's sun disk and the sky inside it
    sun_direction: vec3<f32>,
    sun_cos: f32,
    sky: vec3<f32>,
    // raea: 1 when some material is see-through (bind_group.rs), else trace_ray skips its second query
    any_alpha: u32,
    // raea: the sky portals (bind_group.rs pack_sky_portals): three rows each, the corner and the area, then the edges
    portal_count: u32,
    portals: array<vec4<f32>, 24>,
}

// raea: a sky portal, the parallelogram corner + a u + b v for a and b from 0 to 1; normal: cross(u, v), unit length
struct SkyPortal {
    corner: vec3<f32>,
    area: f32,
    u: vec3<f32>,
    v: vec3<f32>,
    normal: vec3<f32>,
}

fn sky_portal(id: u32) -> SkyPortal {
    let c = environment_map_light.portals[3u * id];
    let u = environment_map_light.portals[3u * id + 1u].xyz;
    let v = environment_map_light.portals[3u * id + 2u].xyz;
    return SkyPortal(c.xyz, c.w, u, v, normalize(cross(u, v)));
}

const LIGHT_NOT_PRESENT_THIS_FRAME = 0xFFFFFFFFu;

@group(0) @binding(0) var<storage> vertex_buffers: binding_array<VertexBuffer>;
@group(0) @binding(1) var<storage> index_buffers: binding_array<IndexBuffer>;
@group(0) @binding(2) var textures: binding_array<texture_2d<f32>>;
@group(0) @binding(3) var samplers: binding_array<sampler>;
@group(0) @binding(4) var<storage> materials: array<Material>;
@group(0) @binding(5) var tlas: acceleration_structure;
@group(0) @binding(6) var previous_frame_tlas: acceleration_structure;
@group(0) @binding(7) var<storage> transforms: array<mat3x4<f32>>;
@group(0) @binding(8) var<storage> previous_frame_transforms: array<mat3x4<f32>>;
@group(0) @binding(9) var<storage> geometry_ids: array<InstanceGeometryIds>;
@group(0) @binding(10) var<storage> material_ids: array<u32>;
@group(0) @binding(11) var<storage> light_sources: array<LightSource>;
@group(0) @binding(12) var<storage> directional_lights: array<DirectionalLight>;
@group(0) @binding(13) var<storage> previous_frame_light_id_translations: array<u32>;
@group(0) @binding(14) var brdf_dfg_lut: texture_2d<f32>;
@group(0) @binding(15) var brdf_dfg_lut_sampler: sampler;
@group(0) @binding(16) var environment_map: texture_cube<f32>;
@group(0) @binding(17) var environment_map_sampler: sampler;
@group(0) @binding(18) var<storage> environment_map_light: EnvironmentMapLight;

const RAY_T_MIN = 0.001f;
const RAY_T_MAX = 100000.0f;

const RAY_NO_CULL = 0xFFu;

// raea: the TLAS instance masks (instances.rs MASK_*). A ray first finds the nearest opaque instance as Solari always
// did, then walks the instances whose material it may pass through, up to that hit, keeping the first one alpha_accepts.
const RAY_MASK_OPAQUE = 0x01u;
const RAY_MASK_ALPHA = 0x02u;

fn trace_ray(ray_origin: vec3<f32>, ray_direction: vec3<f32>, ray_t_min: f32, ray_t_max: f32, ray_flag: u32) -> RayIntersection {
    var rq: ray_query;
    rayQueryInitialize(&rq, tlas, RayDesc(ray_flag, RAY_MASK_OPAQUE, ray_t_min, ray_t_max, ray_origin, ray_direction));
    rayQueryProceed(&rq);
    let opaque = rayQueryGetCommittedIntersection(&rq);
    if environment_map_light.any_alpha == 0u { return opaque; }
    var t_max = ray_t_max;
    if opaque.kind != RAY_QUERY_INTERSECTION_NONE { t_max = opaque.t; }

    var aq: ray_query;
    rayQueryInitialize(&aq, tlas, RayDesc(ray_flag | RAY_FLAG_FORCE_NO_OPAQUE, RAY_MASK_ALPHA, ray_t_min, t_max, ray_origin, ray_direction));
    while rayQueryProceed(&aq) {
        if alpha_accepts(rayQueryGetCandidateIntersection(&aq), ray_origin, ray_direction) {
            rayQueryConfirmIntersection(&aq);
        }
    }
    let through = rayQueryGetCommittedIntersection(&aq);
    if through.kind != RAY_QUERY_INTERSECTION_NONE { return through; }
    return opaque;
}

fn trace_ray_previous_frame(ray_origin: vec3<f32>, ray_direction: vec3<f32>, ray_t_min: f32, ray_t_max: f32, ray_flag: u32) -> RayIntersection {
    var rq: ray_query;
    rayQueryInitialize(&rq, previous_frame_tlas, RayDesc(ray_flag, RAY_MASK_OPAQUE, ray_t_min, ray_t_max, ray_origin, ray_direction));
    rayQueryProceed(&rq);
    let opaque = rayQueryGetCommittedIntersection(&rq);
    if environment_map_light.any_alpha == 0u { return opaque; }
    var t_max = ray_t_max;
    if opaque.kind != RAY_QUERY_INTERSECTION_NONE { t_max = opaque.t; }

    var aq: ray_query;
    rayQueryInitialize(&aq, previous_frame_tlas, RayDesc(ray_flag | RAY_FLAG_FORCE_NO_OPAQUE, RAY_MASK_ALPHA, ray_t_min, t_max, ray_origin, ray_direction));
    while rayQueryProceed(&aq) {
        // the previous frame's instances keep their slots, so this frame's materials stand in for theirs
        if alpha_accepts(rayQueryGetCandidateIntersection(&aq), ray_origin, ray_direction) {
            rayQueryConfirmIntersection(&aq);
        }
    }
    let through = rayQueryGetCommittedIntersection(&aq);
    if through.kind != RAY_QUERY_INTERSECTION_NONE { return through; }
    return opaque;
}

// raea: whether a ray stops at a candidate hit on a see-through material: Mask by its cutoff, Blend with the
// probability of its alpha (so many rays average to the blend), Premultiplied also for its clear coat's reflection
// (coat_albedo), Add never (it only adds light)
fn alpha_accepts(hit: RayIntersection, ray_origin: vec3<f32>, ray_direction: vec3<f32>) -> bool {
    let instance_id = hit.instance_custom_data;
    let material = materials[material_ids[instance_id]];
    if material.alpha_mode == ALPHA_OPAQUE { return true; }
    if material.alpha_mode == ALPHA_ADD { return false; }

    let vertices = load_vertices(geometry_ids[instance_id], hit.primitive_index);
    let barycentrics = vec3(1.0 - hit.barycentrics.x - hit.barycentrics.y, hit.barycentrics);
    let alpha = material_alpha(material, material_uv(material, mat3x2(vertices[0].uv, vertices[1].uv, vertices[2].uv) * barycentrics));
    if material.alpha_mode == ALPHA_MASK { return alpha >= material.alpha_cutoff; }
    var p = alpha;
    if material.alpha_mode == ALPHA_PREMULTIPLIED {
        let world = transform_positions(affine3_to_square(transforms[instance_id]), vertices);
        let normal = normalize(cross(world[1] - world[0], world[2] - world[0]));
        p = alpha + (1.0 - alpha) * coat_albedo(material.reflectance, abs(dot(normal, ray_direction)));
    }
    return alpha_hash(hit, ray_origin, ray_direction) < p;
}

// raea: a number in [0, 1) that changes with the ray and the hit, for alpha_accepts' coin
fn alpha_hash(hit: RayIntersection, ray_origin: vec3<f32>, ray_direction: vec3<f32>) -> f32 {
    var h = hit.primitive_index * 747796405u + hit.instance_custom_data;
    let parts = array<u32, 6>(bitcast<u32>(hit.t), bitcast<u32>(ray_direction.x), bitcast<u32>(ray_direction.y), bitcast<u32>(ray_direction.z), bitcast<u32>(ray_origin.x), bitcast<u32>(ray_origin.z));
    for (var i = 0u; i < 6u; i++) {
        h = (h ^ parts[i]) * 747796405u + 2891336453u;
        h = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
        h = (h >> 22u) ^ h;
    }
    return f32(h >> 8u) / 16777216.0;
}

fn sample_texture(id: u32, uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(textures[id], samplers[id], uv, 0.0).rgb; // TODO: Mipmap
}

struct ResolvedMaterial {
    base_color: vec3<f32>,
    emissive: vec3<f32>,
    reflectance: f32,
    perceptual_roughness: f32,
    roughness: f32,
    metallic: f32,
    // raea: Material's
    diffuse_transmission: f32,
}

struct ResolvedRayHitFull {
    world_position: vec3<f32>,
    previous_frame_world_position: vec3<f32>,
    world_normal: vec3<f32>,
    geometric_world_normal: vec3<f32>,
    world_tangent: vec4<f32>,
    // raea: whether its back is lit as its front: a double-sided material (a sheet, a leaf, a curtain) or one passing
    // light through (diffuse transmission). Any other material is a closed solid's outside, whose inside (found only by a
    // ray started within a hair of a contact) is dark: a path there ends, as raster's back-face culling has it, rather
    // than bounce round a white sphere's inside a hundred times for nothing
    back_lit: bool,
    uv: vec2<f32>,
    triangle_area: f32,
    triangle_count: u32,
    material: ResolvedMaterial,
}

fn resolve_material(material: Material, uv: vec2<f32>) -> ResolvedMaterial {
    var m: ResolvedMaterial;

    m.base_color = material.base_color.rgb;
    if material.base_color_texture_id != TEXTURE_MAP_NONE {
        m.base_color *= sample_texture(material.base_color_texture_id, uv);
    }

    m.emissive = material.emissive.rgb;
    if material.emissive_texture_id != TEXTURE_MAP_NONE {
        m.emissive *= sample_texture(material.emissive_texture_id, uv);
    }

    m.reflectance = material.reflectance;

    m.perceptual_roughness = material.perceptual_roughness;
    m.metallic = material.metallic;
    if material.metallic_roughness_texture_id != TEXTURE_MAP_NONE {
        let metallic_roughness = sample_texture(material.metallic_roughness_texture_id, uv);
        m.perceptual_roughness *= metallic_roughness.g;
        m.metallic *= metallic_roughness.b;
    }

    m.roughness = m.perceptual_roughness * m.perceptual_roughness;
    m.diffuse_transmission = material.diffuse_transmission;

    return m;
}

fn resolve_ray_hit_full(ray_hit: RayIntersection) -> ResolvedRayHitFull {
    let barycentrics = vec3(1.0 - ray_hit.barycentrics.x - ray_hit.barycentrics.y, ray_hit.barycentrics);
    return resolve_triangle_data_full(ray_hit.instance_custom_data, ray_hit.primitive_index, barycentrics);
}

fn load_vertices(instance_geometry_ids: InstanceGeometryIds, triangle_id: u32) -> array<Vertex, 3> {
    let index_buffer = &index_buffers[instance_geometry_ids.index_buffer_id].indices;
    let vertex_buffer = &vertex_buffers[instance_geometry_ids.vertex_buffer_id].vertices;

    let indices_i = (triangle_id * 3u) + vec3(0u, 1u, 2u) + instance_geometry_ids.index_buffer_offset;
    let indices = vec3((*index_buffer)[indices_i.x], (*index_buffer)[indices_i.y], (*index_buffer)[indices_i.z]) + instance_geometry_ids.vertex_buffer_offset;

    return array<Vertex, 3>(
        unpack_vertex((*vertex_buffer)[indices.x]),
        unpack_vertex((*vertex_buffer)[indices.y]),
        unpack_vertex((*vertex_buffer)[indices.z])
    );
}

fn transform_positions(transform: mat4x4<f32>, vertices: array<Vertex, 3>) -> array<vec3<f32>, 3> {
    return array<vec3<f32>, 3>(
        (transform * vec4(vertices[0].position, 1.0)).xyz,
        (transform * vec4(vertices[1].position, 1.0)).xyz,
        (transform * vec4(vertices[2].position, 1.0)).xyz
    );
}

fn resolve_triangle_data_full(instance_id: u32, triangle_id: u32, barycentrics: vec3<f32>) -> ResolvedRayHitFull {
    let material_id = material_ids[instance_id];
    let material = materials[material_id];

    let transform = affine3_to_square(transforms[instance_id]);
    let previous_frame_transform = affine3_to_square(previous_frame_transforms[instance_id]);

    let instance_geometry_ids = geometry_ids[instance_id];
    let vertices = load_vertices(instance_geometry_ids, triangle_id);

    let world_vertices = transform_positions(transform, vertices);
    let world_position = mat3x3(world_vertices[0], world_vertices[1], world_vertices[2]) * barycentrics;

    let previous_frame_world_vertices = transform_positions(previous_frame_transform, vertices);
    let previous_frame_world_position = mat3x3(previous_frame_world_vertices[0], previous_frame_world_vertices[1], previous_frame_world_vertices[2]) * barycentrics;

    let uv = material_uv(material, mat3x2(vertices[0].uv, vertices[1].uv, vertices[2].uv) * barycentrics);

    let transform_3x3 = mat3x3(transform[0].xyz, transform[1].xyz, transform[2].xyz);

    let cofactor = mat3x3(cross(transform_3x3[1], transform_3x3[2]), cross(transform_3x3[2], transform_3x3[0]), cross(transform_3x3[0], transform_3x3[1]));
    let determinant = dot(transform_3x3[0], cofactor[0]);
    let determinant_sign = select(sign(determinant), 1.0, determinant == 0.0);

    let local_tangent = mat3x3(vertices[0].tangent.xyz, vertices[1].tangent.xyz, vertices[2].tangent.xyz) * barycentrics;
    let world_tangent = vec4(normalize(transform_3x3 * local_tangent), vertices[0].tangent.w * determinant_sign);

    let local_normal = mat3x3(vertices[0].normal, vertices[1].normal, vertices[2].normal) * barycentrics; // TODO: Use barycentric lerp, ray_hit.object_to_world, cross product geo normal
    var world_normal = normalize(determinant_sign * (cofactor * local_normal));

    let geometric_world_normal = world_normal;
    if material.normal_map_texture_id != TEXTURE_MAP_NONE {
        let TBN = calculate_tbn_mikktspace(world_normal, world_tangent);
        let T = TBN[0];
        let B = TBN[1];
        let N = TBN[2];
        var Nt = sample_texture(material.normal_map_texture_id, uv) * 2.0 - 1.0;
        Nt.z = sqrt(max(1.0 - dot(Nt.xy, Nt.xy), 0.0)); // Reconstruct Z to support two-channel normal maps
        world_normal = normalize(Nt.x * T + Nt.y * B + Nt.z * N);
    }

    let triangle_edge0 = world_vertices[0] - world_vertices[1];
    let triangle_edge1 = world_vertices[0] - world_vertices[2];
    let triangle_area = length(cross(triangle_edge0, triangle_edge1)) / 2.0;

    var resolved_material = resolve_material(material, uv);
    // raea: the vertex colour multiplies the base colour, as Bevy's StandardMaterial does in raster
    resolved_material.base_color *= (mat3x4(vertices[0].color, vertices[1].color, vertices[2].color) * barycentrics).rgb;

    return ResolvedRayHitFull(
        world_position,
        previous_frame_world_position,
        world_normal,
        geometric_world_normal,
        world_tangent,
        (material.flags & MATERIAL_DOUBLE_SIDED) != 0u || material.diffuse_transmission > 0.0,
        uv,
        triangle_area,
        instance_geometry_ids.triangle_count,
        resolved_material,
    );
}
