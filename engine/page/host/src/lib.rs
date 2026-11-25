//! raea-page-host: one raea page in a headless Bevy app, driven and read the way the benches drive an app:
//! input injected as the messages winit writes (src/inject.rs), the page's document read as a
//! tree of its data-testid elements (src/page.rs, src/tree.rs), the window read back as a PNG, all over the Bevy Remote
//! Protocol (src/brp.rs: page.*) and an MCP bridge to it (src/mcp.rs). The bin (src/main.rs) mounts a bundle; an app
//! adds [`HostPlugin`] and mounts its own (page::mount), its systems around the host's.

pub mod brp;
/// the desktop window's side, in raea-page-window (under these names for main.rs and tests)
#[cfg(feature = "window")]
pub use raea_page_window::idle;
pub use raea_page_window::{chrome, native, scheme};
/// the live stage, in raea-page-stage
pub use raea_page_stage as stage;
pub mod inject;
pub mod mcp;
pub mod page;
pub mod shell;
pub mod tree;

use std::time::Duration;

use bevy::camera::{ImageRenderTarget, RenderTarget};
use bevy::picking::input::PointerInputSettings;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;

/// The camera the UI draws with (its target is the window's image).
#[derive(Resource)]
pub struct UiCam(pub Entity);

/// The window's size at launch, in points.
#[derive(Resource, Clone, Copy)]
pub struct LaunchSize(pub UVec2);

/// The page takes its pointers through raea-page's picking route and its keys through KeysPlugin (docs/blitz-bevy.feature
/// B3 H6), and its JS steps in PageSystems::Step (HostStepPlugin, H8); off, page::input dispatches and page::step steps.
pub fn picking() -> bool {
    PICKING.load(std::sync::atomic::Ordering::Relaxed)
}

static PICKING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The host's resources and its frame: the counter and the input in First (and, on screen, the window's pointer into
/// picking), the tree, state and pictures kept in Last. The camera is the app's to add (`camera`),
/// before it mounts its page.
pub struct HostPlugin {
    pub size: UVec2,
    /// the longest gap between two presses that makes them a double click (AppKit's doubleClickInterval), for the
    /// window on screen; injected input says its own count
    pub double_click: Duration,
    pub picking: bool,
}

impl Plugin for HostPlugin {
    fn build(&self, app: &mut App) {
        PICKING.store(self.picking, std::sync::atomic::Ordering::Relaxed);
        // the pointer is fed by inject.rs: headless, picking's own mouse reader would place it on the window, not its
        // image; on screen, inject::window_pointer reads the window's events as that reader does, with a trackpad's
        // deltas in points
        // a text field's copy, cut and paste reach the clipboard (src/shell.rs), unless the app gave its pages a shell
        if !app.world().contains_resource::<raea_sys_page::PageShell>() {
            app.insert_resource(raea_sys_page::PageShell(std::sync::Arc::new(shell::Shell)));
        }
        app.insert_resource(PointerInputSettings { is_mouse_enabled: false, is_touch_enabled: false })
            .insert_resource(inject::Clicks::new(self.double_click))
            .insert_resource(LaunchSize(self.size))
            .init_resource::<inject::Frame>()
            .init_resource::<inject::Pending>()
            .init_resource::<brp::Started>()
            .init_resource::<tree::PngWaiting>()
            .init_resource::<tree::WindowFacts>()
            .init_resource::<tree::Tree>()
            .init_resource::<tree::Extra>()
            .init_resource::<page::Events>()
            // BRP's listener is bound in Startup: MCP calls made before wait for this (src/mcp.rs)
            .add_systems(PostStartup, mcp::ready)
            .add_systems(First, (inject::tick, inject::pump, inject::window_pointer.run_if(|t: Option<Res<inject::Target>>| t.is_some_and(|t| t.windowed))).chain())
            .add_systems(Last, tree::answer);
    }
}

/// The page's plugins: raea-page's, its host (raea-runtime-js's calls on a Blitz page), and with picking the bridge's route,
/// keys and step.
pub fn page_plugins(app: &mut App, picking: bool) {
    app.add_plugins((raea_page::PagePlugin, raea_runtime_dom::host::HostPlugin)).init_resource::<raea_runtime_js::UiFrame>();
    if picking {
        app.add_plugins((raea_runtime_input::pick::PickPlugin, raea_runtime_input::keys::KeysPlugin, raea_runtime_dom::host::HostStepPlugin));
    }
}

/// Headless: the window is an image the UI camera draws into, at one pixel a point.
pub fn camera(mut commands: Commands, mut images: ResMut<Assets<Image>>, size: Res<LaunchSize>) {
    let image = images.add(Image::new_target_texture(size.0.x, size.0.y, TextureFormat::Rgba8UnormSrgb, None));
    let target = RenderTarget::Image(ImageRenderTarget { handle: image.clone(), scale_factor: 1.0 });
    let norm = target.normalize(None).expect("an image target normalizes");
    let cam = commands.spawn((Camera2d, Camera { clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() }, target)).id();
    commands.insert_resource(inject::Target { image, norm, scale: 1.0, windowed: false });
    commands.insert_resource(UiCam(cam));
}

/// On screen (raea's window, the `window` feature): the UI camera draws to the primary window at its own scale; the
/// window's pointer reaches picking through inject::window_pointer, its keys through bevy_winit as any app's.
pub fn camera_window(mut commands: Commands, mut images: ResMut<Assets<Image>>, primary: Query<Entity, With<bevy::window::PrimaryWindow>>) {
    let win = primary.single().expect("raea's window has a primary window");
    let target = RenderTarget::Window(bevy::window::WindowRef::Primary);
    let norm = target.normalize(Some(win)).expect("the primary window normalizes");
    let cam = commands.spawn((Camera2d, Camera { clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() }, target)).id();
    // the image is unused on screen (inject::Target), a pixel to fill the field
    let image = images.add(Image::new_target_texture(1, 1, TextureFormat::Rgba8UnormSrgb, None));
    commands.insert_resource(inject::Target { image, norm, scale: 1.0, windowed: true });
    commands.insert_resource(UiCam(cam));
}
