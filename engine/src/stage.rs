//! Bevy's schedule as a composition's frame loop (docs/bevy.feature B2). The composition (Blitz's document with its
//! V8 runtime) is a non-Send resource of a Bevy World, and a frame is one run of the schedule: the seek system moves
//! it to the frame's time (animation tracks, script callbacks, canvases), then the paint system rasterises it into
//! the frame buffer. The calls and their order are the ones Composition::render made, so frames are unchanged; the
//! later stages (style, layout, text and paint as systems over entities, B3) are added to this same schedule.

use bevy_ecs::prelude::*;

use crate::scene::Composition;

/// The time of the frame being made, in seconds.
#[derive(Resource, Default)]
pub struct FrameClock {
    pub t: f64,
}

/// The frame the schedule paints into (RGBA, the composition's output size).
#[derive(Resource, Default)]
pub struct FrameBuffer {
    pub rgba: Vec<u8>,
}

/// A composition stepped by Bevy's schedule.
pub struct Stage {
    pub world: World,
    schedule: Schedule,
}

fn seek(mut comp: NonSendMut<Composition>, clock: Res<FrameClock>) {
    comp.seek_frame(clock.t);
}

fn paint(mut comp: NonSendMut<Composition>, clock: Res<FrameClock>, mut fb: ResMut<FrameBuffer>) {
    comp.paint(&mut fb.rgba, clock.t, std::time::Instant::now());
}

impl Stage {
    pub fn new(comp: Composition) -> Stage {
        let mut world = World::new();
        world.insert_non_send(comp);
        world.insert_resource(FrameClock::default());
        world.insert_resource(FrameBuffer::default());
        let mut schedule = Schedule::default();
        // the document isn't Send (Rc, V8): its systems run on this thread, in order
        schedule.set_executor(bevy_ecs::schedule::SingleThreadedExecutor::new());
        schedule.add_systems((seek, paint).chain());
        Stage { world, schedule }
    }

    /// Frame `t` into `out` (its buffer is reused across frames).
    pub fn frame(&mut self, t: f64, out: &mut Vec<u8>) {
        self.world.resource_mut::<FrameClock>().t = t;
        std::mem::swap(&mut self.world.resource_mut::<FrameBuffer>().rgba, out);
        self.schedule.run(&mut self.world);
        std::mem::swap(&mut self.world.resource_mut::<FrameBuffer>().rgba, out);
    }

    /// Frame `t` into `out` and the text on it (Composition::text_report: boxes, edges, clips, safe zone, overlaps);
    /// `safe` is the safe zone (top, right, bottom, left in CSS px).
    pub fn text_boxes(&mut self, t: f64, out: &mut Vec<u8>, safe: Option<[f64; 4]>) -> serde_json::Value {
        let ((), runs) = blitz_paint::collect_text_runs(|| self.frame(t, out));
        self.comp().text_report(t, runs, safe)
    }

    /// The composition back out of the World (a host that hands it on).
    pub fn into_comp(mut self) -> Composition {
        self.world.remove_non_send::<Composition>().expect("the stage holds its composition")
    }

    /// The composition, for what isn't a frame yet (text boxes, media layers, page keys, warnings).
    pub fn comp(&mut self) -> Mut<'_, Composition> {
        self.world.non_send_mut::<Composition>()
    }
}
