//! stable-diffusion.cpp backend: runs inpainting by spawning `sd-cli` (ggml diffusion models:
//! FLUX.1 Fill, SDXL/SD 1.5 inpainting). No HTTP server is needed.
//!
//! One process per request. The source and the mask are written as PNGs into a temporary
//! directory, `sd-cli` writes the result there, and the process is killed when it outlives
//! the configured timeout (the model load of a quantized FLUX counts towards it).
//!
//! PhotoCraft's mask is white where the picture should be replaced, the convention
//! stable-diffusion.cpp uses for `--mask`. The result is resampled to the selected region.

use std::path::PathBuf;

use crate::{InferenceBackend, InferenceRequest, InferenceResult, MlError, Task};

/// `sd-cli` plus the model files it needs.
#[derive(Clone, Debug)]
pub struct SdCliBackend {
    /// Path to `sd-cli.exe` / `sd-cli`.
    pub cli: PathBuf,
    /// The diffusion model (`--diffusion-model`), GGUF or safetensors.
    pub model: String,
    /// The VAE (`--vae`).
    pub vae: String,
    /// CLIP-L text encoder (`--clip_l`), optional for SDXL.
    pub clip_l: String,
    /// T5-XXL text encoder (`--t5xxl`), required by FLUX.
    pub t5xxl: String,
    /// Extra arguments appended verbatim, e.g. `--steps 20 --guidance 30 --cfg-scale 1`.
    pub extra_args: String,
    /// How long a run may take, in seconds.
    pub timeout_secs: u32,
}

/// Everything one run needs beyond the model paths.
#[cfg(any(feature = "native", test))]
struct RunParams<'a> {
    source: &'a str,
    mask: &'a str,
    output: &'a str,
    width: u32,
    height: u32,
    prompt: &'a str,
    negative: &'a str,
    seed: Option<u64>,
    strength: f32,
}

impl SdCliBackend {
    pub fn new(
        cli: impl Into<PathBuf>,
        model: impl Into<String>,
        vae: impl Into<String>,
        clip_l: impl Into<String>,
        t5xxl: impl Into<String>,
        extra_args: impl Into<String>,
        timeout_secs: u32,
    ) -> Self {
        Self {
            cli: cli.into(),
            model: model.into(),
            vae: vae.into(),
            clip_l: clip_l.into(),
            t5xxl: t5xxl.into(),
            extra_args: extra_args.into(),
            timeout_secs,
        }
    }

    /// The arguments for one inpainting run (separated for tests).
    #[cfg(any(feature = "native", test))]
    fn args_for(&self, p: &RunParams<'_>) -> Vec<String> {
        let mut args: Vec<String> = vec![
            "--diffusion-model".into(),
            self.model.clone(),
            "--vae".into(),
            self.vae.clone(),
            "-i".into(),
            p.source.into(),
            "--mask".into(),
            p.mask.into(),
            "-o".into(),
            p.output.into(),
            "-W".into(),
            p.width.to_string(),
            "-H".into(),
            p.height.to_string(),
            "-p".into(),
            p.prompt.into(),
            "--strength".into(),
            format!("{:.3}", p.strength),
        ];
        if !self.clip_l.trim().is_empty() {
            args.push("--clip_l".into());
            args.push(self.clip_l.clone());
        }
        if !self.t5xxl.trim().is_empty() {
            args.push("--t5xxl".into());
            args.push(self.t5xxl.clone());
        }
        if !p.negative.is_empty() {
            args.push("-n".into());
            args.push(p.negative.into());
        }
        if let Some(seed) = p.seed {
            args.push("-s".into());
            args.push(seed.to_string());
        }
        // The extra arguments may carry a quoted path; a plain split keeps each token.
        args.extend(self.extra_args.split_whitespace().map(str::to_string));
        args
    }

