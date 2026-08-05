use arc_swap::ArcSwap;
use axum::Json;
use axum::Router;
use axum::extract::rejection::JsonRejection;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Component, Path, PathBuf};
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, Semaphore};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;
use zen_engine::loader::MemoryLoader;
use zen_engine::model::DecisionContent;
use zen_engine::nodes::http_handler::{HttpHandler, HttpHandlerRequest, HttpHandlerResponse};
use zen_engine::{DecisionEngine, ENGINE_VERSION, EvaluationOptions, ZEN_CONFIG};

pub const SERVICE_ID: &str = "zen-bre";
pub const WRAPPER_VERSION: &str = env!("CARGO_PKG_VERSION");

const DEFAULT_PORT: u16 = 18089;
const DEFAULT_MAX_BODY_BYTES: usize = 1_048_576;
const DEFAULT_MAX_MODEL_BYTES: u64 = 4 * 1024 * 1024;
const DEFAULT_MAX_TOTAL_MODEL_BYTES: u64 = 64 * 1024 * 1024;
const DEFAULT_MAX_MODELS: usize = 1_000;
const DEFAULT_MAX_CONCURRENCY: usize = 32;
const DEFAULT_EVALUATION_TIMEOUT_MS: u64 = 5_000;
const DEFAULT_MAX_DEPTH: u8 = 10;
const DEFAULT_MAX_JSON_DEPTH: usize = 64;
const MAX_DIAGNOSTICS: usize = 32;

#[derive(Debug, Clone)]
pub struct Config {
    pub host: IpAddr,
    pub port: u16,
    pub service_root: PathBuf,
    pub decisions_dir: PathBuf,
    pub state_dir: PathBuf,
    pub max_body_bytes: usize,
    pub max_model_bytes: u64,
    pub max_total_model_bytes: u64,
    pub max_models: usize,
    pub max_concurrency: usize,
    pub evaluation_timeout: Duration,
    pub max_depth: u8,
    pub max_json_depth: usize,
    pub allow_non_loopback: bool,
}

