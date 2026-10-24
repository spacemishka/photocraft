//! ComfyUI backend: runs the AI retouching jobs through a user's local ComfyUI.
//!
//! A ComfyUI workflow in **API format** (Workflow ▸ Export (API)) is the model: the file is
//! copied as JSON and patched at run time. Placeholders, anywhere in a string value:
//!
//! | Placeholder           | Value                                                    |
//! |-----------------------|----------------------------------------------------------|
//! | `{{image}}`           | name of the uploaded source image in ComfyUI's input     |
//! | `{{mask}}`            | name of the uploaded mask (white = repaint)              |
//! | `{{prompt}}`          | the command's prompt (`""` when none)                    |
//! | `{{negative_prompt}}` | the command's negative prompt                            |
//! | `{{seed}}`            | number                                                   |
//! | `{{strength}}`        | number, 0..1 (a KSampler's `denoise`)                    |
//! | `{{width}}`, `{{height}}` | numbers: the region's pixel size                     |
//! | `{{task}}`            | task name (`inpaint`)                                    |
//!
//! A value that is exactly a numeric placeholder becomes a JSON number, so it can feed
//! `LoadImage`, `LoadImageMask`, `CLIPTextEncode`, `KSampler` (`seed`, `denoise`, steps) and
//! `EmptyLatentImage` directly. Anything else is text.
//!
//! Flow: upload image + mask (`POST /upload/image`), queue the patched workflow
//! (`POST /prompt`), poll `GET /history/{id}`, fetch the first output image
//! (`GET /view`). Segmentation stays with the engine's classical fallback: `supports` is
//! true for inpainting only.

use std::path::PathBuf;

#[cfg(feature = "native")]
use std::time::{Duration, Instant};

#[cfg(any(feature = "native", test))]
use serde_json::Value;

use crate::{InferenceBackend, InferenceRequest, InferenceResult, MlError, Task};

/// A ComfyUI server plus the workflow that implements inpainting.
#[derive(Clone, Debug)]
pub struct ComfyBackend {
    /// Base URL, e.g. `http://127.0.0.1:8188`.
    pub base_url: String,
    /// Path to the workflow JSON in ComfyUI's API format.
    pub workflow: PathBuf,
    /// How long to wait for a queued prompt, in seconds.
    pub timeout_secs: u32,
}

impl ComfyBackend {
    pub fn new(base_url: impl Into<String>, workflow: impl Into<PathBuf>, timeout_secs: u32) -> Self {
        Self { base_url: base_url.into(), workflow: workflow.into(), timeout_secs }
    }
}

impl InferenceBackend for ComfyBackend {
    fn id(&self) -> &str {
        "comfyui"
    }

    fn supports(&self, task: Task) -> bool {
        cfg!(feature = "native") && matches!(task, Task::Inpaint)
    }

    fn run(&self, req: InferenceRequest) -> Result<InferenceResult, MlError> {
        #[cfg(not(feature = "native"))]
        {
            let _ = req;
            return Err(MlError::Unsupported);
        }

        #[cfg(feature = "native")]
        {
            self.run_native(req)
        }
    }

    fn ping(&self) -> Result<(), MlError> {
        #[cfg(not(feature = "native"))]
        return Err(MlError::Unsupported);

        #[cfg(feature = "native")]
        {
            let client = crate::backend::make_client(self.timeout_secs)?;
            let url = format!("{}/system_stats", self.base_url.trim_end_matches('/'));
            let resp = client.get(url).send().map_err(|e| MlError::Network(e.to_string()))?;
            if !resp.status().is_success() {
                return Err(MlError::Response(format!("ComfyUI: HTTP {}", resp.status().as_u16())));
            }
            Ok(())
        }
    }
}

/// One image in a history reply.
#[cfg(any(feature = "native", test))]
#[derive(Clone, Debug, PartialEq, Eq)]
struct ImageRef {
    filename: String,
    subfolder: String,
    kind: String,
}

/// Values substituted into the workflow's placeholders.
#[cfg(any(feature = "native", test))]
#[derive(Clone, Debug)]
struct Vars {
    image: String,
    mask: String,
    prompt: String,
    negative_prompt: String,
    seed: u64,
    strength: f32,
    width: u32,
    height: u32,
    task: &'static str,
}

