//! Whisper module — local-first speech-to-text via whisper.cpp.
//!
//! ## Batch commands
//!
//! | Command                  | Payload                                                            | Result                                       |
//! |--------------------------|--------------------------------------------------------------------|----------------------------------------------|
//! | `whisper.load_model`     | `{ path, name? }`                                                  | `{ id, name }`                               |
//! | `whisper.unload_model`   | `{ id }`                                                           | `{ unloaded: bool }`                         |
//! | `whisper.list_models`    | `{}`                                                               | `{ models: [{id, name, path}] }`             |
//! | `whisper.transcribe_file`| `{ id, path, language?, threads? }`                                | `{ segments: [{start_ms, end_ms, text}] }`   |
//! | `whisper.transcribe_pcm` | `{ id, samples_f32: [..], sample_rate, language?, threads? }`      | `{ segments: [{start_ms, end_ms, text}] }`   |
//!
//! ## Streaming commands
//!
//! | Command                  | Payload                                                            | Result                                       |
//! |--------------------------|--------------------------------------------------------------------|----------------------------------------------|
//! | `whisper.create_session` | `{ model_id, language?, threads? }`                                | `{ session_id }`                             |
//! | `whisper.push_audio`     | `{ session_id, samples: [f32] }`                                   | `{}`                                         |
//! | `whisper.poll_segments`  | `{ session_id }`                                                   | `{ segments, audio_duration_ms }`            |
//! | `whisper.close_session`  | `{ session_id }`                                                   | `{ segments, audio_duration_ms }`            |
//!
//! ## Architecture
//!
//! Whisper.cpp is the SPEC's chosen local-first STT engine. The
//! [`whisper-rs`](https://crates.io/crates/whisper-rs) crate vendors the
//! C++ source and exposes a sync API, which slots straight into the
//! daemon's sync `kernel.invoke` boundary without an async runtime.
//!
//! The module owns a [`WhisperManager`] of loaded models keyed by numeric
//! id. A `WhisperContext` is the heavy object — it pins the model weights
//! in memory, so callers `load_model` once at boot (or on-demand from a
//! KBar action) and reuse the resulting id across many transcription
//! calls. Each transcription call creates a fresh `WhisperState` from the
//! shared context, so two concurrent transcriptions on the same model
//! never trample each other.
//!
//! ### Streaming transcription
//!
//! `whisper.create_session` starts a streaming session that accumulates
//! audio via `push_audio`. `poll_segments` runs whisper on the entire
//! buffer and returns ALL segments (because whisper.cpp's `full()` resets
//! state each call — there is no true incremental append). The host diffs
//! with previous results to detect new/changed segments. This matches
//! how most real-time STT APIs work: the full transcript is returned and
//! the caller handles deduplication.
//!
//! For the SPEC's "Self-Dictation" pattern the host forks raw PCM to
//! both `whisper.push_audio` (for local transcription) and the
//! conferencing track pipeline (encoded to Opus for WebRTC transport).
//!
//! ### Audio input formats
//!
//! * `whisper.transcribe_file` accepts a WAV path. WAVs are decoded with
//!   [`hound`], integer samples are converted to f32, and stereo is
//!   downmixed to mono via the helpers `whisper-rs` re-exports from
//!   whisper.cpp's utility surface. The expected sample rate is 16 kHz —
//!   anything else is rejected up front so the caller knows to resample.
//! * `whisper.transcribe_pcm` accepts a JSON array of pre-decoded
//!   `f32` PCM samples plus the source sample rate, again rejecting
//!   anything that isn't 16 kHz mono.
//!
//! ## Feature gating
//!
//! `whisper-rs` builds whisper.cpp from source via CMake, so the `whisper`
//! Cargo feature is desktop-only — neither `mobile`, `wasm`, nor
//! `embedded` pull it in, and `full` deliberately omits it so the default
//! `cargo test` matrix does not require a CMake toolchain. Hosts that want
//! local STT opt in with `cargo build --features whisper` (and need
//! `cmake` on `PATH`).

use crate::builder::DaemonBuilder;
use crate::module::DaemonModule;
use crate::registry::CommandError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use whisper_rs::{
    convert_integer_to_float_audio, convert_stereo_to_mono_audio, FullParams, SamplingStrategy,
    WhisperContext, WhisperContextParameters,
};

