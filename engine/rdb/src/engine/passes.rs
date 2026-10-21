//! A frame's passes in place of its picture (RAEA_PASS), for work done on the frames outside the engine (kgpu's
//! light transport, a learned shading pass): `depth` (metres along the view, in millimetres as 24 bits: r low byte, b
//! high; 0xffffff past everything), `normal` (the world normal as Bevy's prepass keeps it, n x 0.5 + 0.5 a byte a
//! channel; 0 past everything) or `albedo` (every StandardMaterial drawn unlit, untonemapped: its colour and texture).
//! Each is written over the frame after the post-processing, as values the sRGB output encodes back to those exact
//! bytes, so a PNG keeps them: a still as it is, a shot's frames (`-o DIR`) only with RAEA_READBACK=rgba (the default
//! readback is NV12, whose 4:2:0 chroma scrambles packed values). A render is a function of its frame, so a shot's passes and
//! its picture line up pixel for pixel.

use bevy::{
    camera::Camera3d,
    core_pipeline::{
        Core3dSystems,
        prepass::{DepthPrepass, NormalPrepass, ViewPrepassTextures},
        schedule::Core3d,
        tonemapping::Tonemapping,
        FullscreenShader,
    },
    prelude::*,
    render::{
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        render_resource::{
            binding_types::{texture_2d, texture_depth_2d, uniform_buffer},
            *,
        },
        renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery},
        texture::{CachedTexture, TextureCache},
        view::{ExtractedView, ViewTarget},
        Render, RenderApp, RenderStartup, RenderSystems,
    },
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Pass {
    Depth,
    Normal,
    Albedo,
    /// how much of the sun's disc each point sees, traced against the scene (rt.rs): 255 in full sun
    Sun,
    /// how much of each point's hemisphere is open (cosine-weighted, rays out to RAEA_PASS_REACH, 50 m), traced: 255 open
    Sky,
}

/// The pass RAEA_PASS asks for, if any.
pub(super) fn wanted() -> Option<Pass> {
    match std::env::var("RAEA_PASS").as_deref() {
        Ok("depth") => Some(Pass::Depth),
        Ok("normal") => Some(Pass::Normal),
        Ok("albedo") => Some(Pass::Albedo),
        Ok("sun") => Some(Pass::Sun),
        Ok("sky") => Some(Pass::Sky),
        _ => None,
    }
}

/// A camera drawing a pass.
#[derive(Component, Clone, Copy, ExtractComponent)]
struct PassView(u32);

pub(super) fn install(app: &mut App) {
    let Some(pass) = wanted() else { return };
    bevy::asset::embedded_asset!(app, "passes.wgsl");
    bevy::asset::embedded_asset!(app, "passes_rt.wgsl");
    if matches!(pass, Pass::Sun | Pass::Sky) {
        super::rt::install(app);
    }
    app.add_plugins(ExtractComponentPlugin::<PassView>::default()).add_systems(PostUpdate, move |commands: Commands, cams: Query<Entity, (With<Camera3d>, Without<PassView>)>, mats: ResMut<Assets<StandardMaterial>>| mark(pass, commands, cams, mats));
    let Some(render) = app.get_sub_app_mut(RenderApp) else { return };
    render
        .add_systems(RenderStartup, init)
        .add_systems(Render, prepare.in_set(RenderSystems::PrepareResources))
        .add_systems(Core3d, (capture.after(bevy::core_pipeline::core_3d::main_transparent_pass_3d).before(Core3dSystems::PostProcess), draw.after(Core3dSystems::PostProcess).before(bevy::ui_render::ui_pass).before(bevy::core_pipeline::upscaling::upscaling)));
}

/// Every 3D camera draws the pass (before the UI, which draws a render's frame tag in the row under the frame): its depth and normals kept (a prepass, single-sampled), and for albedo every
/// material unlit and nothing tonemapped or fogged.
fn mark(pass: Pass, mut commands: Commands, cams: Query<Entity, (With<Camera3d>, Without<PassView>)>, mut mats: ResMut<Assets<StandardMaterial>>) {
    for e in &cams {
        let mut c = commands.entity(e);
        c.insert((PassView(pass as u32), DepthPrepass, NormalPrepass, Msaa::Off));
        if pass == Pass::Albedo {
            c.insert(Tonemapping::None).remove::<DistanceFog>().remove::<bevy::post_process::bloom::Bloom>().remove::<bevy::render::view::ColorGrading>();
        }
    }
    if pass == Pass::Albedo {
        // (only those not yet unlit, so the assets aren't marked changed every frame)
        let ids: Vec<_> = mats.iter().filter(|(_, m)| !m.unlit).map(|(id, _)| id).collect();
        for id in ids {
            if let Some(mut m) = mats.get_mut(id) {
                m.unlit = true;
            }
        }
    }
}

#[derive(Resource)]
struct Pipelines {
    layout: BindGroupLayoutDescriptor,
    /// the traced passes': the same and the scene's TLAS
    rt_layout: BindGroupLayoutDescriptor,
    shader: Handle<Shader>,
    rt_shader: Handle<Shader>,
    fullscreen: bevy::render::render_resource::VertexState,
    /// by (entry, format)
    ids: std::sync::Mutex<bevy::platform::collections::HashMap<(u32, TextureFormat), CachedRenderPipelineId>>,
}

#[derive(ShaderType, Clone, Copy)]
struct Params {
    view_from_clip: Mat4,
    world_from_clip: Mat4,
    /// the direction to the sun, and the cosine of its disc's angular radius (0.267 degrees)
    sun: Vec4,
    /// the frame (a traced pass's rays turn with it), rays a pixel
    frame: UVec4,
}