impl Config {
    pub fn from_env_and_args() -> Result<Self, String> {
        let service_root = env::var_os("SERVICE_ROOT")
            .map(PathBuf::from)
            .unwrap_or(env::current_dir().map_err(|_| "working directory is unavailable")?);
        let decisions_dir_override = env::var_os("ZEN_BRE_DECISIONS_DIR").map(PathBuf::from);
        let mut decisions_dir_is_explicit = decisions_dir_override.is_some();
        let mut config = Self {
            host: parse_env("ZEN_BRE_HOST", IpAddr::V4(Ipv4Addr::LOCALHOST))?,
            port: parse_env("ZEN_BRE_PORT", DEFAULT_PORT)?,
            decisions_dir: decisions_dir_override.unwrap_or_else(|| service_root.join("decisions")),
            state_dir: service_root.join(".state"),
            service_root,
            max_body_bytes: parse_env("ZEN_BRE_MAX_BODY_BYTES", DEFAULT_MAX_BODY_BYTES)?,
            max_model_bytes: parse_env("ZEN_BRE_MAX_MODEL_BYTES", DEFAULT_MAX_MODEL_BYTES)?,
            max_total_model_bytes: parse_env(
                "ZEN_BRE_MAX_TOTAL_MODEL_BYTES",
                DEFAULT_MAX_TOTAL_MODEL_BYTES,
            )?,
            max_models: parse_env("ZEN_BRE_MAX_MODELS", DEFAULT_MAX_MODELS)?,
            max_concurrency: parse_env("ZEN_BRE_MAX_CONCURRENCY", DEFAULT_MAX_CONCURRENCY)?,
            evaluation_timeout: Duration::from_millis(parse_env(
                "ZEN_BRE_EVALUATION_TIMEOUT_MS",
                DEFAULT_EVALUATION_TIMEOUT_MS,
            )?),
            max_depth: parse_env("ZEN_BRE_MAX_DEPTH", DEFAULT_MAX_DEPTH)?,
            max_json_depth: parse_env("ZEN_BRE_MAX_JSON_DEPTH", DEFAULT_MAX_JSON_DEPTH)?,
            allow_non_loopback: parse_env("ZEN_BRE_ALLOW_NON_LOOPBACK", false)?,
        };

        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--host" => config.host = parse_arg(&arg, args.next())?,
                "--port" => config.port = parse_arg(&arg, args.next())?,
                "--service-root" => {
                    config.service_root = PathBuf::from(required_arg(&arg, args.next())?);
                    config.state_dir = config.service_root.join(".state");
                    if !decisions_dir_is_explicit {
                        config.decisions_dir = config.service_root.join("decisions");
                    }
                }
                "--decisions-dir" => {
                    config.decisions_dir = PathBuf::from(required_arg(&arg, args.next())?);
                    decisions_dir_is_explicit = true;
                }
                "--max-body-bytes" => config.max_body_bytes = parse_arg(&arg, args.next())?,
                "--max-model-bytes" => config.max_model_bytes = parse_arg(&arg, args.next())?,
                "--max-total-model-bytes" => {
                    config.max_total_model_bytes = parse_arg(&arg, args.next())?
                }
                "--max-models" => config.max_models = parse_arg(&arg, args.next())?,
                "--max-concurrency" => config.max_concurrency = parse_arg(&arg, args.next())?,
                "--evaluation-timeout-ms" => {
                    let value: u64 = parse_arg(&arg, args.next())?;
                    config.evaluation_timeout = Duration::from_millis(value);
                }
                "--max-depth" => config.max_depth = parse_arg(&arg, args.next())?,
                "--max-json-depth" => config.max_json_depth = parse_arg(&arg, args.next())?,
                "--allow-non-loopback" => config.allow_non_loopback = true,
                "--help" | "-h" => {
                    println!(
                        "Usage: lasso-zen-bre [--host 127.0.0.1] [--port 18089] \
                         [--service-root PATH] [--decisions-dir PATH] \
                         [--max-body-bytes N] [--max-concurrency N] \
                         [--evaluation-timeout-ms N] [--max-depth N]"
                    );
                    std::process::exit(0);
                }
                other => return Err(format!("unsupported argument: {other}")),
            }
        }

        config.validate()?;
        Ok(config)
    }

    pub fn prepare(mut self) -> Result<Self, String> {
        fs::create_dir_all(&self.service_root)
            .map_err(|_| "service root could not be created".to_string())?;
        reject_symlink(&self.service_root, "service root")?;
        self.service_root = self
            .service_root
            .canonicalize()
            .map_err(|_| "service root could not be resolved".to_string())?;

        if self.decisions_dir.is_relative() {
            self.decisions_dir = self.service_root.join(&self.decisions_dir);
        }
        self.state_dir = self.service_root.join(".state");

        for (path, label) in [
            (&self.decisions_dir, "decisions directory"),
            (&self.state_dir, "state directory"),
            (&self.service_root.join("config"), "config directory"),
            (&self.service_root.join("logs"), "logs directory"),
        ] {
            fs::create_dir_all(path).map_err(|_| format!("{label} could not be created"))?;
            reject_symlink(path, label)?;
            let resolved = path
                .canonicalize()
                .map_err(|_| format!("{label} could not be resolved"))?;
            if !resolved.starts_with(&self.service_root) {
                return Err(format!("{label} must remain inside SERVICE_ROOT"));
            }
        }

        self.decisions_dir = self
            .decisions_dir
            .canonicalize()
            .map_err(|_| "decisions directory could not be resolved".to_string())?;
        self.state_dir = self
            .state_dir
            .canonicalize()
            .map_err(|_| "state directory could not be resolved".to_string())?;
        self.validate()?;
        Ok(self)
    }

    fn validate(&self) -> Result<(), String> {
        if !self.host.is_loopback() && !self.allow_non_loopback {
            return Err(
                "non-loopback binding requires the explicit --allow-non-loopback option".into(),
            );
        }
        if self.max_body_bytes == 0
            || self.max_model_bytes == 0
            || self.max_total_model_bytes == 0
            || self.max_models == 0
            || self.max_concurrency == 0
            || self.evaluation_timeout.is_zero()
            || self.max_depth == 0
            || self.max_json_depth == 0
        {
            return Err("resource limits must be greater than zero".into());
        }
        if self.max_model_bytes > self.max_total_model_bytes {
            return Err("max model bytes cannot exceed the total model byte limit".into());
        }
        Ok(())
    }

    #[cfg(test)]
    fn for_test(root: &Path) -> Self {
        Self {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            service_root: root.to_path_buf(),
            decisions_dir: root.join("decisions"),
            state_dir: root.join(".state"),
            max_body_bytes: DEFAULT_MAX_BODY_BYTES,
            max_model_bytes: DEFAULT_MAX_MODEL_BYTES,
            max_total_model_bytes: DEFAULT_MAX_TOTAL_MODEL_BYTES,
            max_models: DEFAULT_MAX_MODELS,
            max_concurrency: DEFAULT_MAX_CONCURRENCY,
            evaluation_timeout: Duration::from_millis(DEFAULT_EVALUATION_TIMEOUT_MS),
            max_depth: DEFAULT_MAX_DEPTH,
            max_json_depth: DEFAULT_MAX_JSON_DEPTH,
            allow_non_loopback: false,
        }
        .prepare()
        .expect("test workspace must prepare")
    }
}