    #[cfg(feature = "native")]
    fn run_cli(&self, req: InferenceRequest) -> Result<InferenceResult, MlError> {
        use std::process::{Command, Stdio};

        if req.task != Task::Inpaint {
            return Err(MlError::Unsupported);
        }
        let Some(mask) = req.mask.as_ref() else {
            return Err(MlError::BadParam("inpainting needs a mask".into()));
        };
        if !self.cli.is_file() {
            return Err(MlError::BadParam(format!("sd-cli not found at `{}`", self.cli.display())));
        }
        if self.model.trim().is_empty() {
            return Err(MlError::BadParam("no diffusion model configured (Preferences > AI)".into()));
        }

        let dir = std::env::temp_dir().join(format!(
            "photocraft-ai-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).map_err(|e| MlError::BadParam(format!("temp dir: {e}")))?;
        let source_png = dir.join("source.png");
        let mask_png = dir.join("mask.png");
        let out_png = dir.join("out.png");
        let log_path = dir.join("stderr.txt");
        let shape = |p: &std::path::Path| p.to_string_lossy().into_owned();

        let write = |path: &std::path::Path, surface: &photocraft_raster::Surface| {
            crate::backend::encode_png_bytes(surface, req.region)
                .and_then(|bytes| std::fs::write(path, bytes).map_err(|e| MlError::BadParam(format!("write {}: {e}", path.display()))))
        };
        let result = write(&source_png, &req.image).and_then(|()| write(&mask_png, mask)).and_then(|()| {
            // FLUX and SD both want latent-aligned sizes; the result is resampled back.
            let width = req.region.width().max(1).next_multiple_of(32);
            let height = req.region.height().max(1).next_multiple_of(32);
            let source = shape(&source_png);
            let mask_path = shape(&mask_png);
            let output = shape(&out_png);
            let params = RunParams {
                source: &source,
                mask: &mask_path,
                output: &output,
                width,
                height,
                prompt: req.prompt.as_deref().unwrap_or(""),
                negative: req.negative_prompt.as_deref().unwrap_or(""),
                seed: req.seed,
                strength: req.strength.clamp(0.0, 1.0),
            };
            let args = self.args_for(&params);
            let log = std::fs::File::create(&log_path).map_err(|e| MlError::BadParam(format!("log file: {e}")))?;
            let err_log = log.try_clone().map_err(|e| MlError::BadParam(format!("log file: {e}")))?;
            let mut cmd = Command::new(&self.cli);
            cmd.args(&args).stdin(Stdio::null()).stdout(log).stderr(err_log);
            // A console program started from the GUI would otherwise flash a black window.
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
            }
            let mut child = cmd
                .spawn()
                .map_err(|e| MlError::BadParam(format!("start `{}`: {e}", self.cli.display())))?;

            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(u64::from(self.timeout_secs.max(1)));
            let status = loop {
                match child.try_wait() {
                    Ok(Some(status)) => break status,
                    Ok(None) if std::time::Instant::now() < deadline => std::thread::sleep(std::time::Duration::from_millis(100)),
                    Ok(None) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(MlError::Response(format!(
                            "sd-cli did not finish within {} s (raise AI › Local timeout)",
                            self.timeout_secs
                        )));
                    }
                    Err(e) => return Err(MlError::Response(format!("sd-cli: {e}"))),
                }
            };
            if !status.success() {
                let log = std::fs::read_to_string(&log_path).unwrap_or_default();
                return Err(MlError::Response(format!("sd-cli failed ({}): {}", status, tail(&log, 600))));
            }
            let bytes = std::fs::read(&out_png)
                .map_err(|e| MlError::InvalidOutput(format!("sd-cli wrote no output: {e}")))?;
            let img = photocraft_codecs::decode(&bytes).map_err(|e| MlError::InvalidOutput(e.to_string()))?;
            Ok(InferenceResult::Pixels(super::pixels::surface_from_image(&img, req.region)?))
        });
        let _ = std::fs::remove_dir_all(&dir);
        result
    }
}

