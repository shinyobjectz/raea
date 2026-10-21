//! bevy_hanabi (GPU particles). What had to be written by hand: an effect is an EffectAsset (a graph of modifiers and
//! expressions held behind a Handle), which a row can't carry, so `Particles` is a reflected component of ours that
//! builds a fountain effect from plain numbers when it's added; hanabi's CPU random source is seeded (it seeds itself
//! from the OS); its per-update re-seed is held to the frame steps; and WinitPlugin, which hanabi's `x11` feature
//! switches on, is turned off.

use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;
use bevy_hanabi::prelude::*;

/// A fountain of particles from the entity it's on (a Mesh), built into a hanabi effect when added.
#[derive(Component, Reflect, Clone, Debug)]
#[reflect(Component, Default)]
pub struct Particles {
    /// particles a second
    pub rate: f32,
    /// seconds each lives
    pub lifetime: f32,
    /// upward speed, units a second
    pub speed: f32,
    /// how far from straight up they spread (0 straight, 1 a hemisphere)
    pub spread: f32,
    /// size in scene units
    pub size: f32,
    /// linear RGBA (above 1 glows with Bloom)
    pub color: Vec4,
    /// downward acceleration
    pub gravity: f32,
    /// the most alive at once
    pub capacity: u32,
    /// the GPU random seed
    pub seed: u32,
    /// alpha-blended (soft, drawn in an order the GPU chooses) or opaque
    pub opaque: bool,
}

impl Default for Particles {
    fn default() -> Self {
        Particles { rate: 200.0, lifetime: 1.5, speed: 3.0, spread: 0.35, size: 0.05, color: Vec4::new(1.0, 0.6, 0.2, 1.0), gravity: 4.0, capacity: 4096, seed: 1, opaque: true }
    }
}

pub(crate) fn install(app: &mut App) {
    app.add_plugins(HanabiPlugin);
    // hanabi re-seeds every effect's GPU random state on every update (compile_effects), so particles depended on how
    // many updates a frame took; compiled and re-seeded only on updates that step a frame, they depend on frames
    app.configure_sets(PostUpdate, EffectSystems::CompileEffects.run_if(rdb::engine::plugins::advancing));
    // hanabi seeds its CPU random source from the OS at build: a fixed one instead
    app.insert_resource(bevy_hanabi::Random(rand_pcg::Pcg32::new(0xcafe_f00d_d15e_a5e5, 0xa02b_dbf7_bb3c_0a7)));
    app.register_type::<Particles>();
    app.add_systems(Update, build_particles);
}

/// hanabi's `x11` feature switches bevy_winit on; rdb drives its own updates and has no window.
pub(crate) fn group(b: PluginGroupBuilder) -> PluginGroupBuilder {
    b.disable::<bevy::winit::WinitPlugin>()
}

fn build_particles(mut commands: Commands, mut effects: ResMut<Assets<EffectAsset>>, added: Query<(Entity, &Particles), Added<Particles>>) {
    for (e, p) in &added {
        let w = ExprWriter::new();
        let init_age = SetAttributeModifier::new(Attribute::AGE, w.lit(0.0).expr());
        let init_life = SetAttributeModifier::new(Attribute::LIFETIME, w.lit(p.lifetime).expr());
        let init_pos = SetPositionSphereModifier { center: w.lit(Vec3::ZERO).expr(), radius: w.lit(p.size * 2.0).expr(), dimension: ShapeDimension::Volume };
        // up, tipped by a random direction scaled by the spread
        let dir = (w.rand(VectorType::VEC3F) * w.lit(2.0) - w.lit(1.0)) * w.lit(p.spread) + w.lit(Vec3::Y);
        let init_vel = SetAttributeModifier::new(Attribute::VELOCITY, (dir.normalized() * w.lit(p.speed)).expr());
        let accel = AccelModifier::new(w.lit(Vec3::new(0.0, -p.gravity, 0.0)).expr());
        let mut colour = bevy_hanabi::Gradient::new();
        colour.add_key(0.0, p.color);
        colour.add_key(1.0, p.color * Vec4::new(0.6, 0.6, 0.6, 1.0));
        let asset = EffectAsset::new(p.capacity, SpawnerSettings::rate(p.rate.into()), w.finish())
            .with_name("raea particles")
            .with_alpha_mode(if p.opaque { bevy_hanabi::AlphaMode::Opaque } else { bevy_hanabi::AlphaMode::Blend })
            .init(init_pos)
            .init(init_vel)
            .init(init_age)
            .init(init_life)
            .update(accel)
            .render(ColorOverLifetimeModifier::new(colour))
            .render(SizeOverLifetimeModifier { gradient: bevy_hanabi::Gradient::constant(Vec3::splat(p.size)), screen_space_size: false });
        let handle = effects.add(asset);
        let mut effect = ParticleEffect::new(handle);
        effect.prng_seed = Some(p.seed);
        commands.entity(e).insert(effect);
    }
}