fn parse_env<T>(name: &str, default: T) -> Result<T, String>
where
    T: FromStr,
{
    match env::var(name) {
        Ok(value) => value
            .parse::<T>()
            .map_err(|_| format!("{name} has an invalid value")),
        Err(env::VarError::NotPresent) => Ok(default),
        Err(env::VarError::NotUnicode(_)) => Err(format!("{name} must be valid Unicode")),
    }
}

fn required_arg(name: &str, value: Option<String>) -> Result<String, String> {
    value.ok_or_else(|| format!("{name} requires a value"))
}

fn parse_arg<T>(name: &str, value: Option<String>) -> Result<T, String>
where
    T: FromStr,
{
    required_arg(name, value)?
        .parse::<T>()
        .map_err(|_| format!("{name} has an invalid value"))
}

fn reject_symlink(path: &Path, label: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| format!("{label} is unavailable"))?;
    if metadata.file_type().is_symlink() {
        return Err(format!("{label} must not be a symbolic link"));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub decision_id: Option<String>,
    pub code: String,
}

#[derive(Debug)]
struct RegistryLoadError {
    diagnostics: Vec<Diagnostic>,
}

impl RegistryLoadError {
    fn one(code: &str) -> Self {
        Self {
            diagnostics: vec![Diagnostic {
                decision_id: None,
                code: code.to_string(),
            }],
        }
    }
}

struct Registry {
    engine: Option<DecisionEngine>,
    decision_keys: BTreeMap<String, String>,
    diagnostics: Vec<Diagnostic>,
    generation: u64,
    loaded_at_unix_ms: u128,
}

#[derive(Debug)]
struct DenyOutboundHttp;

impl HttpHandler for DenyOutboundHttp {
    fn handle(
        &self,
        _request: HttpHandlerRequest,
    ) -> Pin<Box<dyn Future<Output = Result<HttpHandlerResponse, String>> + Send + '_>> {
        Box::pin(async { Err("outbound HTTP is disabled".to_string()) })
    }
}

impl Registry {
    fn invalid(error: RegistryLoadError) -> Self {
        Self {
            engine: None,
            decision_keys: BTreeMap::new(),
            diagnostics: error.diagnostics,
            generation: 0,
            loaded_at_unix_ms: now_unix_ms(),
        }
    }

    fn load(config: &Config, generation: u64) -> Result<Self, RegistryLoadError> {
        let paths = scan_decision_paths(config)?;
        let loader = Arc::new(MemoryLoader::default());
        let mut decision_keys = BTreeMap::new();
        let mut reverse_keys = BTreeMap::new();
        let mut diagnostics = Vec::new();

        for path in paths {
            let (id, key) = match decision_identity(&config.decisions_dir, &path) {
                Ok(identity) => identity,
                Err(code) => {
                    push_diagnostic(&mut diagnostics, None, code);
                    continue;
                }
            };
            if decision_keys.contains_key(&id) {
                push_diagnostic(&mut diagnostics, Some(id), "duplicate_decision_id");
                continue;
            }
            let bytes = match fs::read(&path) {
                Ok(bytes) => bytes,
                Err(_) => {
                    push_diagnostic(&mut diagnostics, Some(id), "model_read_failed");
                    continue;
                }
            };
            let content = match serde_json::from_slice::<DecisionContent>(&bytes) {
                Ok(content) => content,
                Err(_) => {
                    push_diagnostic(&mut diagnostics, Some(id), "invalid_jdm");
                    continue;
                }
            };
            loader.add(key.clone(), content);
            reverse_keys.insert(key.clone(), id.clone());
            decision_keys.insert(id, key);
        }

        if !diagnostics.is_empty() {
            return Err(RegistryLoadError { diagnostics });
        }

        let engine = DecisionEngine::default()
            .with_loader(loader)
            .with_http_handler(Some(Arc::new(DenyOutboundHttp)));
        let failures = engine.compile();
        if !failures.is_empty() {
            let diagnostics = failures
                .into_iter()
                .take(MAX_DIAGNOSTICS)
                .map(|failure| Diagnostic {
                    decision_id: reverse_keys.get(failure.key.as_ref()).cloned(),
                    code: "compile_failed".to_string(),
                })
                .collect();
            return Err(RegistryLoadError { diagnostics });
        }

        Ok(Self {
            engine: Some(engine),
            decision_keys,
            diagnostics: Vec::new(),
            generation,
            loaded_at_unix_ms: now_unix_ms(),
        })
    }

    fn is_ready(&self) -> bool {
        self.engine.is_some() && self.diagnostics.is_empty()
    }
}

