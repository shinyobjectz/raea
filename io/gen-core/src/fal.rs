//! fal directly, for what made doesn't carry (3D: Tripo, Meshy, Rodin, Hunyuan 3D; any niche endpoint): a step
//! whose model is `fal:<endpoint id>`. A request goes on fal's queue (queue.fal.run), is polled at the status URL it
//! hands back and read from the response URL; the files are every `url` in the response. Media go inline as data
//! URIs. The key is FAL_KEY; prices come from fal's pricing API (a unit price per image, second or request).

use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{Value as J, json};

use crate::http::{Body, call, key_header};

pub struct Fal {
    headers: Vec<String>,
}

impl Fal {
    pub fn from_settings() -> Result<Fal, String> {
        Ok(Fal { headers: vec![key_header("FAL_KEY", "Key", "fal needs FAL_KEY in ~/.raea/env (fal.ai/dashboard/keys)")?] })
    }

    fn json(&self, method: &str, url: &str, body: Option<&J>) -> Result<J, String> {
        let r = call(method, url, &self.headers, body.map(Body::Json).unwrap_or(Body::None), None, 120)?;
        let v = r.json().map_err(|e| format!("fal {}: {e}", crate::http::redact(url)))?;
        if !r.ok() {
            return Err(format!("fal {} answered {}: {}", crate::http::redact(url), r.code, crate::http::clip(&v["detail"].as_str().map(str::to_string).unwrap_or(v.to_string()))));
        }
        Ok(v)
    }

    /// The endpoint's unit price in cents and what the unit is (`image`, `second`, `request`…), when fal publishes one.
    pub fn price(&self, endpoint: &str) -> Result<(f64, String), String> {
        let v = self.json("GET", &format!("https://api.fal.ai/v1/models/pricing?endpoint_id={endpoint}"), None)?;
        let p = v["prices"].as_array().and_then(|a| a.first()).ok_or(format!("fal has no price for {endpoint}"))?;
        Ok((p["unit_price"].as_f64().unwrap_or(0.0) * 100.0, p["unit"].as_str().unwrap_or("request").to_string()))
    }

    /// The endpoint's input fields (from its OpenAPI page), kept in ~/.cache/raea-gen/fal/: (name, is a list).
    pub fn inputs(&self, endpoint: &str) -> Vec<(String, bool)> {
        let cache = crate::made::cache_dir().join("fal").join(format!("{}.json", endpoint.replace('/', "__")));
        let spec = std::fs::read(&cache).ok().and_then(|b| serde_json::from_slice::<J>(&b).ok()).or_else(|| {
            let r = call("GET", &format!("https://fal.ai/api/openapi/queue/openapi.json?endpoint_id={endpoint}"), &[], Body::None, None, 30).ok()?;
            let v = r.json().ok().filter(|_| r.ok())?;
            let _ = std::fs::create_dir_all(cache.parent()?);
            let _ = std::fs::write(&cache, v.to_string());
            Some(v)
        });
        let Some(spec) = spec else { return vec![] };
        spec["components"]["schemas"].as_object().into_iter().flatten().filter(|(k, _)| k.ends_with("Input")).flat_map(|(_, v)| {
            v["properties"].as_object().into_iter().flatten().map(|(n, p)| (n.clone(), p["type"] == "array")).collect::<Vec<_>>()
        }).collect()
    }

    /// A request run to the end: fal's response JSON.
    /// A local file into fal's storage; its https URL, so a request carries a link rather than megabytes of data URI.
    pub fn upload(&self, file: &std::path::Path) -> Result<String, String> {
        let mime = crate::made::mime_of(file);
        let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        let v = self.json("POST", "https://rest.alpha.fal.ai/storage/upload/initiate?storage_type=fal-cdn-v3", Some(&json!({"content_type": mime, "file_name": name})))?;
        let (put, url) = (v["upload_url"].as_str().ok_or("fal storage: no upload_url")?, v["file_url"].as_str().ok_or("fal storage: no file_url")?);
        let r = call("PUT", put, &[format!("Content-Type: {mime}")], Body::File(file), None, 600)?;
        if !r.ok() {
            return Err(format!("fal storage upload of {} answered {}", file.display(), r.code));
        }
        Ok(url.to_string())
    }

