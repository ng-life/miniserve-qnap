use axum::{
    Json, Router,
    extract::{Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware,
    middleware::Next,
    response::{Html, IntoResponse, Response},
    routing::{get, post, put},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    env,
    fs::File,
    io::Read,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use subtle::ConstantTimeEq;
use tokio::{
    fs,
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    sync::{Mutex, RwLock},
};

const INDEX_HTML: &str = include_str!("../shared/index.html");
const MAX_LOG_LINES: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct Config {
    share_dir: String,
    listen_address: String,
    port: u16,
    title: String,
    route_prefix: String,
    upload: bool,
    mkdir: bool,
    hidden: bool,
    follow_symlinks: bool,
    username: String,
    password: String,
    color_scheme: String,
    sorting_method: String,
    sorting_order: String,
    index_file: String,
    pretty_urls: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            share_dir: "/share/Public".into(),
            listen_address: "0.0.0.0".into(),
            port: 8080,
            title: "Miniserve 文件共享".into(),
            route_prefix: String::new(),
            upload: false,
            mkdir: false,
            hidden: false,
            follow_symlinks: false,
            username: String::new(),
            password: String::new(),
            color_scheme: "squirrel".into(),
            sorting_method: "name".into(),
            sorting_order: "asc".into(),
            index_file: String::new(),
            pretty_urls: false,
        }
    }
}

#[derive(Clone)]
struct AppState {
    inner: Arc<Inner>,
}

struct Inner {
    config_path: PathBuf,
    admin_auth_path: PathBuf,
    miniserve_path: PathBuf,
    child: Mutex<Option<Child>>,
    logs: RwLock<VecDeque<String>>,
    started_at: AtomicU64,
}

#[derive(Serialize)]
struct StatusResponse {
    running: bool,
    pid: Option<u32>,
    started_at: u64,
    miniserve_version: &'static str,
    password_set: bool,
    service_url: String,
    config: Config,
    logs: Vec<String>,
}

#[derive(Serialize)]
struct ApiMessage {
    ok: bool,
    message: String,
}

type ApiError = (StatusCode, Json<ApiMessage>);

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    let config_path = arg_value(&args, "--config").unwrap_or_else(|| "./config.json".into());
    let admin_auth_path =
        arg_value(&args, "--admin-auth-file").unwrap_or_else(|| "./admin-auth.txt".into());
    let miniserve_path = arg_value(&args, "--miniserve").unwrap_or_else(|| "./miniserve".into());
    let listen = arg_value(&args, "--listen").unwrap_or_else(|| "0.0.0.0:8090".into());

    let state = AppState {
        inner: Arc::new(Inner {
            config_path: PathBuf::from(config_path),
            admin_auth_path: PathBuf::from(admin_auth_path),
            miniserve_path: PathBuf::from(miniserve_path),
            child: Mutex::new(None),
            logs: RwLock::new(VecDeque::new()),
            started_at: AtomicU64::new(0),
        }),
    };

    if let Err(error) = ensure_config(&state).await {
        eprintln!("cannot initialize configuration: {error}");
        std::process::exit(1);
    }
    if let Err(error) = ensure_admin_auth(&state).await {
        eprintln!("cannot initialize management authentication: {error}");
        std::process::exit(1);
    }
    if let Err(error) = restart_miniserve(&state).await {
        push_log(&state, format!("ERROR miniserve 启动失败：{error}")).await;
    }

    let protected = Router::new()
        .route("/", get(index))
        .route("/favicon.ico", get(favicon))
        .route("/api/status", get(status))
        .route("/api/config", put(update_config))
        .route("/api/restart", post(restart))
        .fallback(not_found)
        .route_layer(middleware::from_fn_with_state(state.clone(), require_admin));
    let app = Router::new()
        .route("/healthz", get(health))
        .merge(protected)
        .with_state(state.clone());

    let address: SocketAddr = listen.parse().unwrap_or_else(|error| {
        eprintln!("invalid --listen address {listen}: {error}");
        std::process::exit(2);
    });
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .unwrap_or_else(|error| {
            eprintln!("cannot bind management server to {address}: {error}");
            std::process::exit(1);
        });
    println!("Miniserve QNAP manager listening on http://{address}");

    if let Err(error) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(state.clone()))
        .await
    {
        eprintln!("management server error: {error}");
    }
}

async fn require_admin(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let authorized = match request.headers().get(header::AUTHORIZATION) {
        Some(value) => verify_basic_auth(&state.inner.admin_auth_path, value).await,
        None => false,
    };
    if authorized {
        return next.run(request).await;
    }
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Basic realm=\"Miniserve QNAP\"")],
        "Authentication required",
    )
        .into_response()
}

