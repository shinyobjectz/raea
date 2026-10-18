//! Text lines read without Apple Vision: ocrs (MIT/Apache, on RTen) with its published detection and recognition
//! models, fetched once into ~/.cache/raea/ocrs. The same shape as vision::text_lines (text, confidence, box 0..1 from
//! the top left), so `read-text --backend ocrs` can stand in for Vision wherever it matches it on our measurements.

use std::path::PathBuf;

const MODELS: [(&str, &str); 2] = [
    ("text-detection.rten", "https://ocrs-models.s3-accelerate.amazonaws.com/text-detection.rten"),
    ("text-recognition.rten", "https://ocrs-models.s3-accelerate.amazonaws.com/text-recognition.rten"),
];

fn model(name: &str, url: &str) -> Result<PathBuf, String> {
    let dir = PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".cache/raea/ocrs");
    let p = dir.join(name);
    if !p.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let tmp = p.with_extension("part");
        let ok = std::process::Command::new("curl").args(["-fsSL", "-o"]).arg(&tmp).arg(url).status().map(|s| s.success()).unwrap_or(false);
        if !ok {
            return Err(format!("couldn't fetch {url}"));
        }
        std::fs::rename(&tmp, &p).map_err(|e| e.to_string())?;
    }
    Ok(p)
}

pub struct Reader {
    engine: ocrs::OcrEngine,
}

impl Reader {
    pub fn new() -> Result<Self, String> {
        let det = rten::Model::load_file(model(MODELS[0].0, MODELS[0].1)?).map_err(|e| e.to_string())?;
        let rec = rten::Model::load_file(model(MODELS[1].0, MODELS[1].1)?).map_err(|e| e.to_string())?;
        let engine = ocrs::OcrEngine::new(ocrs::OcrEngineParams { detection_model: Some(det), recognition_model: Some(rec), ..Default::default() })
            .map_err(|e| e.to_string())?;
        Ok(Self { engine })
    }

    /// Every line of text in an RGB8 picture: (text, confidence 1.0, box 0..1 from the top left). ocrs gives no
    /// per-line confidence, so lines it reads count as certain and the measurement decides.
    pub fn lines(&self, rgb: &[u8], w: u32, h: u32) -> Result<Vec<(String, f32, [f64; 4])>, String> {
        let src = ocrs::ImageSource::from_bytes(rgb, (w, h)).map_err(|e| e.to_string())?;
        let input = self.engine.prepare_input(src).map_err(|e| e.to_string())?;
        let words = self.engine.detect_words(&input).map_err(|e| e.to_string())?;
        let lines = self.engine.find_text_lines(&input, &words);
        let texts = self.engine.recognize_text(&input, &lines).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for (rects, text) in lines.iter().zip(texts.iter()) {
            let Some(text) = text else { continue };
            let t = text.to_string().trim().to_string();
            if t.chars().filter(|c| c.is_alphanumeric()).count() < 2 {
                continue;
            }
            let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for r in rects {
                for c in r.corners() {
                    x0 = x0.min(c.x); y0 = y0.min(c.y); x1 = x1.max(c.x); y1 = y1.max(c.y);
                }
            }
            out.push((t, 1.0, [x0 as f64 / w as f64, y0 as f64 / h as f64, (x1 - x0) as f64 / w as f64, (y1 - y0) as f64 / h as f64]));
        }
        Ok(out)
    }
}