    pub fn run(&self, endpoint: &str, input: &J, stall: Duration) -> Result<J, String> {
        let q = self.json("POST", &format!("https://queue.fal.run/{endpoint}"), Some(input))?;
        let status = q["status_url"].as_str().ok_or_else(|| format!("fal queue: no status_url in {q}"))?.to_string();
        let response = q["response_url"].as_str().ok_or("fal queue: no response_url")?.to_string();
        let (mut seen, mut since, mut nap) = (String::new(), Instant::now(), 1);
        loop {
            let s = self.json("GET", &status, None)?;
            let now = format!("{}:{}", s["status"], s["queue_position"]);
            match s["status"].as_str() {
                Some("COMPLETED") => break,
                Some("FAILED" | "ERROR") => return Err(format!("fal {endpoint} failed: {s}")),
                _ => {}
            }
            if now != seen {
                (seen, since) = (now, Instant::now());
            } else if since.elapsed() > stall {
                let _ = self.json("PUT", &format!("{}/cancel", status.trim_end_matches("/status")), None);
                return Err(format!("fal {endpoint} sat at {seen} for {}s, canceled", stall.as_secs()));
            }
            std::thread::sleep(Duration::from_secs(nap));
            nap = (nap + 1).min(6);
        }
        self.json("GET", &response, None)
    }
}

/// Every file URL in a response, in the order they appear (images[0].url, video.url, model_mesh.url, …).
pub fn urls(v: &J, out: &mut Vec<String>) {
    match v {
        J::Object(o) => {
            if let Some(u) = o.get("url").and_then(|u| u.as_str()).filter(|u| u.starts_with("http")) {
                out.push(u.to_string());
            }
            o.iter().filter(|(k, _)| k.as_str() != "url").for_each(|(_, x)| urls(x, out));
        }
        J::Array(a) => a.iter().for_each(|x| urls(x, out)),
        _ => {}
    }
}

/// A local file as a data URI, which fal's endpoints take wherever they take a URL.
pub fn data_uri(file: &Path) -> Result<String, String> {
    let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(format!("data:{};base64,{}", crate::made::mime_of(file), base64(&bytes)))
}