fn init(mut commands: Commands, assets: Res<AssetServer>, fullscreen: Res<FullscreenShader>) {
    let layout = BindGroupLayoutDescriptor::new(
        "raea_passes_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (uniform_buffer::<Params>(false), texture_depth_2d(), texture_2d(TextureSampleType::Float { filterable: false }), texture_2d(TextureSampleType::Float { filterable: false })),
        ),
    );
    let rt_layout = BindGroupLayoutDescriptor::new(
        "raea_passes_rt_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (uniform_buffer::<Params>(false), texture_depth_2d(), texture_2d(TextureSampleType::Float { filterable: false }), texture_2d(TextureSampleType::Float { filterable: false }), bevy::render::render_resource::binding_types::acceleration_structure()),
        ),
    );
    commands.insert_resource(Pipelines {
        layout,
        rt_layout,
        shader: assets.load("embedded://rdb/engine/passes.wgsl"),
        rt_shader: assets.load("embedded://rdb/engine/passes_rt.wgsl"),
        fullscreen: fullscreen.to_vertex_state(),
        ids: Default::default(),
    });
}

/// What albedo copies before the post-processing (the unlit scene), the size and format of the frame.
#[derive(Component)]
struct Kept(CachedTexture);

fn prepare(mut commands: Commands, mut cache: ResMut<TextureCache>, device: Res<RenderDevice>, views: Query<(Entity, &ViewTarget, &ExtractedView, &PassView)>) {
    for (e, target, view, pass) in &views {
        if pass.0 != Pass::Albedo as u32 {
            continue;
        }
        let size = target.main_texture().size();
        let t = cache.get(
            &device,
            TextureDescriptor {
                label: Some("raea_passes_kept"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: view.target_format,
                usage: TextureUsages::COPY_DST | TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
        );
        commands.entity(e).insert(Kept(t));
    }
}

fn capture(view: ViewQuery<(&ViewTarget, &PassView, Option<&Kept>)>, mut ctx: RenderContext) {
    let (target, _, kept) = view.into_inner();
    let Some(kept) = kept else { return };
    let src = target.main_texture();
    if src.format() != kept.0.texture.format() {
        return;
    }
    ctx.command_encoder().copy_texture_to_texture(src.as_image_copy(), kept.0.texture.as_image_copy(), src.size());
}

fn draw(view: ViewQuery<(&ViewTarget, &ExtractedView, &PassView, Option<&ViewPrepassTextures>, Option<&Kept>)>, p: Option<Res<Pipelines>>, cache: Res<PipelineCache>, queue: Res<RenderQueue>, scene: Option<Res<super::rt::Scene>>, sun: Option<Res<super::rt::Sun>>, frame: Res<bevy::diagnostic::FrameCount>, mut ctx: RenderContext) {
    let Some(p) = p else { return };
    let (target, extracted, pass, prepass, kept) = view.into_inner();
    let format = target.main_texture_format();
    let traced = pass.0 >= Pass::Sun as u32;
    if traced && scene.is_none() {
        return;
    }
    let id = *p.ids.lock().unwrap().entry((pass.0, format)).or_insert_with(|| {
        cache.queue_render_pipeline(RenderPipelineDescriptor {
            label: Some("raea_passes".into()),
            layout: vec![if traced { p.rt_layout.clone() } else { p.layout.clone() }],
            vertex: p.fullscreen.clone(),
            fragment: Some(FragmentState {
                shader: if traced { p.rt_shader.clone() } else { p.shader.clone() },
                entry_point: Some(["depth", "normal", "albedo", "sun", "sky"][pass.0 as usize].into()),
                targets: vec![Some(ColorTargetState { format, blend: None, write_mask: ColorWrites::ALL })],
                ..default()
            }),
            ..default()
        })
    });
    let (Some(pipeline), Some(prepass)) = (cache.get_render_pipeline(id), prepass) else { return };
    let (Some(depth), Some(normal)) = (prepass.depth_view(), prepass.normal_view()) else { return };
    let device = ctx.render_device().clone();
    let world_from_clip = extracted.world_from_view.to_matrix() * extracted.clip_from_view.inverse();
    let rays = std::env::var("RAEA_PASS_RAYS").ok().and_then(|v| v.parse().ok()).unwrap_or(64u32);
    let sun_dir = sun.map(|s| s.0).unwrap_or(Vec3::Y).normalize_or(Vec3::Y);
    let mut params = bevy::render::render_resource::UniformBuffer::from(Params {
        view_from_clip: extracted.clip_from_view.inverse(),
        world_from_clip,
        sun: sun_dir.extend(0.267f32.to_radians().cos()),
        // the sky pass's reach in millimetres (RAEA_PASS_REACH metres; 50 by default, the room and out of it; half a
        // metre or so makes it contact occlusion)
        frame: UVec4::new(frame.0, rays, (std::env::var("RAEA_PASS_REACH").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(50.0) * 1000.0) as u32, 0),
    });
    params.write_buffer(&device, &queue);
    let copy = kept.map(|k| &k.0.default_view).unwrap_or(normal);
    let group = match &scene {
        Some(s) if traced => device.create_bind_group("raea_passes_rt", &cache.get_bind_group_layout(&p.rt_layout), &BindGroupEntries::sequential((params.binding().unwrap(), depth, normal, copy, s.0.as_binding()))),
        _ => device.create_bind_group("raea_passes", &cache.get_bind_group_layout(&p.layout), &BindGroupEntries::sequential((params.binding().unwrap(), depth, normal, copy))),
    };
    let mut rp = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor { label: Some("raea_passes"), color_attachments: &[Some(target.get_color_attachment())], ..default() });
    rp.set_pipeline(pipeline);
    rp.set_bind_group(0, &group, &[]);
    rp.draw(0..3, 0..1);
}
