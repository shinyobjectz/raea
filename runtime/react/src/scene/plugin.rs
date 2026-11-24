//! ScenePlugin: a composition's <Canvas3D> drawn into an App's render target.

use super::*;

/// raea's 3D in a plain Bevy App (docs/sdk.feature S6a): each Update, React's tree in this World is made into a live
/// scene's rows and applied to the same World, so
/// `App::new().add_plugins((DefaultPlugins, RaeaPlugins, Scene3dPlugin, ReactPlugin::new(bundle), ScenePlugin { .. }))`
/// draws a composition's <Canvas3D> into `target`. It is live play's path (live_scene) with the App's World in place of
/// a separate Engine's: the first frame with any 3D opens the scene (its camera and parts), later frames set only
/// what changed and drop what's gone.
pub struct ScenePlugin {
    pub target: bevy::camera::RenderTarget,
    pub size: (u32, u32),
    /// where the composition's media resolve (its folder)
    pub dir: PathBuf,
    /// the composition's frame: held at one (a still), or None to step one frame each Update from 0
    pub frame: Option<i64>,
}

/// ScenePlugin's systems, by name, for a plugin to order against (the page plugin's PageSystems after Rows): Frame tells
/// React the frame (in First), Rows makes React's tree into the scene's rows (in Update, before Scene3dSystems).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum SceneSystems {
    Frame,
    Rows,
}

/// The composition's frame this update, as ScenePlugin told React: one clock for the 3D and whatever else draws the
/// composition (the page's CSS animations).
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneFrame(pub i64);

#[derive(Resource)]
pub(crate) struct SceneRows {
    pub(crate) ls: LiveScene,
    pub(crate) open: bool,
    pub(crate) dir: PathBuf,
    pub(crate) frame: Option<i64>,
    pub(crate) next: i64,
}

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(raea_render::engine::SceneTarget { target: self.target.clone(), size: self.size })
            .insert_resource(SceneRows { ls: LiveScene::default(), open: false, dir: self.dir.clone(), frame: self.frame, next: 0 })
            .init_resource::<SceneFrame>()
            .add_systems(First, scene_frame.in_set(SceneSystems::Frame))
            .add_systems(Update, scene_rows.in_set(SceneSystems::Rows).before(raea_render::engine::raea_plugins::Scene3dSystems));
    }
}

/// The composition's clock: React told the frame to render, as live play tells it (raea-frame), each update.
pub(crate) fn scene_frame(mut s: ResMut<SceneRows>, mut frame: ResMut<SceneFrame>, outbox: Res<ReactOutbox>) {
    let f = s.frame.unwrap_or(s.next);
    s.next += 1;
    frame.0 = f;
    outbox.send(serde_json::json!({"t": "event", "name": "raea-frame", "value": {"frame": f, "input": {"keys": []}}}));
}

pub(crate) fn scene_rows(world: &mut World) {
    let Some(mut s) = world.remove_resource::<SceneRows>() else { return };
    if let Err(e) = scene_rows_into(world, &mut s) {
        error!("raea's 3D: {e}");
    }
    world.insert_resource(s);
}

pub(crate) fn scene_rows_into(world: &mut World, s: &mut SceneRows) -> R<()> {
    let scene = scene_of(world);
    if scene.is_empty() && !s.open {
        return Ok(());
    }
    edit_dir(&s.dir);
    let clear = scene_clear(world);
    let f = Shot { scene: &scene, clear: clear.as_deref() };
    let (rows, gone) = if s.open {
        s.ls.frame(f)
    } else {
        let (w, h) = world.resource::<raea_render::engine::SceneTarget>().size;
        let p = s.ls.open(f, w, h, 30.0);
        s.open = true;
        (raea_render::inserted_of(&p), Vec::new())
    };
    if !gone.is_empty() {
        raea_render::engine::remove_rows_from(world, &gone)?;
    }
    if !rows.is_empty() {
        // a glTF model spawned here loads over the frames after, as any Bevy App's does
        raea_render::engine::apply_rows(world, &s.dir, &rows)?;
    }
    Ok(())
}

/// The edit's folder: where its 3D parts' files are found (on this thread) and its assets (native asset.load, any thread).
pub fn edit_dir(dir: &Path) {
    raea_core_scene::set_media_dir(dir);
    *crate::native::ASSET_DIR.lock().unwrap() = Some(dir.to_path_buf());
}