#[cfg(any(feature = "native", test))]
impl Vars {
    /// The placeholder's value when it is a number, so JSON fields keep their type.
    #[cfg(any(feature = "native", test))]
    fn number(&self, name: &str) -> Option<Value> {
        match name {
            "seed" => Some(Value::from(self.seed)),
            "strength" => serde_json::Number::from_f64(f64::from(self.strength)).map(Value::Number),
            "width" => Some(Value::from(self.width)),
            "height" => Some(Value::from(self.height)),
            _ => None,
        }
    }

    #[cfg(any(feature = "native", test))]
    fn text(&self, name: &str) -> &str {
        match name {
            "image" => &self.image,
            "mask" => &self.mask,
            "prompt" => &self.prompt,
            "negative_prompt" => &self.negative_prompt,
            "task" => self.task,
            _ => "",
        }
    }

    /// Every placeholder as `(name, text)`, for substituting inside a longer string.
    #[cfg(any(feature = "native", test))]
    fn pairs(&self) -> [(&'static str, String); 8] {
        [
            ("image", self.image.clone()),
            ("mask", self.mask.clone()),
            ("prompt", self.prompt.clone()),
            ("negative_prompt", self.negative_prompt.clone()),
            ("seed", self.seed.to_string()),
            ("strength", self.strength.to_string()),
            ("width", self.width.to_string()),
            ("height", self.height.to_string()),
        ]
    }
}

/// `{{name}}` exactly, if that is all the string holds.
#[cfg(any(feature = "native", test))]
fn exact_placeholder(s: &str) -> Option<&str> {
    let name = s.strip_prefix("{{")?.strip_suffix("}}")?;
    (!name.is_empty() && !name.contains(['{', '}'])).then_some(name)
}

/// The workflow with every placeholder replaced (recursively, values only).
#[cfg(any(feature = "native", test))]
fn substitute(value: &Value, vars: &Vars) -> Value {
    match value {
        Value::String(s) => match exact_placeholder(s) {
            Some(name) => vars.number(name).unwrap_or_else(|| Value::String(vars.text(name).to_string())),
            None => {
                let mut out = s.clone();
                for (name, text) in vars.pairs() {
                    out = out.replace(&format!("{{{{{name}}}}}"), &text);
                }
                Value::String(out)
            }
        },
        Value::Array(items) => Value::Array(items.iter().map(|v| substitute(v, vars)).collect()),
        Value::Object(map) => Value::Object(map.iter().map(|(k, v)| (k.clone(), substitute(v, vars))).collect()),
        other => other.clone(),
    }
}

/// The first output image in a `GET /history/{id}` reply.
#[cfg(any(feature = "native", test))]
fn history_image(history: &Value, id: &str) -> Option<ImageRef> {
    let outputs = history.get(id)?.get("outputs")?.as_object()?;
    for node in outputs.values() {
        let Some(images) = node.get("images").and_then(Value::as_array) else { continue };
        for img in images {
            let filename = img.get("filename").and_then(Value::as_str)?;
            let subfolder = img.get("subfolder").and_then(Value::as_str).unwrap_or("");
            let kind = img.get("type").and_then(Value::as_str).unwrap_or("output");
            if kind == "output" {
                return Some(ImageRef { filename: filename.to_string(), subfolder: subfolder.to_string(), kind: kind.to_string() });
            }
        }
    }
    None
}

#[cfg(any(feature = "native", test))]
fn truncate(text: &str, max: usize) -> &str {
    match text.char_indices().nth(max) {
        Some((at, _)) => &text[..at],
        None => text,
    }
}

#[cfg(feature = "native")]
impl ComfyBackend {
    fn run_native(&self, req: InferenceRequest) -> Result<InferenceResult, MlError> {
        if req.task != Task::Inpaint {
            return Err(MlError::Unsupported);
        }
        let Some(mask) = req.mask.as_ref() else {
            return Err(MlError::BadParam("inpainting needs a mask".into()));
        };
        let text = std::fs::read_to_string(&self.workflow)
            .map_err(|e| MlError::BadParam(format!("workflow `{}`: {e}", self.workflow.display())))?;
        let workflow: Value = serde_json::from_str(&text).map_err(|e| MlError::BadParam(format!("workflow JSON: {e}")))?;

        let client = crate::backend::make_client(self.timeout_secs)?;
        let base = self.base_url.trim_end_matches('/');
        let image = crate::backend::encode_png_bytes(&req.image, req.region)?;
        let mask = crate::backend::encode_png_bytes(mask, req.region)?;
        let image = upload(&client, base, "photocraft-source.png", &image)?;
        let mask = upload(&client, base, "photocraft-mask.png", &mask)?;

        let vars = Vars {
            image,
            mask,
            prompt: req.prompt.clone().unwrap_or_default(),
            negative_prompt: req.negative_prompt.clone().unwrap_or_default(),
            seed: req.seed.unwrap_or(0),
            strength: req.strength.clamp(0.0, 1.0),
            width: req.region.width().max(1),
            height: req.region.height().max(1),
            task: req.task.api_name(),
        };
        let prompt_id = queue(&client, base, substitute(&workflow, &vars))?;
        let bytes = wait_for_output(&client, base, &prompt_id, self.timeout_secs)?;
        let img = photocraft_codecs::decode(&bytes).map_err(|e| MlError::InvalidOutput(e.to_string()))?;
        Ok(InferenceResult::Pixels(super::pixels::surface_from_image(&img, req.region)?))
    }
}

#[cfg(feature = "native")]
fn upload(client: &reqwest::blocking::Client, base: &str, name: &str, bytes: &[u8]) -> Result<String, MlError> {
    let part = reqwest::blocking::multipart::Part::bytes(bytes.to_vec())
        .file_name(name.to_string())
        .mime_str("image/png")
        .map_err(|e| MlError::Response(e.to_string()))?;
    let form = reqwest::blocking::multipart::Form::new()
        .part("image", part)
        .text("type", "input")
        .text("overwrite", "true");
    let resp = client.post(format!("{base}/upload/image")).multipart(form).send().map_err(net)?;
    let status = resp.status();
    let text = resp.text().map_err(|e| MlError::Response(e.to_string()))?;
    if !status.is_success() {
        return Err(MlError::Response(format!("ComfyUI upload: HTTP {}: {}", status.as_u16(), truncate(&text, 300))));
    }
    let reply: Value = serde_json::from_str(&text).map_err(|e| MlError::Response(format!("ComfyUI upload reply: {e}")))?;
    let uploaded = reply
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| MlError::Response("ComfyUI upload reply has no `name`".into()))?;
    let subfolder = reply.get("subfolder").and_then(Value::as_str).unwrap_or("");
    Ok(if subfolder.is_empty() { uploaded.to_string() } else { format!("{subfolder}/{uploaded}") })
}

