//! Bevy plugins in raea's engine (sdk/raea/docs/bevy-plugins.md). A project declares its plugins in package.json
//! (raea.plugins, or raea.bevy.plugins, its first name); tools/bevy-plugins.sh resolves the declarations into this crate's dependencies and features and
//! into src/declared.rs, which registers each plugin with raea-render's hooks (runtime/plugins/src/lib.rs). Every Engine
//! made afterwards adds the plugins, steps them by the frame index, and applies TSX's component rows through the
//! TypeRegistry; `bridge` gives TypeScript a plain headless App with any of them, and bevy_remote's method table to call
//! into it. What a plugin needed by hand (its adapter or its calls) is a module here, named by its declaration.

mod asset;
pub mod bridge;
mod input;
mod needs;

// the adapters and calls a declaration names, by feature (here, not in declared.rs, so cargo shear sees them)
// >>> declared modules: written by tools/bevy-plugins.sh sync; don't edit
#[cfg(feature = "avian")]
pub mod avian;
#[cfg(feature = "bevy")]
pub mod bevy;
#[cfg(feature = "common_assets")]
pub mod common_assets;
#[cfg(feature = "enhanced_input")]
pub mod enhanced_input;
#[cfg(feature = "hanabi")]
pub mod hanabi;
#[cfg(feature = "leafwing")]
pub mod leafwing;
#[cfg(feature = "lightyear")]
pub mod lightyear;
#[cfg(feature = "tnua")]
pub mod tnua;
// <<< declared modules

include!("declared.rs");

#[doc(hidden)]
pub struct OnePlugin;
#[doc(hidden)]
pub struct AGroup;

/// A declared plugin added unless one before it already added it (bevy_console adds EguiPlugin itself, and Bevy
/// panics on a unique plugin added twice). Marked by kind as Bevy's own `Plugins<M>` is; a group is added as it is.
pub trait AddOnce<M> {
    fn add_once(self, app: &mut ::bevy::app::App);
}

impl<P: ::bevy::app::Plugin> AddOnce<OnePlugin> for P {
    fn add_once(self, app: &mut ::bevy::app::App) {
        if !app.is_plugin_added::<P>() {
            app.add_plugins(self);
        }
    }
}

impl<G: ::bevy::app::PluginGroup> AddOnce<AGroup> for G {
    fn add_once(self, app: &mut ::bevy::app::App) {
        app.add_plugins(self);
    }
}

/// A tuple (quinnet's server and client together): each one added once.
macro_rules! add_once_tuple {
    ($($p:ident $m:ident),+) => {
        impl<$($m, $p: AddOnce<$m>),+> AddOnce<($($m,)+)> for ($($p,)+) {
            #[allow(non_snake_case)]
            fn add_once(self, app: &mut ::bevy::app::App) {
                let ($($p,)+) = self;
                $($p.add_once(app);)+
            }
        }
    };
}
add_once_tuple!(A MA, B MB);
add_once_tuple!(A MA, B MB, C MC);
add_once_tuple!(A MA, B MB, C MC, D MD);

/// One declared plugin, as tools/bevy-plugins.sh resolved it.
#[derive(Clone, Debug)]
pub struct Declared {
    /// its name in the declaration, which is also its cargo feature and its name in raea-render's registry
    pub name: &'static str,
    pub krate: &'static str,
    /// the source resolved for this Bevy, as TOML (`version = "=0.7.0"`, `git = ..., rev = ...`)
    pub source: &'static str,
    /// the type-path prefixes its generated TypeScript keeps
    pub prefixes: &'static [&'static str],
    /// the other declared plugins it needs added with it (a backend's engine: tnua's avian)
    pub needs: &'static [&'static str],
    /// what a plain headless App needs for it: "assets" (AssetPlugin), "mesh" (Assets<Mesh>), "states", "input"
    pub headless: &'static [&'static str],
    /// only an App that names it gets it
    pub on_demand: bool,
}

/// Every plugin this build has: registered with raea-render, plus the bridge's own natives (`bevy.app.*`). Call before the
/// first Engine.
pub fn install_all() {
    install();
    // the catalog: an Engine adds those its project chooses (raea_runtime_plugins::choose, docs/nobuild.feature NB4a)
    raea_runtime_plugins::catalog(built().map(|d| raea_runtime_plugins::Entry { name: d.name, needs: d.needs, prefixes: d.prefixes }).collect());
    bridge::register_natives();
}

/// The plugins this build has, by name.
pub fn compiled() -> Vec<&'static str> {
    declared().iter().filter(|d| raea_runtime_plugins::names().iter().any(|n| n == &d.name || n.starts_with(&format!("{}_", d.name)))).map(|d| d.name).collect()
}

/// The declared plugins this build compiled in.
fn built() -> impl Iterator<Item = Declared> {
    let compiled = compiled();
    declared().into_iter().filter(move |d| compiled.contains(&d.name))
}

/// The type-path prefixes each compiled plugin's reflected types live under (what the generator keeps).
pub fn prefixes() -> Vec<&'static str> {
    built().flat_map(|d| d.prefixes.iter().copied()).collect()
}