fn scan_decision_paths(config: &Config) -> Result<Vec<PathBuf>, RegistryLoadError> {
    let mut stack = vec![config.decisions_dir.clone()];
    let mut paths = Vec::new();
    let mut diagnostics = Vec::new();
    let mut total_bytes = 0_u64;

    while let Some(directory) = stack.pop() {
        let entries = fs::read_dir(&directory)
            .map_err(|_| RegistryLoadError::one("decisions_directory_unreadable"))?;
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => {
                    push_diagnostic(&mut diagnostics, None, "directory_entry_unreadable");
                    continue;
                }
            };
            let path = entry.path();
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(_) => {
                    push_diagnostic(&mut diagnostics, None, "model_metadata_unreadable");
                    continue;
                }
            };
            if metadata.file_type().is_symlink() {
                push_diagnostic(&mut diagnostics, None, "symlink_not_allowed");
                continue;
            }
            if metadata.is_dir() {
                stack.push(path);
                continue;
            }
            if !metadata.is_file() {
                push_diagnostic(&mut diagnostics, None, "unsupported_file_type");
                continue;
            }
            if path.file_name() == Some(OsStr::new(".gitkeep")) {
                continue;
            }
            if path.extension() != Some(OsStr::new("json")) {
                push_diagnostic(&mut diagnostics, None, "unsupported_file_type");
                continue;
            }
            if metadata.len() > config.max_model_bytes {
                push_diagnostic(&mut diagnostics, None, "model_too_large");
                continue;
            }
            total_bytes = total_bytes.saturating_add(metadata.len());
            if total_bytes > config.max_total_model_bytes {
                push_diagnostic(&mut diagnostics, None, "registry_too_large");
                continue;
            }
            paths.push(path);
            if paths.len() > config.max_models {
                push_diagnostic(&mut diagnostics, None, "too_many_models");
            }
        }
    }

    if !diagnostics.is_empty() {
        return Err(RegistryLoadError { diagnostics });
    }
    paths.sort();
    Ok(paths)
}

fn push_diagnostic(diagnostics: &mut Vec<Diagnostic>, decision_id: Option<String>, code: &str) {
    if diagnostics.len() < MAX_DIAGNOSTICS {
        diagnostics.push(Diagnostic {
            decision_id,
            code: code.to_string(),
        });
    }
}

fn decision_identity(root: &Path, path: &Path) -> Result<(String, String), &'static str> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| "path_outside_decisions_root")?;
    let mut key_segments = Vec::new();
    let mut id_segments = Vec::new();

    for (index, component) in relative.components().enumerate() {
        let Component::Normal(segment) = component else {
            return Err("invalid_decision_path");
        };
        let segment = segment.to_str().ok_or("invalid_decision_path")?;
        let is_last = index + 1 == relative.components().count();
        let id_segment = if is_last {
            segment
                .strip_suffix(".json")
                .ok_or("unsupported_file_type")?
        } else {
            segment
        };
        if !valid_id_segment(id_segment) {
            return Err("invalid_decision_id");
        }
        key_segments.push(segment.to_string());
        id_segments.push(id_segment.to_string());
    }

    if id_segments.is_empty() {
        return Err("invalid_decision_id");
    }
    Ok((id_segments.join("/"), key_segments.join("/")))
}

fn valid_decision_id(id: &str) -> bool {
    !id.is_empty() && id.split('/').all(valid_id_segment)
}

fn valid_id_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment != "."
        && segment != ".."
        && !segment.contains("..")
        && segment
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[derive(Clone)]
pub struct AppState {
    registry: Arc<ArcSwap<Registry>>,
    config: Arc<Config>,
    evaluation_slots: Arc<Semaphore>,
    reload_lock: Arc<Mutex<()>>,
    generation: Arc<AtomicU64>,
    correlation: Arc<AtomicU64>,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        let generation = 1;
        let initial = match Registry::load(&config, generation) {
            Ok(registry) => registry,
            Err(load_error) => Registry::invalid(load_error),
        };
        let max_concurrency = config.max_concurrency;
        let state = Self {
            registry: Arc::new(ArcSwap::from_pointee(initial)),
            config: Arc::new(config),
            evaluation_slots: Arc::new(Semaphore::new(max_concurrency)),
            reload_lock: Arc::new(Mutex::new(())),
            generation: Arc::new(AtomicU64::new(generation)),
            correlation: Arc::new(AtomicU64::new(1)),
        };
        let snapshot = state.registry.load_full();
        if snapshot.is_ready()
            && let Err(code) = write_registry_state(&state.config.state_dir, &snapshot)
        {
            warn!(
                service = SERVICE_ID,
                outcome = code,
                "registry state was not persisted"
            );
        }
        state
    }

    fn next_correlation_id(&self) -> String {
        format!(
            "zbre-{:016x}",
            self.correlation.fetch_add(1, Ordering::Relaxed)
        )
    }
}