/// The last `max` characters of a log (errors and the useful sampler line are at the end).
#[cfg(feature = "native")]
fn tail(text: &str, max: usize) -> &str {
    let len = text.chars().count();
    if len <= max {
        return text;
    }
    let skip = len - max;
    match text.char_indices().nth(skip) {
        Some((at, _)) => &text[at..],
        None => text,
    }
}

impl InferenceBackend for SdCliBackend {
    fn id(&self) -> &str {
        "sd-cli"
    }

    fn supports(&self, task: Task) -> bool {
        cfg!(feature = "native") && matches!(task, Task::Inpaint)
    }

    fn run(&self, req: InferenceRequest) -> Result<InferenceResult, MlError> {
        #[cfg(not(feature = "native"))]
        {
            let _ = req;
            Err(MlError::Unsupported)
        }

        #[cfg(feature = "native")]
        {
            self.run_cli(req)
        }
    }

    fn ping(&self) -> Result<(), MlError> {
        if !self.cli.is_file() {
            return Err(MlError::BadParam(format!("sd-cli not found at `{}`", self.cli.display())));
        }
        if self.model.trim().is_empty() {
            return Err(MlError::BadParam("no diffusion model configured".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backend(extra: &str) -> SdCliBackend {
        SdCliBackend::new(
            "sd-cli.exe",
            "flux-fill-Q4_0.gguf",
            "ae.safetensors",
            "clip_l.safetensors",
            "t5xxl-Q4_K_M.gguf",
            extra,
            120,
        )
    }

    #[test]
    fn the_command_carries_the_model_the_mask_size_and_the_extra_arguments() {
        let b = backend("--steps 20 --guidance 30 --cfg-scale 1");
        let p = RunParams {
            source: "in.png",
            mask: "mask.png",
            output: "out.png",
            width: 736,
            height: 416,
            prompt: "a red wall",
            negative: "blurry",
            seed: Some(7),
            strength: 0.85,
        };
        let args = b.args_for(&p);
        let joined = args.join(" ");
        assert!(joined.contains("--diffusion-model flux-fill-Q4_0.gguf"), "{joined}");
        assert!(joined.contains("--vae ae.safetensors"), "{joined}");
        assert!(joined.contains("--clip_l clip_l.safetensors"), "{joined}");
        assert!(joined.contains("--t5xxl t5xxl-Q4_K_M.gguf"), "{joined}");
        assert!(joined.contains("-i in.png --mask mask.png -o out.png"), "{joined}");
        assert!(joined.contains("-W 736 -H 416"), "{joined}");
        assert!(joined.contains("-p a red wall"), "{joined}");
        assert!(joined.contains("-n blurry"), "{joined}");
        assert!(joined.contains("-s 7"), "{joined}");
        assert!(joined.contains("--strength 0.850"), "{joined}");
        assert!(joined.ends_with("--steps 20 --guidance 30 --cfg-scale 1"), "{joined}");
    }

    #[test]
    fn empty_encoders_and_prompts_are_omitted() {
        let b = SdCliBackend::new("sd-cli.exe", "m.gguf", "vae.safetensors", "", "", "", 30);
        let p = RunParams {
            source: "i.png",
            mask: "m.png",
            output: "o.png",
            width: 512,
            height: 512,
            prompt: "",
            negative: "",
            seed: None,
            strength: 1.0,
        };
        let args = b.args_for(&p);
        let joined = args.join(" ");
        assert!(!joined.contains("--clip_l") && !joined.contains("--t5xxl"), "{joined}");
        assert!(!joined.contains(" -n ") && !joined.contains(" -s "), "{joined}");
        assert!(args.windows(2).any(|w| w[0] == "-p" && w[1].is_empty()), "an empty prompt is still passed: {joined}");
    }

    #[test]
    fn ping_reports_a_missing_binary_without_spawning_it() {
        let b = SdCliBackend::new("definitely-not-here/sd-cli.exe", "m.gguf", "", "", "", "", 30);
        assert!(matches!(b.ping(), Err(MlError::BadParam(_))));
    }
}