async fn verify_basic_auth(path: &Path, value: &HeaderValue) -> bool {
    let Ok(header_value) = value.to_str() else {
        return false;
    };
    let Some(encoded) = header_value.strip_prefix("Basic ") else {
        return false;
    };
    let Ok(decoded) = STANDARD.decode(encoded) else {
        return false;
    };
    let Ok(received) = std::str::from_utf8(&decoded) else {
        return false;
    };
    let Ok(expected) = fs::read_to_string(path).await else {
        return false;
    };
    constant_time_eq(received.as_bytes(), expected.trim_end().as_bytes())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len() && bool::from(left.ct_eq(right))
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let mut child_guard = state.inner.child.lock().await;
    let healthy = matches!(child_guard.as_mut().map(Child::try_wait), Some(Ok(None)));
    if healthy {
        (StatusCode::OK, "ok")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "miniserve unavailable")
    }
}

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|item| item == name)
        .and_then(|position| args.get(position + 1))
        .cloned()
}

async fn index() -> impl IntoResponse {
    (
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        Html(INDEX_HTML),
    )
}

async fn not_found() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, "Not found")
}

async fn favicon() -> impl IntoResponse {
    StatusCode::NO_CONTENT
}

async fn status(State(state): State<AppState>) -> Result<Json<StatusResponse>, ApiError> {
    status_payload(&state)
        .await
        .map(Json)
        .map_err(internal_error)
}

async fn update_config(
    State(state): State<AppState>,
    Json(mut new_config): Json<Config>,
) -> Result<Json<StatusResponse>, ApiError> {
    let old_config = load_config(&state.inner.config_path)
        .await
        .map_err(internal_error)?;
    if new_config.username.is_empty() {
        new_config.password.clear();
    } else if new_config.password.is_empty() {
        new_config.password = old_config.password;
    }
    validate_config(&new_config).map_err(bad_request)?;
    save_config(&state.inner.config_path, &new_config)
        .await
        .map_err(internal_error)?;
    restart_miniserve(&state).await.map_err(internal_error)?;
    push_log(&state, "INFO 配置已保存，服务已重启".into()).await;
    status_payload(&state)
        .await
        .map(Json)
        .map_err(internal_error)
}

async fn restart(State(state): State<AppState>) -> Result<Json<StatusResponse>, ApiError> {
    restart_miniserve(&state).await.map_err(internal_error)?;
    status_payload(&state)
        .await
        .map(Json)
        .map_err(internal_error)
}

async fn status_payload(state: &AppState) -> Result<StatusResponse, String> {
    let config = load_config(&state.inner.config_path).await?;
    let (running, pid) = {
        let mut child_guard = state.inner.child.lock().await;
        if let Some(child) = child_guard.as_mut() {
            match child.try_wait() {
                Ok(None) => (true, child.id()),
                Ok(Some(exit)) => {
                    push_log(state, format!("ERROR miniserve 已退出：{exit}")).await;
                    *child_guard = None;
                    (false, None)
                }
                Err(error) => return Err(format!("cannot inspect miniserve process: {error}")),
            }
        } else {
            (false, None)
        }
    };
    let mut public_config = config.clone();
    public_config.password.clear();
    let service_url = format!(
        "http://{}:{}{}",
        display_host(&config.listen_address),
        config.port,
        config.route_prefix
    );
    let logs = state.inner.logs.read().await.iter().cloned().collect();
    Ok(StatusResponse {
        running,
        pid,
        started_at: state.inner.started_at.load(Ordering::Relaxed),
        miniserve_version: "0.35.0",
        password_set: !config.password.is_empty(),
        service_url,
        config: public_config,
        logs,
    })
}

fn display_host(interface: &str) -> &str {
    match interface {
        "0.0.0.0" => "NAS-IP",
        "::" => "[NAS-IP]",
        other => other,
    }
}

async fn ensure_config(state: &AppState) -> Result<(), String> {
    if state.inner.config_path.exists() {
        let config = load_config(&state.inner.config_path).await?;
        validate_config(&config)?;
        return Ok(());
    }
    if let Some(parent) = state.inner.config_path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|error| format!("cannot create configuration directory: {error}"))?;
    }
    save_config(&state.inner.config_path, &Config::default()).await
}