pub fn build_router(state: AppState) -> Router {
    let max_body_bytes = state.config.max_body_bytes;
    Router::new()
        .route("/health/live", get(health_live))
        .route("/health/ready", get(health_ready))
        .route("/version", get(version))
        .route("/v1/decisions", get(list_decisions))
        .route("/v1/decisions/reload", post(reload_decisions))
        .route("/v1/decisions/{*path}", post(evaluate_decision))
        .fallback(not_found)
        .layer(DefaultBodyLimit::max(max_body_bytes))
        .with_state(state)
}

async fn health_live() -> Response {
    json_response(
        StatusCode::OK,
        json!({"service": SERVICE_ID, "status": "ok"}),
    )
}

async fn health_ready(State(state): State<AppState>) -> Response {
    let registry = state.registry.load_full();
    if registry.is_ready() {
        json_response(
            StatusCode::OK,
            json!({
                "service": SERVICE_ID,
                "status": "ready",
                "engine": "zen-engine",
                "engineVersion": ENGINE_VERSION,
                "decisionCount": registry.decision_keys.len(),
                "generation": registry.generation,
                "loadedAtUnixMs": registry.loaded_at_unix_ms,
            }),
        )
    } else {
        json_response(
            StatusCode::SERVICE_UNAVAILABLE,
            json!({
                "service": SERVICE_ID,
                "status": "not_ready",
                "code": "registry_invalid",
                "diagnostics": registry.diagnostics,
            }),
        )
    }
}

async fn version() -> Response {
    json_response(
        StatusCode::OK,
        json!({
            "service": SERVICE_ID,
            "wrapperVersion": WRAPPER_VERSION,
            "engine": "zen-engine",
            "engineVersion": ENGINE_VERSION,
            "arbitraryPrecision": true,
            "buildIdentity": option_env!("LASSO_ZEN_BRE_BUILD_SHA").unwrap_or("development"),
        }),
    )
}

async fn list_decisions(State(state): State<AppState>) -> Response {
    let registry = state.registry.load_full();
    let decisions = registry
        .decision_keys
        .keys()
        .map(|id| json!({"id": id, "status": "valid"}))
        .collect::<Vec<_>>();
    json_response(
        StatusCode::OK,
        json!({
            "service": SERVICE_ID,
            "valuePolicy": "metadata_only",
            "generation": registry.generation,
            "decisions": decisions,
        }),
    )
}

async fn reload_decisions(State(state): State<AppState>) -> Response {
    let correlation_id = state.next_correlation_id();
    let Ok(_guard) = state.reload_lock.try_lock() else {
        return api_error(
            StatusCode::CONFLICT,
            "reload_in_progress",
            "A registry reload is already in progress.",
            &correlation_id,
        );
    };
    let generation = state.generation.fetch_add(1, Ordering::SeqCst) + 1;
    let config = state.config.clone();
    let started = Instant::now();
    let result = tokio::task::spawn_blocking(move || Registry::load(&config, generation)).await;

    match result {
        Ok(Ok(registry)) => {
            let count = registry.decision_keys.len();
            let registry = Arc::new(registry);
            state.registry.store(registry.clone());
            let persistence = write_registry_state(&state.config.state_dir, &registry);
            info!(
                service = SERVICE_ID,
                correlation_id,
                decision_count = count,
                generation,
                duration_ms = started.elapsed().as_millis() as u64,
                outcome = "success",
                "decision registry reloaded"
            );
            json_response(
                StatusCode::OK,
                json!({
                    "service": SERVICE_ID,
                    "status": "reloaded",
                    "decisionCount": count,
                    "generation": generation,
                    "correlationId": correlation_id,
                    "statePersisted": persistence.is_ok(),
                }),
            )
        }
        Ok(Err(load_error)) => {
            warn!(
                service = SERVICE_ID,
                correlation_id,
                generation,
                duration_ms = started.elapsed().as_millis() as u64,
                outcome = "validation_failed",
                "decision registry reload rejected"
            );
            json_response(
                StatusCode::UNPROCESSABLE_ENTITY,
                json!({
                    "service": SERVICE_ID,
                    "status": "reload_rejected",
                    "code": "registry_invalid",
                    "diagnostics": load_error.diagnostics,
                    "correlationId": correlation_id,
                }),
            )
        }
        Err(_) => {
            error!(
                service = SERVICE_ID,
                correlation_id,
                outcome = "internal_error",
                "decision registry reload task failed"
            );
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "The registry could not be reloaded.",
                &correlation_id,
            )
        }
    }
}