/// The expected sample rate every Whisper model in this module assumes.
pub const WHISPER_SAMPLE_RATE: u32 = 16_000;

// ── Manager ───────────────────────────────────────────────────────────

/// Manager owning every loaded Whisper context and streaming session.
pub struct WhisperManager {
    next_id: AtomicU64,
    state: Mutex<HashMap<u64, ModelEntry>>,
    next_session_id: AtomicU64,
    sessions: Mutex<HashMap<u64, StreamingSession>>,
}

struct ModelEntry {
    name: String,
    path: PathBuf,
    context: Arc<WhisperContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadedModel {
    pub id: u64,
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

/// A streaming transcription session that accumulates audio in a buffer.
/// `poll_segments` runs `full()` on the entire buffer each time — there
/// is no incremental append in whisper.cpp, so the host diffs results.
struct StreamingSession {
    model_id: u64,
    buffer: Vec<f32>,
    language: Option<String>,
    n_threads: Option<i32>,
}

impl Default for WhisperManager {
    fn default() -> Self {
        Self::new()
    }
}

impl WhisperManager {
    /// Fresh, empty manager. No models loaded until `load_model` runs.
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            state: Mutex::new(HashMap::new()),
            next_session_id: AtomicU64::new(1),
            sessions: Mutex::new(HashMap::new()),
        }
    }

    // ── Model management ────────────────────────────────────────────