async fn ensure_admin_auth(state: &AppState) -> Result<(), String> {
    let path = &state.inner.admin_auth_path;
    if path.exists() {
        let value = fs::read_to_string(path)
            .await
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        validate_admin_auth(value.trim_end())?;
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|error| format!("cannot create authentication directory: {error}"))?;
    }
    let credentials = format!("admin:{}\n", random_password()?);
    write_private(path, credentials.as_bytes()).await?;
    eprintln!(
        "Management password generated. Read the admin credentials from {}",
        path.display()
    );
    Ok(())
}

fn random_password() -> Result<String, String> {
    let mut bytes = [0_u8; 18];
    File::open("/dev/urandom")
        .and_then(|mut source| source.read_exact(&mut bytes))
        .map_err(|error| format!("cannot read secure random bytes: {error}"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn validate_admin_auth(value: &str) -> Result<(), String> {
    let Some((username, password)) = value.split_once(':') else {
        return Err("management auth file must contain username:password".into());
    };
    if username.is_empty() || password.len() < 16 {
        return Err(
            "management username must be set and password must have at least 16 characters".into(),
        );
    }
    if value.contains(['\n', '\r']) {
        return Err("management credentials must be on one line".into());
    }
    Ok(())
}

async fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, bytes)
        .await
        .map_err(|error| format!("cannot write {}: {error}", temporary.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o600))
            .await
            .map_err(|error| format!("cannot protect {}: {error}", temporary.display()))?;
    }
    fs::rename(&temporary, path)
        .await
        .map_err(|error| format!("cannot replace {}: {error}", path.display()))
}

async fn load_config(path: &Path) -> Result<Config, String> {
    let bytes = fs::read(path)
        .await
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("cannot parse {}: {error}", path.display()))
}

async fn save_config(path: &Path, config: &Config) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(config)
        .map_err(|error| format!("cannot serialize configuration: {error}"))?;
    write_private(path, &bytes).await
}

fn validate_config(config: &Config) -> Result<(), String> {
    let path = Path::new(&config.share_dir);
    if !path.is_absolute() {
        return Err("共享目录必须是绝对路径".into());
    }
    if !path.is_dir() {
        return Err(format!("共享目录不存在或不是目录：{}", config.share_dir));
    }
    config
        .listen_address
        .parse::<IpAddr>()
        .map_err(|_| "监听地址必须是有效的 IPv4 或 IPv6 地址".to_string())?;
    if config.port == 0 || config.port == 8090 {
        return Err("服务端口必须在 1-65535 之间，且不能使用管理端口 8090".into());
    }
    if !config.route_prefix.is_empty()
        && (!config.route_prefix.starts_with('/')
            || config.route_prefix.contains(char::is_whitespace))
    {
        return Err("路由前缀必须为空，或是以 / 开头且不含空格的路径".into());
    }
    if config.mkdir && !config.upload {
        return Err("允许创建目录时必须同时允许上传".into());
    }
    if config.username.is_empty() != config.password.is_empty() {
        return Err("用户名和密码必须同时填写，或同时留空".into());
    }
    if config.username.contains([':', '\n', '\r']) || config.password.contains(['\n', '\r']) {
        return Err("用户名或密码包含不支持的字符".into());
    }
    if !["squirrel", "archlinux", "ayu-dark", "zenburn", "monokai"]
        .contains(&config.color_scheme.as_str())
    {
        return Err("未知的主题风格".into());
    }
    if !["name", "size", "date"].contains(&config.sorting_method.as_str()) {
        return Err("未知的排序字段".into());
    }
    if !["asc", "desc"].contains(&config.sorting_order.as_str()) {
        return Err("未知的排序方向".into());
    }
    if config.index_file.contains(['/', '\\', '\n', '\r']) {
        return Err("索引文件只能填写文件名，不能包含目录".into());
    }
    Ok(())
}

