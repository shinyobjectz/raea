//! BevyPage: a composition's page laid out and painted by Bevy, frame by frame (what a render and play draw).

use super::*;

pub struct BevyPage {
    /// the page under it: Blitz's (stylo, Blitz's layout, Vello on the GPU; docs/blitz-bevy.feature)
    pub(crate) page: raea_page::motion::BlitzPage,
    pub w: u32,
    pub h: u32,
    pub(crate) dir: PathBuf,
    /// the layered Canvas3D's element, when the page has one
    pub(crate) slot: Option<u64>,
    /// the element holding the page's camera (`view_box`)
    pub(crate) view: Option<u64>,
    /// the camera's view was in that element's box at the last frame (its holders keep their backgrounds)
    pub in_box: bool,
    /// the page has an element under all the others for a 3D frame (live play, `set_under`)
    pub(crate) under: bool,
    /// the app screens on the page (name, size, module) and live play's input routed to them (runtime/page/src/screen.rs)
    pub(crate) app_screens: Vec<(String, u32, u32, String)>,
    pub(crate) apps: crate::screen::Apps,
    /// the last walk's elements, kept for the next
    pub(crate) walk: Walk,
    /// the composition's surface, as the last sync found it
    pub(crate) surf: Option<Entity>,
    /// the UiMaterial elements the last sync met: page id and spec (JSON)
    pub(crate) ui_mats: Vec<(u64, String)>,
}

impl BevyPage {
    /// A page `w` x `h` for the composition in `dir` (its media resolve from there), in raea's bundled faces.
    pub fn new(w: u32, h: u32, dir: &Path) -> R<BevyPage> {
        // on the shared device (raea-render's engine and the legacy app's renderer are there too), so live play's picture is a texture
        // they draw as it is
        let fonts = raea_io_home::home().join("core/fonts");
        let page = raea_page::motion::BlitzPage::new_on(w, h, Some(&fonts), Some(raea_runtime_view::render_creation()))?;
        Ok(BevyPage { page, w, h, dir: dir.to_path_buf(), slot: None, view: None, in_box: false, under: false, app_screens: Vec::new(), apps: Default::default(), walk: Walk::default(), surf: None, ui_mats: Vec::new() })
    }

    /// The page made to match the React tree in `react` (the composition's App's World). `holders`: the props elements
    /// holding 3D take instead of their own (see-through), by React node; `brand`: the space's brand kit. Errors when the
    /// composition's surface isn't there or a video can't be decoded.
    pub fn sync(&mut self, react: &mut World, holders: &HashMap<u64, Map<String, Value>>, brand: Option<&str>) -> R<()> {
        let surf = match self.surf.filter(|e| react.get::<ReactKind>(*e).is_some_and(|k| k.0 == "surface")) {
            Some(e) => e,
            None => {
                let e = raea_runtime_react::scene::surface(react).ok_or("the composition's surface isn't there")?;
                self.surf = Some(e);
                e
            }
        };
        let mut videos = Vec::new();
        let mut screens = Vec::new();
        SCENE_SLOT.with(|c| c.set(None));
        VIEW_HOLDER.with(|c| c.set(None));
        IN_3D.with(|c| c.set(0));
        IN_SVG.with(|c| c.set(false));
        APP_SCREENS.with(|c| c.borrow_mut().clear());
        TEXTURE_SLOTS.with(|c| c.borrow_mut().clear());
        UI_MATERIALS.with(|c| c.borrow_mut().clear());
        REFUSED.with(|c| c.borrow_mut().clear());
        self.walk.begin(self.w, self.h, brand, self.under);
        walk(react, surf, BODY, At::Str(""), &self.dir, holders, &mut self.walk, &mut videos, &mut screens);
        self.walk.end();
        // an <Img> of a file that isn't there fails the render, naming it (footage.md), never a frame without it; so does
        // a framed take with no face track
        let refused = REFUSED.with(|c| std::mem::take(&mut *c.borrow_mut()));
        if !refused.is_empty() {
            return Err(refused.join("; ").into());
        }
        self.page.sync(self.walk.wants(), &self.walk.order);
        // each <Screen>'s nested composition: a root of its own at the screen's size, drawn into its own picture
        let mut keep = Vec::new();
        for s in screens {
            videos.extend(s.videos);
            let (mut want, mut order, first) = frame_wants(s.w, s.h, brand);
            want.extend(s.want);
            for (k, v) in s.order {
                order.insert(k, v);
            }
            order.insert(BODY, first.into_iter().chain(std::iter::once(s.top)).collect());
            self.page.sync_sub(&s.name, s.w, s.h, &want, &order)?;
            keep.push(s.name);
        }
        self.page.retain_subs(&keep);
        self.slot = SCENE_SLOT.with(|c| c.get());
        let view = VIEW_HOLDER.with(|c| c.get());
        // the camera left its holder (its 3D unmounted, the holder kept): the view it showed goes with it
        if let Some(old) = self.view.filter(|&old| view != Some(old)) {
            self.page.unplace(old);
        }
        self.view = view;
        self.app_screens = APP_SCREENS.with(|c| std::mem::take(&mut *c.borrow_mut()));
        self.ui_mats = UI_MATERIALS.with(|c| std::mem::take(&mut *c.borrow_mut()));
        for (tex, id) in TEXTURE_SLOTS.with(|c| std::mem::take(&mut *c.borrow_mut())) {
            if let Some((t, w, h)) = SHARED.with(|s| s.borrow().get(&tex).cloned()) {
                self.page.place_texture(id, &t.into(), w, h);
            }
        }
        crate::screen::retire(&mut self.apps, &self.app_screens);
        crate::screen::step_all(&mut self.apps, &self.app_screens, &self.dir);
        self.page.place_videos(videos)
    }