async fn evaluate_decision(
    State(state): State<AppState>,
    axum::extract::Path(path): axum::extract::Path<String>,
    payload: Result<Json<Value>, JsonRejection>,
) -> Response {
    let correlation_id = state.next_correlation_id();
    let Some(id) = path.strip_suffix("/evaluate") else {
        return api_error(
            StatusCode::NOT_FOUND,
            "not_found",
            "The requested endpoint does not exist.",
            &correlation_id,
        );
    };
    if !valid_decision_id(id) {
        return api_error(
            StatusCode::BAD_REQUEST,
            "invalid_decision_id",
            "The decision identifier is invalid.",
            &correlation_id,
        );
    }
    let Json(context) = match payload {
        Ok(payload) => payload,
        Err(rejection) => {
            let status = rejection.status();
            let code = if status == StatusCode::PAYLOAD_TOO_LARGE {
                "request_too_large"
            } else {
                "invalid_json"
            };
            return api_error(
                status,
                code,
                "The JSON request body is invalid.",
                &correlation_id,
            );
        }
    };
    if json_depth(&context) > state.config.max_json_depth {
        return api_error(
            StatusCode::BAD_REQUEST,
            "context_too_deep",
            "The JSON context exceeds the configured nesting limit.",
            &correlation_id,
        );
    }

    let registry = state.registry.load_full();
    if !registry.is_ready() {
        return api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "registry_not_ready",
            "The decision registry is not ready.",
            &correlation_id,
        );
    }
    let Some(key) = registry.decision_keys.get(id).cloned() else {
        return api_error(
            StatusCode::NOT_FOUND,
            "decision_not_found",
            "The requested decision does not exist.",
            &correlation_id,
        );
    };
    let Some(engine) = registry.engine.clone() else {
        return api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "registry_not_ready",
            "The decision registry is not ready.",
            &correlation_id,
        );
    };
    let Ok(permit) = state.evaluation_slots.clone().try_acquire_owned() else {
        return api_error(
            StatusCode::TOO_MANY_REQUESTS,
            "concurrency_limit",
            "The evaluation concurrency limit has been reached.",
            &correlation_id,
        );
    };

    let max_depth = state.config.max_depth;
    let timeout = state.config.evaluation_timeout;
    let decision_id = id.to_string();
    let started = Instant::now();
    let task = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| "runtime_failed")?;
        runtime
            .block_on(engine.evaluate_with_opts(
                key,
                context.into(),
                EvaluationOptions {
                    trace: false,
                    max_depth,
                },
            ))
            .map(|response| response.result.to_value())
            .map_err(|_| "evaluation_failed")
    });

    match tokio::time::timeout(timeout, task).await {
        Ok(Ok(Ok(result))) => {
            info!(
                service = SERVICE_ID,
                correlation_id,
                decision_id,
                duration_ms = started.elapsed().as_millis() as u64,
                outcome = "success",
                "decision evaluated"
            );
            json_response(
                StatusCode::OK,
                json!({
                    "decisionId": decision_id,
                    "result": result,
                    "correlationId": correlation_id,
                }),
            )
        }
        Ok(Ok(Err(code))) => {
            warn!(
                service = SERVICE_ID,
                correlation_id,
                decision_id,
                duration_ms = started.elapsed().as_millis() as u64,
                outcome = code,
                "decision evaluation failed"
            );
            api_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                code,
                "The decision could not be evaluated.",
                &correlation_id,
            )
        }
        Ok(Err(_)) => {
            error!(
                service = SERVICE_ID,
                correlation_id,
                decision_id,
                outcome = "internal_error",
                "decision evaluation task failed"
            );
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "The decision could not be evaluated.",
                &correlation_id,
            )
        }
        Err(_) => {
            warn!(
                service = SERVICE_ID,
                correlation_id,
                decision_id,
                duration_ms = started.elapsed().as_millis() as u64,
                outcome = "evaluation_timeout",
                "decision evaluation timed out"
            );
            api_error(
                StatusCode::GATEWAY_TIMEOUT,
                "evaluation_timeout",
                "The decision exceeded the evaluation time limit.",
                &correlation_id,
            )
        }
    }
}

fn json_depth(value: &Value) -> usize {
    match value {
        Value::Array(values) => 1 + values.iter().map(json_depth).max().unwrap_or(0),
        Value::Object(values) => 1 + values.values().map(json_depth).max().unwrap_or(0),
        _ => 1,
    }
}

async fn not_found() -> Response {
    api_error(
        StatusCode::NOT_FOUND,
        "not_found",
        "The requested endpoint does not exist.",
        "none",
    )
}

fn json_response(status: StatusCode, body: Value) -> Response {
    (status, Json(body)).into_response()
}

fn api_error(status: StatusCode, code: &str, message: &str, correlation_id: &str) -> Response {
    json_response(
        status,
        json!({
            "error": {"code": code, "message": message},
            "correlationId": correlation_id,
        }),
    )
}

fn write_registry_state(state_dir: &Path, registry: &Registry) -> Result<(), &'static str> {
    let target = state_dir.join("registry.json");
    let temporary = state_dir.join("registry.json.tmp");
    let document = serde_json::to_vec_pretty(&json!({
        "generation": registry.generation,
        "decisionCount": registry.decision_keys.len(),
        "loadedAtUnixMs": registry.loaded_at_unix_ms,
        "engineVersion": ENGINE_VERSION,
    }))
    .map_err(|_| "state_serialization_failed")?;
    fs::write(&temporary, document).map_err(|_| "state_write_failed")?;
    fs::rename(&temporary, &target).map_err(|_| "state_replace_failed")?;
    Ok(())
}