    pub fn load_model(&self, path: PathBuf, name: Option<String>) -> Result<u64, String> {
        if !path.exists() {
            return Err(format!("model path does not exist: {}", path.display()));
        }
        let path_str = path
            .to_str()
            .ok_or_else(|| format!("model path is not valid utf-8: {}", path.display()))?
            .to_string();
        let context =
            WhisperContext::new_with_params(&path_str, WhisperContextParameters::default())
                .map_err(|e| format!("WhisperContext::new_with_params: {e}"))?;

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let entry = ModelEntry {
            name: name.unwrap_or_else(|| {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("model-{id}"))
            }),
            path,
            context: Arc::new(context),
        };
        self.state
            .lock()
            .map_err(|_| "whisper state poisoned".to_string())?
            .insert(id, entry);
        Ok(id)
    }

    pub fn unload_model(&self, id: u64) -> Result<bool, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "whisper state poisoned".to_string())?;
        Ok(state.remove(&id).is_some())
    }

    pub fn list_models(&self) -> Vec<LoadedModel> {
        let state = match self.state.lock() {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let mut out: Vec<LoadedModel> = state
            .iter()
            .map(|(id, entry)| LoadedModel {
                id: *id,
                name: entry.name.clone(),
                path: entry.path.display().to_string(),
            })
            .collect();
        out.sort_by_key(|m| m.id);
        out
    }

    fn context(&self, id: u64) -> Result<Arc<WhisperContext>, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "whisper state poisoned".to_string())?;
        state
            .get(&id)
            .map(|e| e.context.clone())
            .ok_or_else(|| format!("unknown whisper model: {id}"))
    }

    // ── Batch transcription ─────────────────────────────────────────

    pub fn transcribe_pcm(
        &self,
        id: u64,
        samples: &[f32],
        sample_rate: u32,
        language: Option<&str>,
        threads: Option<i32>,
    ) -> Result<Vec<TranscriptSegment>, String> {
        if sample_rate != WHISPER_SAMPLE_RATE {
            return Err(format!(
                "whisper expects {WHISPER_SAMPLE_RATE} Hz mono PCM, got {sample_rate} Hz"
            ));
        }
        let ctx = self.context(id)?;
        let mut state = ctx
            .create_state()
            .map_err(|e| format!("create_state: {e}"))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        if let Some(lang) = language {
            params.set_language(Some(lang));
        }
        if let Some(n) = threads {
            params.set_n_threads(n);
        }
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        state
            .full(params, samples)
            .map_err(|e| format!("whisper full(): {e}"))?;

        let mut out: Vec<TranscriptSegment> = Vec::new();
        for segment in state.as_iter() {
            let text = segment
                .to_str_lossy()
                .map_err(|e| format!("segment.to_str_lossy: {e}"))?
                .into_owned();
            out.push(TranscriptSegment {
                start_ms: segment.start_timestamp() * 10,
                end_ms: segment.end_timestamp() * 10,
                text,
            });
        }
        Ok(out)
    }

    pub fn transcribe_file(
        &self,
        id: u64,
        path: PathBuf,
        language: Option<&str>,
        threads: Option<i32>,
    ) -> Result<Vec<TranscriptSegment>, String> {
        let reader = hound::WavReader::open(&path)
            .map_err(|e| format!("hound::WavReader::open({}): {e}", path.display()))?;
        let spec = reader.spec();
        if spec.sample_rate != WHISPER_SAMPLE_RATE {
            return Err(format!(
                "wav must be {WHISPER_SAMPLE_RATE} Hz, got {} Hz",
                spec.sample_rate
            ));
        }

        let int_samples: Vec<i16> = reader
            .into_samples::<i16>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("wav decode: {e}"))?;

        let mut float_samples = vec![0.0_f32; int_samples.len()];
        convert_integer_to_float_audio(&int_samples, &mut float_samples)
            .map_err(|e| format!("convert_integer_to_float_audio: {e}"))?;

        let mono_samples = match spec.channels {
            1 => float_samples,
            2 => {
                let mut mono = vec![0.0_f32; float_samples.len() / 2];
                convert_stereo_to_mono_audio(&float_samples, &mut mono)
                    .map_err(|e| format!("convert_stereo_to_mono_audio: {e}"))?;
                mono
            }
            n => return Err(format!("unsupported channel count: {n}")),
        };

        self.transcribe_pcm(id, &mono_samples, WHISPER_SAMPLE_RATE, language, threads)
    }

    // ── Streaming transcription ─────────────────────────────────────

    /// Start a streaming transcription session backed by the given model.
    pub fn create_session(
        &self,
        model_id: u64,
        language: Option<String>,
        n_threads: Option<i32>,
    ) -> Result<u64, String> {
        // Verify the model exists before allocating a session.
        let _ = self.context(model_id)?;
        let sid = self.next_session_id.fetch_add(1, Ordering::SeqCst);
        self.sessions
            .lock()
            .map_err(|_| "sessions poisoned".to_string())?
            .insert(
                sid,
                StreamingSession {
                    model_id,
                    buffer: Vec::new(),
                    language,
                    n_threads,
                },
            );
        Ok(sid)
    }

    /// Append PCM samples to a streaming session's buffer.
    pub fn push_audio(&self, session_id: u64, samples: &[f32]) -> Result<(), String> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| "sessions poisoned".to_string())?;
        let session = sessions
            .get_mut(&session_id)
            .ok_or_else(|| format!("unknown session: {session_id}"))?;
        session.buffer.extend_from_slice(samples);
        Ok(())
    }

    /// Transcribe the full accumulated buffer and return ALL segments.
    /// The host diffs with previous results to detect new/changed text.
    pub fn poll_segments(&self, session_id: u64) -> Result<(Vec<TranscriptSegment>, f64), String> {
        let (model_id, buffer, language, n_threads) = {
            let sessions = self
                .sessions
                .lock()
                .map_err(|_| "sessions poisoned".to_string())?;
            let session = sessions
                .get(&session_id)
                .ok_or_else(|| format!("unknown session: {session_id}"))?;
            (
                session.model_id,
                session.buffer.clone(),
                session.language.clone(),
                session.n_threads,
            )
        };
        if buffer.is_empty() {
            return Ok((Vec::new(), 0.0));
        }
        let audio_duration_ms = (buffer.len() as f64 / WHISPER_SAMPLE_RATE as f64) * 1000.0;
        let segments = self.transcribe_pcm(
            model_id,
            &buffer,
            WHISPER_SAMPLE_RATE,
            language.as_deref(),
            n_threads,
        )?;
        Ok((segments, audio_duration_ms))
    }

    /// Finalize a streaming session: transcribe the buffer one last time,
    /// then tear down the session. Returns final segments.
    pub fn close_session(&self, session_id: u64) -> Result<(Vec<TranscriptSegment>, f64), String> {
        let (model_id, buffer, language, n_threads) = {
            let mut sessions = self
                .sessions
                .lock()
                .map_err(|_| "sessions poisoned".to_string())?;
            let session = sessions
                .remove(&session_id)
                .ok_or_else(|| format!("unknown session: {session_id}"))?;
            (
                session.model_id,
                session.buffer,
                session.language,
                session.n_threads,
            )
        };
        if buffer.is_empty() {
            return Ok((Vec::new(), 0.0));
        }
        let audio_duration_ms = (buffer.len() as f64 / WHISPER_SAMPLE_RATE as f64) * 1000.0;
        let segments = self.transcribe_pcm(
            model_id,
            &buffer,
            WHISPER_SAMPLE_RATE,
            language.as_deref(),
            n_threads,
        )?;
        Ok((segments, audio_duration_ms))
    }
}