fn base64(b: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            s.push(if i <= c.len() { T[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    s
}

/// A step's inputs as the endpoint's fields: prompt, seed, negative_prompt; x_<name> as <name>; any other setting the
/// endpoint's schema names as it is (`resolution`, `aspect_ratio`: a 4K step on fal-ai/nano-banana-pro/edit came back
/// 1K while they were dropped); media into the endpoint's image field (from its schema: `image_url`,
/// `input_image_url`, `image_urls`, …).
pub fn body(inputs: &serde_json::Map<String, J>, media: &[(String, String)], fields: &[(String, bool)]) -> J {
    let mut b = json!({});
    for (k, v) in inputs {
        match k.as_str() {
            "prompt" | "seed" | "negative" => { b[if k == "negative" { "negative_prompt" } else { k.as_str() }] = v.clone(); }
            k if k.starts_with("x_") => { b[&k[2..]] = v.clone(); }
            k if fields.iter().any(|(n, _)| n == k) && b.get(k).is_none() => { b[k] = v.clone(); }
            _ => {}
        }
    }
    // a mask (image.edit's `mask`: where an inpainting model may paint) into the endpoint's mask field, not with the
    // pictures (it went into image_urls beside the picture, and a fill model read no mask); a last frame
    // (video.generate's `end_image`) into the endpoint's end-frame field
    for (name, default) in [("mask", "mask_url"), ("end_image", "end_image_url")] {
        if let Some((_, u)) = media.iter().find(|(n, _)| n == name) {
            let f = fields.iter().find(|(n, l)| !*l && if name == "mask" { n.contains(name) } else { n.starts_with(name) }).map(|(n, _)| n.clone()).unwrap_or_else(|| default.into());
            b[f.as_str()] = json!(u);
        }
    }
    let images: Vec<&String> = media.iter().filter(|(n, _)| n != "mask" && n != "end_image").map(|(_, u)| u).collect();
    if images.is_empty() {
        return b;
    }
    // the plain field first (image_url, image_urls), then any other picture field that isn't an end frame: Seedance's
    // schema lists end_image_url before image_url, and its start frame went in as the end frame (fal answered 422)
    let field = |list: bool| {
        let plain = if list { "image_urls" } else { "image_url" };
        fields.iter().find(|(n, l)| *l == list && n == plain)
            .or_else(|| fields.iter().find(|(n, l)| *l == list && n.contains("image_url") && !n.starts_with("end_")))
            .map(|(n, _)| n.clone())
    };
    match (images.len(), field(false), field(true)) {
        (1, Some(one), _) => b[one.as_str()] = json!(images[0]),
        (_, _, Some(many)) => b[many.as_str()] = json!(images),
        (_, Some(one), None) => b[one.as_str()] = json!(images[0]),
        // no schema to read: fal's most common names
        (1, None, None) => b["image_url"] = json!(images[0]),
        _ => b["image_urls"] = json!(images),
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_start_frame_goes_to_image_url_not_the_end_frame() {
        // Seedance 2.5's schema order: end_image_url comes before image_url
        let fields: Vec<(String, bool)> = ["generate_audio", "resolution", "end_image_url", "image_url", "aspect_ratio", "prompt", "duration"].iter().map(|n| (n.to_string(), false)).collect();
        let mut inputs = serde_json::Map::new();
        inputs.insert("resolution".into(), json!("720p"));
        inputs.insert("aspect_ratio".into(), json!("9:16"));
        let b = body(&inputs, &[("start_image".to_string(), "https://f/start.jpg".to_string())], &fields);
        assert_eq!(b["image_url"], "https://f/start.jpg");
        assert!(b.get("end_image_url").is_none());
        assert_eq!((b["resolution"].as_str(), b["aspect_ratio"].as_str()), (Some("720p"), Some("9:16")));
        let b = body(&inputs, &[("start_image".into(), "https://f/a.jpg".into()), ("end_image".into(), "https://f/z.jpg".into())], &fields);
        assert_eq!((b["image_url"].as_str(), b["end_image_url"].as_str()), (Some("https://f/a.jpg"), Some("https://f/z.jpg")));
    }

    #[test]
    fn a_mask_goes_to_the_mask_field() {
        let fields = vec![("image_url".to_string(), false), ("mask_url".to_string(), false), ("prompt".to_string(), false)];
        let media = vec![("image".to_string(), "https://f/pic.png".to_string()), ("mask".to_string(), "https://f/mask.png".to_string())];
        let b = body(&serde_json::Map::new(), &media, &fields);
        assert_eq!(b["image_url"], json!("https://f/pic.png"));
        assert_eq!(b["mask_url"], json!("https://f/mask.png"));
        assert!(b.get("image_urls").is_none());
    }

    #[test]
    fn settings_the_schema_names_go_through() {
        let inputs = json!({"prompt": "a face", "seed": 7, "resolution": "4K", "aspect_ratio": "1:1", "image": "x", "x_safety_tolerance": "4"});
        let fields = vec![("resolution".to_string(), false), ("aspect_ratio".to_string(), false), ("image_urls".to_string(), true), ("prompt".to_string(), false)];
        let b = body(inputs.as_object().unwrap(), &[("image".into(), "https://f/1.png".into())], &fields);
        assert_eq!(b["resolution"], "4K");
        assert_eq!(b["aspect_ratio"], "1:1");
        assert_eq!(b["safety_tolerance"], "4");
        assert_eq!(b["image_urls"], json!(["https://f/1.png"]));
        // a setting the endpoint doesn't name is not sent (made.app's names that fal has no field for)
        assert!(b.get("image").is_none());
        let none = body(inputs.as_object().unwrap(), &[], &[]);
        assert!(none.get("resolution").is_none());
    }
}