    /// The box of the element a camera in the page sits in, as last drawn (its transforms applied: a shot laid out at
    /// 1920 x 1080 and scaled to a 4K frame fills it), rounded to whole px, when it isn't the whole frame
    /// (docs/sdk.feature S3a): the camera's view is drawn there (`set_view`), not under the whole page.
    pub fn view_box(&mut self) -> Option<[u32; 4]> {
        let b = self.page.client_box(self.view?)?;
        let r = |v: f32| v.round().max(0.0) as u32;
        let b = [r(b[0]), r(b[1]), r(b[2]).min(self.w), r(b[3]).min(self.h)];
        let b = (b != [0, 0, self.w, self.h] && b[2] > 0 && b[3] > 0).then_some(b);
        *VIEW_BOX.lock().unwrap() = b;
        b
    }

    /// The camera's view, drawn in box `b` of `src` (live play's 3D target), as the picture of the element it sits in.
    pub fn set_view_texture(&mut self, src: &wgpu::Texture, b: [u32; 4]) {
        if let Some(id) = self.view {
            self.page.place_texture_at(id, &src.clone().into(), (b[0], b[1]), b[2], b[3], false);
        }
    }

    /// The camera's view (`rgba` straight, `w` x `h`) as the picture of the element it sits in.
    pub fn set_view(&mut self, w: u32, h: u32, rgba: Vec<u8>) {
        if let Some(id) = self.view {
            self.page.place_picture(id, w, h, rgba);
        }
    }

    /// Whether the page has a layered Canvas3D's slot (its 3D in the page's paint order).
    pub fn has_slot(&self) -> bool {
        self.slot.is_some()
    }

    /// The layered 3D's frame (`rgba` straight, `w` x `h`) as its slot's picture.
    pub fn set_scene(&mut self, w: u32, h: u32, rgba: Vec<u8>) {
        if let Some(id) = self.slot {
            self.page.place_picture(id, w, h, rgba);
        }
    }

    /// The page and each <Screen>'s picture drawn and read back: the page's premultiplied RGBA, and each screen's name,
    /// size and RGBA.
    pub fn frame_all(&mut self) -> R<(Vec<u8>, Vec<(String, u32, u32, Vec<u8>)>)> {
        let (px, mut pics) = self.page.frame_all()?;
        pics.extend(crate::screen::pictures(&mut self.apps)?);
        Ok((px, pics))
    }

    /// The page drawn at frame `f` of a composition at `fps`: its CSS animations and transitions where they are then,
    /// and its app screens stepped there.
    pub fn at_frame(&mut self, f: i64, fps: f64) {
        self.page.at_frame(f, fps);
        self.set_app_frame(f, fps);
    }

    /// The frame the app screens step to at the next sync (live play, whose page keeps its own CSS clock).
    pub fn set_app_frame(&mut self, f: i64, fps: f64) {
        self.apps.frame = Some((f, fps));
    }

    /// The page drawn and read back: premultiplied RGBA rows, `w` x `h`.
    pub fn frame(&mut self) -> R<Vec<u8>> {
        self.page.frame()
    }

    /// The first thing the page shows that isn't black, and where it's written (raea-page's visible_paint): a frame all
    /// black while it's there was lost (#96).
    pub fn visible_paint(&mut self) -> Option<String> {
        self.page.visible_paint()
    }

    /// Whether the page's GPU still runs work (raea-page's gpucheck).
    pub fn canary(&self) -> R<()> {
        self.page.canary()
    }

    /// A React element's box as it's drawn, transforms applied ([x, y, w, h] in page px), by its React id.
    pub fn client_box(&mut self, id: u64) -> Option<[f32; 4]> {
        self.page.client_box(id + REACT_IDS)
    }

    /// Every React element's boxes as the page lays them out now (`page.layout`, docs/layout-reads.feature LR1), keyed by
    /// React id: the page's own elements (the body, holders' parts) left out. Lay the page out first.
    pub fn layout_snapshot(&self, request: &str) -> String {
        // the elements whose computed style the read asks for (React ids; op ids here)
        let styles: Vec<u64> = serde_json::from_str::<serde_json::Value>(request)
            .ok()
            .and_then(|v| v["styles"].as_array().map(|a| a.iter().filter_map(serde_json::Value::as_u64).map(|r| r + REACT_IDS).collect()))
            .unwrap_or_default();
        self.page.layout_snapshot(|id| (id >= REACT_IDS && id & TEXT_BIT == 0).then(|| id - REACT_IDS), &styles)
    }