async fn restart_miniserve(state: &AppState) -> Result<(), String> {
    let config = load_config(&state.inner.config_path).await?;
    validate_config(&config)?;

    let mut child_guard = state.inner.child.lock().await;
    if let Some(child) = child_guard.as_mut() {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
    *child_guard = None;

    let auth_file = state.inner.config_path.with_file_name("auth.txt");
    if config.username.is_empty() {
        let _ = fs::remove_file(&auth_file).await;
    } else {
        fs::write(
            &auth_file,
            format!("{}:{}\n", config.username, config.password),
        )
        .await
        .map_err(|error| format!("cannot write authentication file: {error}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&auth_file, std::fs::Permissions::from_mode(0o600))
                .await
                .map_err(|error| format!("cannot protect authentication file: {error}"))?;
        }
    }

    let mut command = Command::new(&state.inner.miniserve_path);
    command.kill_on_drop(true);
    command
        .arg("--verbose")
        .arg("--interfaces")
        .arg(&config.listen_address)
        .arg("--port")
        .arg(config.port.to_string())
        .arg("--title")
        .arg(&config.title)
        .arg("--color-scheme")
        .arg(&config.color_scheme)
        .arg("--default-sorting-method")
        .arg(&config.sorting_method)
        .arg("--default-sorting-order")
        .arg(&config.sorting_order)
        .arg("--enable-zip")
        .arg("--enable-tar-gz")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if !config.route_prefix.is_empty() {
        command.arg("--route-prefix").arg(&config.route_prefix);
    }
    if config.upload {
        command.arg("--upload-files");
    }
    if config.mkdir {
        command.arg("--mkdir");
    }
    if config.hidden {
        command.arg("--hidden");
    }
    if !config.follow_symlinks {
        command.arg("--no-symlinks");
    }
    if !config.username.is_empty() {
        command.arg("--auth-file").arg(&auth_file);
    }
    if !config.index_file.is_empty() {
        command.arg("--index").arg(&config.index_file);
    }
    if config.pretty_urls {
        command.arg("--pretty-urls");
    }
    command.arg("--").arg(&config.share_dir);

    let mut child = command.spawn().map_err(|error| {
        format!(
            "cannot start {}: {error}",
            state.inner.miniserve_path.display()
        )
    })?;
    let pid = child.id().unwrap_or_default();
    if let Some(stdout) = child.stdout.take() {
        collect_output(state.clone(), stdout, "INFO");
    }
    if let Some(stderr) = child.stderr.take() {
        collect_output(state.clone(), stderr, "LOG");
    }
    *child_guard = Some(child);
    drop(child_guard);
    tokio::time::sleep(Duration::from_millis(350)).await;
    let mut child_guard = state.inner.child.lock().await;
    if let Some(child) = child_guard.as_mut() {
        match child.try_wait() {
            Ok(None) => {}
            Ok(Some(exit)) => {
                *child_guard = None;
                return Err(format!("miniserve exited during startup: {exit}"));
            }
            Err(error) => return Err(format!("cannot inspect miniserve startup: {error}")),
        }
    } else {
        return Err("miniserve process disappeared during startup".into());
    }
    drop(child_guard);
    state.inner.started_at.store(now(), Ordering::Relaxed);
    push_log(
        state,
        format!(
            "INFO miniserve 已启动，PID {pid}，监听 {}:{}，共享 {}",
            config.listen_address, config.port, config.share_dir
        ),
    )
    .await;
    Ok(())
}

fn collect_output<R>(state: AppState, stream: R, level: &'static str)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(stream).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            push_log(&state, format!("{level} {line}")).await;
        }
    });
}

async fn push_log(state: &AppState, line: String) {
    let mut logs = state.inner.logs.write().await;
    logs.push_front(format!("{} {line}", now()));
    logs.truncate(MAX_LOG_LINES);
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

async fn shutdown_signal(state: AppState) {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("install SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = terminate.recv() => {},
        }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
    let mut child_guard = state.inner.child.lock().await;
    if let Some(child) = child_guard.as_mut() {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
}

fn bad_request(message: String) -> ApiError {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiMessage { ok: false, message }),
    )
}

fn internal_error(message: String) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiMessage { ok: false, message }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_configuration_is_safe() {
        let config = Config {
            share_dir: env::temp_dir().display().to_string(),
            ..Config::default()
        };
        assert!(validate_config(&config).is_ok());
        assert!(!config.upload);
        assert!(!config.follow_symlinks);
    }

    #[test]
    fn mkdir_requires_upload() {
        let config = Config {
            share_dir: env::temp_dir().display().to_string(),
            mkdir: true,
            ..Config::default()
        };
        assert_eq!(
            validate_config(&config).unwrap_err(),
            "允许创建目录时必须同时允许上传"
        );
    }

    #[test]
    fn management_port_is_reserved() {
        let config = Config {
            share_dir: env::temp_dir().display().to_string(),
            port: 8090,
            ..Config::default()
        };
        assert!(validate_config(&config).unwrap_err().contains("管理端口"));
    }

    #[test]
    fn constant_time_credentials_require_exact_match() {
        assert!(constant_time_eq(
            b"admin:0123456789abcdef",
            b"admin:0123456789abcdef"
        ));
        assert!(!constant_time_eq(b"admin:wrong", b"admin:0123456789abcdef"));
    }

    #[test]
    fn management_password_is_long_and_random() {
        let first = random_password().unwrap();
        let second = random_password().unwrap();
        assert_eq!(first.len(), 36);
        assert_ne!(first, second);
    }
}