// ── Module wiring ──────────────────────────────────────────────────────

pub struct WhisperModule;

impl DaemonModule for WhisperModule {
    fn id(&self) -> &str {
        "prism.whisper"
    }

    fn install(&self, builder: &mut DaemonBuilder) -> Result<(), CommandError> {
        let manager = builder
            .whisper_manager_slot()
            .get_or_insert_with(|| Arc::new(WhisperManager::new()))
            .clone();
        let registry = builder.registry().clone();

        // ── Batch commands ──────────────────────────────────────────

        let m = manager.clone();
        registry.register("whisper.load_model", move |payload| {
            let args: LoadModelArgs = parse(payload, "whisper.load_model")?;
            let path = PathBuf::from(args.path);
            let id = m
                .load_model(path.clone(), args.name.clone())
                .map_err(|e| CommandError::handler("whisper.load_model", e))?;
            let name = args.name.unwrap_or_else(|| {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("model-{id}"))
            });
            Ok(json!({ "id": id, "name": name }))
        })?;

        let m = manager.clone();
        registry.register("whisper.unload_model", move |payload| {
            let args: IdArgs = parse(payload, "whisper.unload_model")?;
            let unloaded = m
                .unload_model(args.id)
                .map_err(|e| CommandError::handler("whisper.unload_model", e))?;
            Ok(json!({ "unloaded": unloaded }))
        })?;

        let m = manager.clone();
        registry.register("whisper.list_models", move |_payload| {
            Ok(json!({ "models": m.list_models() }))
        })?;

        let m = manager.clone();
        registry.register("whisper.transcribe_pcm", move |payload| {
            let args: TranscribePcmArgs = parse(payload, "whisper.transcribe_pcm")?;
            let segments = m
                .transcribe_pcm(
                    args.id,
                    &args.samples_f32,
                    args.sample_rate,
                    args.language.as_deref(),
                    args.threads,
                )
                .map_err(|e| CommandError::handler("whisper.transcribe_pcm", e))?;
            Ok(json!({ "segments": segments }))
        })?;

        let m = manager.clone();
        registry.register("whisper.transcribe_file", move |payload| {
            let args: TranscribeFileArgs = parse(payload, "whisper.transcribe_file")?;
            let segments = m
                .transcribe_file(
                    args.id,
                    PathBuf::from(args.path),
                    args.language.as_deref(),
                    args.threads,
                )
                .map_err(|e| CommandError::handler("whisper.transcribe_file", e))?;
            Ok(json!({ "segments": segments }))
        })?;

        // ── Streaming commands ──────────────────────────────────────

        let m = manager.clone();
        registry.register("whisper.create_session", move |payload| {
            let args: CreateSessionArgs = parse(payload, "whisper.create_session")?;
            let session_id = m
                .create_session(args.model_id, args.language, args.threads)
                .map_err(|e| CommandError::handler("whisper.create_session", e))?;
            Ok(json!({ "session_id": session_id }))
        })?;

        let m = manager.clone();
        registry.register("whisper.push_audio", move |payload| {
            let args: PushAudioArgs = parse(payload, "whisper.push_audio")?;
            m.push_audio(args.session_id, &args.samples)
                .map_err(|e| CommandError::handler("whisper.push_audio", e))?;
            Ok(json!({}))
        })?;

        let m = manager.clone();
        registry.register("whisper.poll_segments", move |payload| {
            let args: SessionIdArgs = parse(payload, "whisper.poll_segments")?;
            let (segments, duration_ms) = m
                .poll_segments(args.session_id)
                .map_err(|e| CommandError::handler("whisper.poll_segments", e))?;
            Ok(json!({ "segments": segments, "audio_duration_ms": duration_ms }))
        })?;

        let m = manager;
        registry.register("whisper.close_session", move |payload| {
            let args: SessionIdArgs = parse(payload, "whisper.close_session")?;
            let (segments, duration_ms) = m
                .close_session(args.session_id)
                .map_err(|e| CommandError::handler("whisper.close_session", e))?;
            Ok(json!({ "segments": segments, "audio_duration_ms": duration_ms }))
        })?;