    /// The React element topmost at the point (page px): its React id (none over the page's own body).
    pub fn hit(&mut self, x: f32, y: f32) -> Option<u64> {
        self.page.hit(x, y).map(|id| id & !TEXT_BIT).filter(|id| *id >= REACT_IDS).map(|id| id - REACT_IDS)
    }

    /// The React elements under the point (page px), the topmost first and then each one up to the root: the path a
    /// pointer event there bubbles along (live play's page pointer).
    pub fn hit_path(&mut self, x: f32, y: f32) -> Vec<u64> {
        let mut out: Vec<u64> = Vec::new();
        for id in self.page.hit_path(x, y).into_iter().map(|id| id & !TEXT_BIT).filter(|id| *id >= REACT_IDS).map(|id| id - REACT_IDS) {
            if out.last() != Some(&id) {
                out.push(id);
            }
        }
        out
    }

    /// Each element's box as last laid out ([x, y, w, h] in page px), by its path among the page's elements.
    pub fn boxes(&mut self) -> Vec<(String, [f32; 4])> {
        self.page.boxes()
    }

    /// Frame `f` of the render's manifest (render.json): the videos, text and pictures the page shows, as laid out.
    pub fn manifest_frame(&mut self, f: i64) -> Value {
        let (videos, text, pictures) = self.page.manifest();
        serde_json::json!({"frame": f, "videos": videos, "text": text, "pictures": pictures})
    }

    /// The page laid out, nothing drawn (`raea world`): what `manifest_frame` and `boxes` read after a `sync`.
    pub fn lay_out(&mut self) {
        self.page.lay_out();
    }

    /// The page drawn, nothing read back (live play): its picture is `texture`.
    pub fn draw(&mut self) {
        self.page.draw();
    }

    /// Whether the page has an element under all the others for a 3D frame (live play's 3D drawn under a page with no
    /// layered slot); its picture is `set_under`'s. Takes effect at the next `sync`.
    pub fn has_under(&mut self, on: bool) {
        self.under = on;
    }

    /// The 3D frame under the page: the top left `w` x `h` of `src` (raea-render's target on the shared device), copied there on
    /// the GPU.
    pub fn set_under(&mut self, src: &wgpu::Texture, w: u32, h: u32) {
        self.page.place_under(UNDER, &src.clone().into(), w, h);
    }

    /// The layered Canvas3D's frame as its slot's picture, made straight on the GPU as the render's is (live play;
    /// `set_scene` takes RGBA).
    pub fn set_scene_texture(&mut self, src: &wgpu::Texture, w: u32, h: u32) {
        if let Some(id) = self.slot {
            self.page.place_texture_at(id, &src.clone().into(), (0, 0), w, h, true);
        }
    }

    /// The faces the page draws with, for an exported game's player (which has no network and may have no core/fonts):
    /// each bundled face (core/fonts, its index.json's family, weight and style) as an @font-face with its file inlined
    /// as a data URL. None when there are none.
    pub fn packaged_fonts_css(&self) -> Option<String> {
        use base64::Engine as _;
        let dir = raea_io_home::home().join("core/fonts");
        let index: Vec<Value> = serde_json::from_slice(&std::fs::read(dir.join("index.json")).ok()?).ok()?;
        let mut css = String::new();
        for f in &index {
            let (Some(file), Some(family), Some(weight)) = (f["file"].as_str(), f["family"].as_str(), f["weight"].as_str()) else { continue };
            let Ok(bytes) = std::fs::read(dir.join(file)) else { continue };
            let style = f["style"].as_str().unwrap_or("normal");
            css.push_str(&format!(
                "@font-face {{ font-family: \"{}\"; font-weight: {weight}; font-style: {style}; src: url(\"data:font/woff2;base64,{}\") format(\"woff2\"); }}\n",
                family.replace('-', " "),
                base64::engine::general_purpose::STANDARD.encode(&bytes)
            ));
        }
        (!css.is_empty()).then_some(css)
    }

    /// The app screens on the page as last synced: each one's name, size and module.
    pub(crate) fn app_screens(&self) -> Vec<(String, u32, u32, String)> {
        self.app_screens.clone()
    }

    /// Live play's input routed to the app screens.
    pub(crate) fn apps(&mut self) -> &mut crate::screen::Apps {
        &mut self.apps
    }

    /// Each <Screen>'s picture as last drawn, by name: its texture on the shared device (an app screen's too).
    pub fn screen_textures(&self) -> Vec<(String, wgpu::Texture)> {
        let apps = crate::screen::textures(&self.apps);
        self.page.sub_textures().into_iter().map(|(k, t, _, _)| (k, t)).chain(apps).map(|(k, t)| (k, (*t).clone())).collect()
    }

    /// The page's picture as last drawn, on the shared device (`Rgba8UnormSrgb`, premultiplied).
    pub fn texture(&self) -> Option<wgpu::Texture> {
        self.page.texture().map(|t| (*t).clone())
    }

    /// The page's App (its World: element boxes for MCP `ui`, the screens' roots).
    pub fn app(&mut self) -> &mut App {
        self.page.app()
    }
}