pub async fn run(config: Config) -> Result<(), String> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .json()
        .with_env_filter(filter)
        .with_target(false)
        .flatten_event(true)
        .try_init();

    ZEN_CONFIG.function_timeout_millis.store(
        config.evaluation_timeout.as_millis() as u64,
        Ordering::Relaxed,
    );
    let address = SocketAddr::new(config.host, config.port);
    let state = AppState::new(config);
    let ready = state.registry.load().is_ready();
    let listener = TcpListener::bind(address)
        .await
        .map_err(|_| "listener bind failed".to_string())?;
    let local_address = listener
        .local_addr()
        .map_err(|_| "listener address unavailable".to_string())?;

    info!(
        service = SERVICE_ID,
        address = %local_address,
        engine_version = ENGINE_VERSION,
        wrapper_version = WRAPPER_VERSION,
        registry_ready = ready,
        outbound_http = false,
        "service started"
    );

    axum::serve(listener, build_router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|_| "HTTP server failed".to_string())?;
    info!(
        service = SERVICE_ID,
        outcome = "clean_shutdown",
        "service stopped"
    );
    Ok(())
}

#[cfg(unix)]
async fn shutdown_signal() {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("SIGTERM handler must install");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {},
        _ = terminate.recv() => {},
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tempfile::TempDir;
    use tower::ServiceExt;

    const EXPRESSION_DECISION: &str = include_str!("../tests/fixtures/expression.json");
    const TABLE_DECISION: &str = include_str!("../tests/fixtures/table.json");
    const SWITCH_DECISION: &str = include_str!("../tests/fixtures/switch.json");
    const FUNCTION_DECISION: &str = include_str!("../tests/fixtures/function.json");
    const NESTED_PARENT_DECISION: &str = include_str!("../tests/fixtures/nested-parent.json");
    const NESTED_CHILD_DECISION: &str = include_str!("../tests/fixtures/nested-child.json");
    const PRECISION_DECISION: &str = include_str!("../tests/fixtures/precision.json");

    fn workspace() -> (TempDir, Config) {
        let temp = tempfile::tempdir().expect("temp directory");
        let config = Config::for_test(temp.path());
        (temp, config)
    }

    fn write_decision(config: &Config, name: &str, content: &str) {
        let path = config.decisions_dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("fixture parent directory");
        }
        fs::write(path, content).expect("fixture must write");
    }

    async fn request(app: Router, method: &str, path: &str, body: &str) -> (StatusCode, Value) {
        let response = app
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .expect("request"),
            )
            .await
            .expect("response");
        let status = response.status();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let body = serde_json::from_slice(&bytes).expect("JSON response");
        (status, body)
    }

    async fn assert_evaluation(
        config: Config,
        decision_id: &str,
        context: Value,
        expected_path: &str,
        expected: Value,
    ) {
        let app = build_router(AppState::new(config));
        let (status, body) = request(
            app,
            "POST",
            &format!("/v1/decisions/{decision_id}/evaluate"),
            &context.to_string(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body.pointer(expected_path), Some(&expected));
    }

    #[test]
    fn decision_ids_are_bounded_and_path_safe() {
        assert!(valid_decision_id("pricing/v1.standard"));
        assert!(valid_decision_id("customer_eligibility"));
        assert!(!valid_decision_id("../secret"));
        assert!(!valid_decision_id("pricing//secret"));
        assert!(!valid_decision_id("pricing/%2e%2e/secret"));
        assert!(!valid_decision_id("pricing\\secret"));
    }

    #[tokio::test]
    async fn evaluates_expression_table_switch_and_function_nodes() {
        let (_temp, config) = workspace();
        write_decision(&config, "expression.json", EXPRESSION_DECISION);
        write_decision(&config, "table.json", TABLE_DECISION);
        write_decision(&config, "switch.json", SWITCH_DECISION);
        write_decision(&config, "function.json", FUNCTION_DECISION);

        assert_evaluation(
            config.clone(),
            "expression",
            json!({"numbers": [1, 2, 10], "firstName": "Max", "lastName": "Lasso"}),
            "/result/fullName",
            json!("Max Lasso"),
        )
        .await;
        assert_evaluation(
            config.clone(),
            "table",
            json!({"input": 12}),
            "/result/output",
            json!(10),
        )
        .await;
        assert_evaluation(
            config.clone(),
            "switch",
            json!({"color": "yellow"}),
            "/result/path1",
            json!(true),
        )
        .await;
        assert_evaluation(
            config,
            "function",
            json!({"input": 12}),
            "/result/output",
            json!(24),
        )
        .await;
    }

    #[tokio::test]
    async fn resolves_nested_decision_nodes_from_the_same_registry() {
        let (_temp, config) = workspace();
        write_decision(&config, "parent.json", NESTED_PARENT_DECISION);
        write_decision(&config, "child.json", NESTED_CHILD_DECISION);
        assert_evaluation(
            config,
            "parent",
            json!({"input": 7}),
            "/result/doubled",
            json!(14),
        )
        .await;
    }

    #[tokio::test]
    async fn arbitrary_precision_numbers_are_not_rounded() {
        let (_temp, config) = workspace();
        write_decision(&config, "precision.json", PRECISION_DECISION);
        let app = build_router(AppState::new(config));
        let (status, body) = request(app, "POST", "/v1/decisions/precision/evaluate", "{}").await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            body.pointer("/result/precise")
                .expect("precise result")
                .to_string(),
            "12345678901234567890.12345679"
        );
    }

    #[tokio::test]
    async fn invalid_reload_preserves_the_last_compiled_registry_and_redacts_source() {
        let (_temp, config) = workspace();
        write_decision(&config, "expression.json", EXPRESSION_DECISION);
        let state = AppState::new(config.clone());
        let app = build_router(state.clone());
        let (before_status, _) = request(
            app.clone(),
            "POST",
            "/v1/decisions/expression/evaluate",
            r#"{"numbers":[1],"firstName":"A","lastName":"B"}"#,
        )
        .await;
        assert_eq!(before_status, StatusCode::OK);

        let sentinel = "DO_NOT_LEAK_DECISION_SOURCE_OR_INPUT";
        write_decision(&config, "broken.json", &format!("{{{sentinel}"));
        let (reload_status, reload_body) =
            request(app.clone(), "POST", "/v1/decisions/reload", "{}").await;
        assert_eq!(reload_status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(!reload_body.to_string().contains(sentinel));
        assert!(
            !reload_body
                .to_string()
                .contains(config.service_root.to_string_lossy().as_ref())
        );

        let (after_status, _) = request(
            app,
            "POST",
            "/v1/decisions/expression/evaluate",
            r#"{"numbers":[1],"firstName":"A","lastName":"B"}"#,
        )
        .await;
        assert_eq!(after_status, StatusCode::OK);
    }

    #[tokio::test]
    async fn invalid_startup_registry_keeps_readiness_unhealthy() {
        let (_temp, config) = workspace();
        write_decision(&config, "broken.json", "{not-json");
        let app = build_router(AppState::new(config));
        let (status, body) = request(app, "GET", "/health/ready", "").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["code"], "registry_invalid");
    }

    #[tokio::test]
    async fn function_nodes_cannot_make_outbound_http_requests() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("test listener");
        let port = listener.local_addr().expect("listener address").port();
        let (_temp, config) = workspace();
        let decision = json!({
            "nodes": [
                {"id": "input", "type": "inputNode", "name": "Request"},
                {
                    "id": "function",
                    "type": "functionNode",
                    "name": "Attempt outbound HTTP",
                    "content": {
                        "source": format!(
                            "import http from 'http'; export const handler = async (input) => {{ await http.get('http://127.0.0.1:{port}/'); return input; }};"
                        )
                    }
                },
                {"id": "output", "type": "outputNode", "name": "Response"}
            ],
            "edges": [
                {"id": "edge-1", "sourceId": "input", "type": "edge", "targetId": "function"},
                {"id": "edge-2", "sourceId": "function", "type": "edge", "targetId": "output"}
            ]
        });
        write_decision(&config, "outbound.json", &decision.to_string());

        let app = build_router(AppState::new(config));
        let (status, body) = request(
            app,
            "POST",
            "/v1/decisions/outbound/evaluate",
            r#"{"sentinel":"must-not-leave-process"}"#,
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert_eq!(body["error"]["code"], "evaluation_failed");
        assert!(
            tokio::time::timeout(Duration::from_millis(200), listener.accept())
                .await
                .is_err(),
            "the ZEN function unexpectedly opened an outbound connection"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_links_are_rejected() {
        use std::os::unix::fs::symlink;

        let (_temp, config) = workspace();
        let outside = config
            .service_root
            .parent()
            .expect("parent")
            .join("outside.json");
        fs::write(&outside, EXPRESSION_DECISION).expect("outside fixture");
        symlink(&outside, config.decisions_dir.join("escape.json")).expect("symlink fixture");
        let error = Registry::load(&config, 1)
            .err()
            .expect("registry must reject symlink");
        assert!(
            error
                .diagnostics
                .iter()
                .any(|item| item.code == "symlink_not_allowed")
        );
        let _ = fs::remove_file(outside);
    }
}