#[cfg(feature = "native")]
fn queue(client: &reqwest::blocking::Client, base: &str, workflow: Value) -> Result<String, MlError> {
    let body = serde_json::json!({ "prompt": workflow, "client_id": "photocraft" });
    let resp = client.post(format!("{base}/prompt")).json(&body).send().map_err(net)?;
    let status = resp.status();
    let text = resp.text().map_err(|e| MlError::Response(e.to_string()))?;
    if !status.is_success() {
        return Err(MlError::Response(format!("ComfyUI prompt: HTTP {}: {}", status.as_u16(), truncate(&text, 500))));
    }
    let reply: Value = serde_json::from_str(&text).map_err(|e| MlError::Response(format!("ComfyUI prompt reply: {e}")))?;
    reply
        .get("prompt_id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| MlError::Response("ComfyUI prompt reply has no `prompt_id`".into()))
}

#[cfg(feature = "native")]
fn wait_for_output(client: &reqwest::blocking::Client, base: &str, id: &str, timeout_secs: u32) -> Result<Vec<u8>, MlError> {
    let deadline = Instant::now() + Duration::from_secs(u64::from(timeout_secs.max(1)));
    loop {
        let resp = client.get(format!("{base}/history/{id}")).send().map_err(net)?;
        if resp.status().is_success() {
            let text = resp.text().map_err(|e| MlError::Response(e.to_string()))?;
            let history: Value = serde_json::from_str(&text).map_err(|e| MlError::Response(format!("ComfyUI history: {e}")))?;
            if let Some(status) = history.get(id).and_then(|e| e.get("status"))
                && status.get("status_str").and_then(Value::as_str) == Some("error")
            {
                return Err(MlError::Response(format!("ComfyUI error: {}", truncate(&status.to_string(), 500))));
            }
            if let Some(img) = history_image(&history, id) {
                let resp = client.get(view_url(base, &img)).send().map_err(net)?;
                let status = resp.status();
                if !status.is_success() {
                    return Err(MlError::Response(format!("ComfyUI view: HTTP {}", status.as_u16())));
                }
                return resp.bytes().map(|b| b.to_vec()).map_err(|e| MlError::Response(e.to_string()));
            }
        }
        if Instant::now() >= deadline {
            return Err(MlError::Response(format!("ComfyUI: no output for prompt {id} within {timeout_secs} s")));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

#[cfg(feature = "native")]
fn net(e: reqwest::Error) -> MlError {
    MlError::Network(e.to_string())
}

/// `GET /view` for an image, with the filename percent-encoded.
#[cfg(any(feature = "native", test))]
fn view_url(base: &str, img: &ImageRef) -> String {
    format!(
        "{base}/view?filename={}&subfolder={}&type={}",
        encode_component(&img.filename),
        encode_component(&img.subfolder),
        encode_component(&img.kind)
    )
}

/// Percent-encode a query-string value (RFC 3986 unreserved characters stay).
#[cfg(any(feature = "native", test))]
fn encode_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars() -> Vars {
        Vars {
            image: "in/photocraft-source.png".into(),
            mask: "in/photocraft-mask.png".into(),
            prompt: "remove the sign".into(),
            negative_prompt: "blurry".into(),
            seed: 42,
            strength: 0.85,
            width: 700,
            height: 420,
            task: "inpaint",
        }
    }

    #[test]
    fn numeric_placeholders_become_numbers_and_text_stays_text() {
        let workflow = serde_json::json!({
            "1": { "inputs": { "image": "{{image}}", "seed": "{{seed}}", "scale": "{{strength}}",
                               "width": "{{width}}", "height": "{{height}}", "mask": "{{mask}}" } },
            "2": { "inputs": { "text": "remove {{prompt}} and {{negative_prompt}}" } }
        });
        let out = substitute(&workflow, &vars());
        let one = &out["1"]["inputs"];
        assert_eq!(one["image"], serde_json::json!("in/photocraft-source.png"));
        assert_eq!(one["mask"], serde_json::json!("in/photocraft-mask.png"));
        assert_eq!(one["seed"], serde_json::json!(42));
        assert!(one["scale"].is_number() && (one["scale"].as_f64().unwrap_or(0.0) - 0.85).abs() < 1e-3);
        assert_eq!(one["width"], serde_json::json!(700));
        assert_eq!(one["height"], serde_json::json!(420));
        assert_eq!(out["2"]["inputs"]["text"], serde_json::json!("remove remove the sign and blurry"));
    }

    #[test]
    fn unknown_placeholders_stay_text_instead_of_vanishing() {
        // An exact numeric placeholder with no number keeps its empty text value.
        let workflow = serde_json::json!({ "x": "{{unknown}}" });
        assert_eq!(substitute(&workflow, &vars()), serde_json::json!({ "x": "" }));
        // Inside a longer string an unknown placeholder is left alone, so a typo is visible.
        let workflow = serde_json::json!({ "x": "prefix {{unknown}} suffix" });
        assert_eq!(substitute(&workflow, &vars()), serde_json::json!({ "x": "prefix {{unknown}} suffix" }));
    }

    #[test]
    fn the_first_output_image_is_found_even_beside_preview_images() {
        let history = serde_json::json!({
            "p1": { "outputs": {
                "9": { "images": [ { "filename": "preview.png", "subfolder": "", "type": "temp" } ] },
                "10": { "images": [ { "filename": "out 1.png", "subfolder": "sub", "type": "output" } ] }
            } }
        });
        let img = history_image(&history, "p1").expect("an output image");
        assert_eq!(img, ImageRef { filename: "out 1.png".into(), subfolder: "sub".into(), kind: "output".into() });
        assert!(history_image(&history, "other").is_none());
        assert!(view_url("http://127.0.0.1:8188", &img).contains("filename=out%201.png"));
    }

    #[test]
    fn a_pending_history_entry_has_no_image() {
        let history = serde_json::json!({ "p1": { "status": { "status_str": "success" }, "outputs": {} } });
        assert!(history_image(&history, "p1").is_none());
    }
}