        Ok(())
    }
}

// ── Helpers ────────────────────────────────────────────────────────────

fn parse<T: for<'de> Deserialize<'de>>(
    payload: JsonValue,
    command: &str,
) -> Result<T, CommandError> {
    serde_json::from_value::<T>(payload)
        .map_err(|e| CommandError::handler(command.to_string(), e.to_string()))
}

// ── Arg structs ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct LoadModelArgs {
    path: String,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TranscribePcmArgs {
    id: u64,
    samples_f32: Vec<f32>,
    sample_rate: u32,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    threads: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct TranscribeFileArgs {
    id: u64,
    path: String,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    threads: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct IdArgs {
    id: u64,
}

#[derive(Debug, Deserialize)]
struct CreateSessionArgs {
    model_id: u64,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    threads: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct PushAudioArgs {
    session_id: u64,
    samples: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct SessionIdArgs {
    session_id: u64,
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::DaemonBuilder;

    fn kernel() -> crate::DaemonKernel {
        DaemonBuilder::new().with_whisper().build().unwrap()
    }

    #[test]
    fn whisper_module_registers_every_command() {
        let kernel = kernel();
        let caps = kernel.capabilities();
        for name in [
            "whisper.load_model",
            "whisper.unload_model",
            "whisper.list_models",
            "whisper.transcribe_pcm",
            "whisper.transcribe_file",
            "whisper.create_session",
            "whisper.push_audio",
            "whisper.poll_segments",
            "whisper.close_session",
        ] {
            assert!(caps.contains(&name.to_string()), "missing {name}");
        }
    }

    #[test]
    fn list_models_starts_empty() {
        let mgr = WhisperManager::new();
        assert!(mgr.list_models().is_empty());
    }

    #[test]
    fn unload_unknown_id_returns_false() {
        let mgr = WhisperManager::new();
        assert!(!mgr.unload_model(42).unwrap());
    }

    #[test]
    fn load_model_rejects_missing_path() {
        let mgr = WhisperManager::new();
        let err = mgr
            .load_model(PathBuf::from("/nonexistent/model.bin"), None)
            .unwrap_err();
        assert!(err.contains("does not exist"));
    }

    #[test]
    fn transcribe_pcm_rejects_wrong_sample_rate() {
        let mgr = WhisperManager::new();
        let err = mgr
            .transcribe_pcm(1, &[0.0f32; 16], 44_100, None, None)
            .unwrap_err();
        assert!(err.contains("16000 Hz"));
    }

    #[test]
    fn transcribe_pcm_unknown_model_errors() {
        let mgr = WhisperManager::new();
        let err = mgr
            .transcribe_pcm(99, &[0.0f32; 16], WHISPER_SAMPLE_RATE, None, None)
            .unwrap_err();
        assert!(err.contains("unknown whisper model"));
    }

    #[test]
    fn preserved_injection_point_for_custom_manager() {
        let injected = Arc::new(WhisperManager::new());
        let mut builder = DaemonBuilder::new();
        *builder.whisper_manager_slot() = Some(injected.clone());
        let kernel = builder.with_whisper().build().unwrap();
        assert!(Arc::ptr_eq(&kernel.whisper_manager().unwrap(), &injected));
    }

    // ── Streaming session tests ─────────────────────────────────────

    #[test]
    fn create_session_rejects_unknown_model() {
        let mgr = WhisperManager::new();
        let err = mgr.create_session(999, None, None).unwrap_err();
        assert!(err.contains("unknown whisper model"));
    }

    #[test]
    fn push_audio_rejects_unknown_session() {
        let mgr = WhisperManager::new();
        let err = mgr.push_audio(999, &[0.0f32; 16]).unwrap_err();
        assert!(err.contains("unknown session"));
    }

    #[test]
    fn close_session_rejects_unknown_session() {
        let mgr = WhisperManager::new();
        let err = mgr.close_session(999).unwrap_err();
        assert!(err.contains("unknown session"));
    }

    #[test]
    fn streaming_commands_registered_via_kernel() {
        let kernel = kernel();
        // create_session should fail with unknown model, not missing command
        let result = kernel.invoke("whisper.create_session", json!({ "model_id": 1 }));
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("unknown whisper model"));
    }
}
