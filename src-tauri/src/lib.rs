use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;
use axum::{
    extract::{DefaultBodyLimit, Multipart, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};
use chrono::Local;
use qrcode::render::svg;
use qrcode::QrCode;
use rand::{distributions::Alphanumeric, Rng};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;
use tokio::sync::oneshot;
use tower_http::cors::CorsLayer;

const SESSION_DURATION_SECS: u64 = 300; // 5 minutes
const MOBILE_HTML: &str = include_str!("mobile.html");

#[derive(Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub local_ip: String,
    pub port: u16,
    pub token: String,
    pub url: String,
    pub qr_svg: String,
    pub expires_in_seconds: u64,
    pub remaining_seconds: u64,
    pub save_dir: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ReceivedFile {
    pub id: String,
    pub filename: String,
    pub original_name: String,
    pub size_bytes: u64,
    pub size_formatted: String,
    pub received_at: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendFileItem {
    pub name: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendRequestInput {
    pub files: Vec<SendFileItem>,
    pub device_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferRequestEvent {
    pub request_id: String,
    pub files: Vec<SendFileItem>,
    pub total_size: u64,
    pub total_size_formatted: String,
    pub device_name: String,
    pub created_at: String,
}

pub struct AppState {
    pub token: RwLock<String>,
    pub created_at: RwLock<Instant>,
    pub port: u16,
    pub local_ip: String,
    pub save_dir: RwLock<PathBuf>,
    pub received_files: RwLock<Vec<ReceivedFile>>,
    pub app_handle: AppHandle,
    pub pending_transfers: Mutex<HashMap<String, oneshot::Sender<bool>>>,
    pub approved_transfers: Mutex<HashSet<String>>,
}

impl AppState {
    pub fn new(app_handle: AppHandle, port: u16, local_ip: String, save_dir: PathBuf) -> Self {
        let token = generate_random_token();
        Self {
            token: RwLock::new(token),
            created_at: RwLock::new(Instant::now()),
            port,
            local_ip,
            save_dir: RwLock::new(save_dir),
            received_files: RwLock::new(Vec::new()),
            app_handle,
            pending_transfers: Mutex::new(HashMap::new()),
            approved_transfers: Mutex::new(HashSet::new()),
        }
    }

    pub fn get_server_info(&self) -> Result<ServerInfo, String> {
        let token = self.token.read().unwrap().clone();
        let elapsed = self.created_at.read().unwrap().elapsed().as_secs();
        let remaining_seconds = if elapsed >= SESSION_DURATION_SECS {
            0
        } else {
            SESSION_DURATION_SECS - elapsed
        };

        let url = format!("http://{}:{}/?token={}", self.local_ip, self.port, token);
        let qr_svg = generate_qr_svg(&url)?;
        let save_dir = self.save_dir.read().unwrap().to_string_lossy().to_string();

        Ok(ServerInfo {
            local_ip: self.local_ip.clone(),
            port: self.port,
            token,
            url,
            qr_svg,
            expires_in_seconds: SESSION_DURATION_SECS,
            remaining_seconds,
            save_dir,
        })
    }

    pub fn refresh_token(&self) -> Result<ServerInfo, String> {
        let new_token = generate_random_token();
        {
            let mut token_lock = self.token.write().unwrap();
            *token_lock = new_token.clone();
            let mut time_lock = self.created_at.write().unwrap();
            *time_lock = Instant::now();
        }

        let url = format!("http://{}:{}/?token={}", self.local_ip, self.port, new_token);
        let qr_svg = generate_qr_svg(&url)?;
        let save_dir = self.save_dir.read().unwrap().to_string_lossy().to_string();

        let info = ServerInfo {
            local_ip: self.local_ip.clone(),
            port: self.port,
            token: new_token,
            url,
            qr_svg,
            expires_in_seconds: SESSION_DURATION_SECS,
            remaining_seconds: SESSION_DURATION_SECS,
            save_dir,
        };

        // Emit update event to frontend
        let _ = self.app_handle.emit("token-refreshed", &info);

        Ok(info)
    }

    pub fn is_token_valid(&self, query_token: Option<&str>) -> bool {
        let current_token = self.token.read().unwrap();
        let created_at = self.created_at.read().unwrap();

        if let Some(t) = query_token {
            if t == *current_token {
                return created_at.elapsed().as_secs() <= SESSION_DURATION_SECS;
            }
        }
        false
    }

    pub fn respond_transfer(&self, request_id: &str, accept: bool) -> Result<(), String> {
        let mut pending = self.pending_transfers.lock().unwrap();
        if let Some(tx) = pending.remove(request_id) {
            let _ = tx.send(accept);
            Ok(())
        } else {
            Err("Yêu cầu gửi file này không tồn tại hoặc đã hết thời gian chờ".to_string())
        }
    }

    pub fn is_transfer_approved(&self, request_id: &str) -> bool {
        let approved = self.approved_transfers.lock().unwrap();
        approved.contains(request_id)
    }

    pub fn consume_transfer_approval(&self, request_id: &str) {
        let mut approved = self.approved_transfers.lock().unwrap();
        approved.remove(request_id);
    }
}

fn generate_random_token() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect()
}

fn generate_qr_svg(url: &str) -> Result<String, String> {
    let code = QrCode::new(url.as_bytes()).map_err(|e| format!("QR code error: {}", e))?;
    let svg_str = code
        .render::<svg::Color>()
        .min_dimensions(260, 260)
        .build();
    Ok(svg_str)
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

fn sanitize_filename(raw_name: &str) -> String {
    // 1. Extract purely the file name component to prevent path traversal
    let file_name = Path::new(raw_name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unnamed_file");

    // 2. Remove illegal/dangerous characters
    let mut safe: String = file_name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();

    // 3. Prevent leading dots (hidden files on Unix) or empty names
    safe = safe.trim_start_matches('.').trim().to_string();
    if safe.is_empty() {
        safe = "file.bin".to_string();
    }

    // 4. Prepend timestamp YYYYMMDD_HHMMSS_ to prevent overwriting
    let timestamp = Local::now().format("%Y%m%d_%H%M%S_").to_string();
    format!("{}{}", timestamp, safe)
}

#[derive(Deserialize)]
struct AuthQuery {
    token: Option<String>,
}

#[derive(Deserialize)]
struct AuthUploadQuery {
    token: Option<String>,
    request_id: Option<String>,
}

async fn handle_web_root(
    Query(query): Query<AuthQuery>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    if !state.is_token_valid(query.token.as_deref()) {
        let expired_html = r#"<!DOCTYPE html>
<html lang="vi">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Phiên làm việc hết hạn - Vi Drop</title>
  <style>
    body { background: #0f172a; color: #f8fafc; font-family: -apple-system, sans-serif; display: flex; align-items: center; justify-content: center; min-height: 100vh; margin: 0; padding: 20px; text-align: center; }
    .card { background: #1e293b; border: 1px solid #334155; border-radius: 16px; padding: 2rem; max-width: 420px; box-shadow: 0 10px 25px rgba(0,0,0,0.5); }
    h1 { color: #f87171; font-size: 1.4rem; margin-bottom: 0.75rem; }
    p { color: #94a3b8; font-size: 0.95rem; line-height: 1.5; }
    .icon { font-size: 3.5rem; margin-bottom: 1rem; }
    .badge { display: inline-block; background: rgba(239, 68, 68, 0.2); border: 1px solid rgba(239, 68, 68, 0.4); color: #fca5a5; padding: 0.3rem 0.8rem; border-radius: 9999px; font-size: 0.8rem; margin-bottom: 1rem; }
  </style>
</head>
<body>
  <div class="card">
    <div class="icon">⏳</div>
    <div class="badge">Mã QR hết hạn / Không hợp lệ</div>
    <h1>Phiên làm việc không hợp lệ</h1>
    <p>Mã QR hoặc liên kết truy cập này đã quá thời hạn 5 phút hoặc không khớp.</p>
    <p style="margin-top: 1rem; color: #38bdf8; font-weight: 500;">Vui lòng quét lại mã QR mới hiển thị trên màn hình máy tính của bạn.</p>
  </div>
</body>
</html>"#;
        return (StatusCode::FORBIDDEN, Html(expired_html.to_string()));
    }

    (StatusCode::OK, Html(MOBILE_HTML.to_string()))
}

async fn handle_request_send(
    Query(query): Query<AuthQuery>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SendRequestInput>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    if !state.is_token_valid(query.token.as_deref()) {
        return Err((
            StatusCode::FORBIDDEN,
            "Phiên làm việc đã hết hạn hoặc mã token không hợp lệ.".to_string(),
        ));
    }

    if payload.files.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Danh sách file gửi không được để trống.".to_string(),
        ));
    }

    let request_id = generate_random_token()[0..12].to_string();
    let total_bytes: u64 = payload.files.iter().map(|f| f.size).sum();
    let total_size_formatted = format_size(total_bytes);
    let device_name = payload
        .device_name
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "Thiết bị di động".to_string());

    let (tx, rx) = oneshot::channel::<bool>();
    {
        let mut pending = state.pending_transfers.lock().unwrap();
        pending.insert(request_id.clone(), tx);
    }

    // Yêu cầu cửa sổ máy tính hiển thị / tập trung
    if let Some(window) = state.app_handle.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }

    // Gửi thông báo yêu cầu xác nhận tới giao diện desktop
    let event = TransferRequestEvent {
        request_id: request_id.clone(),
        files: payload.files,
        total_size: total_bytes,
        total_size_formatted,
        device_name,
        created_at: Local::now().format("%H:%M:%S").to_string(),
    };

    let _ = state.app_handle.emit("transfer-request", &event);

    // Chờ phản hồi từ người nhận trong tối đa 60 giây
    match tokio::time::timeout(std::time::Duration::from_secs(60), rx).await {
        Ok(Ok(true)) => {
            // Người nhận đã chấp nhận!
            {
                let mut approved = state.approved_transfers.lock().unwrap();
                approved.insert(request_id.clone());
            }
            Ok(Json(serde_json::json!({
                "accepted": true,
                "request_id": request_id,
            })))
        }
        Ok(Ok(false)) => {
            Err((
                StatusCode::FORBIDDEN,
                "Người nhận đã từ chối yêu cầu gửi file.".to_string(),
            ))
        }
        Ok(Err(_)) => {
            Err((
                StatusCode::GONE,
                "Yêu cầu gửi file đã bị hủy.".to_string(),
            ))
        }
        Err(_) => {
            // Hết 60s không ai bấm
            {
                let mut pending = state.pending_transfers.lock().unwrap();
                pending.remove(&request_id);
            }
            let _ = state.app_handle.emit(
                "transfer-request-cancelled",
                serde_json::json!({ "request_id": request_id }),
            );
            Err((
                StatusCode::REQUEST_TIMEOUT,
                "Quá thời gian chờ người nhận phản hồi (60 giây).".to_string(),
            ))
        }
    }
}

async fn handle_upload(
    Query(query): Query<AuthUploadQuery>,
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    if !state.is_token_valid(query.token.as_deref()) {
        return Err((
            StatusCode::FORBIDDEN,
            "Phiên làm việc đã hết hạn hoặc token không hợp lệ. Vui lòng quét lại mã QR trên máy tính.".to_string(),
        ));
    }

    let req_id = query.request_id.as_deref().unwrap_or("");
    if !state.is_transfer_approved(req_id) {
        return Err((
            StatusCode::FORBIDDEN,
            "Yêu cầu gửi file này chưa được người nhận chấp nhận hoặc đã hết hạn.".to_string(),
        ));
    }
    // Tiêu thụ mã chấp thuận
    state.consume_transfer_approval(req_id);

    let mut saved_files = Vec::new();

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Lỗi đọc multipart data: {}", e)))?
    {
        let raw_filename = field
            .file_name()
            .unwrap_or("unnamed_file")
            .to_string();

        let final_filename = sanitize_filename(&raw_filename);
        let destination_path = state.save_dir.read().unwrap().join(&final_filename);

        use tokio::io::AsyncWriteExt;
        let mut out_file = tokio::fs::File::create(&destination_path).await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Không thể tạo file lưu trữ trên đĩa: {}", e),
            )
        })?;

        let mut total_bytes: u64 = 0;
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Lỗi nhận chunk dữ liệu: {}", e)))?
        {
            total_bytes += chunk.len() as u64;
            out_file.write_all(&chunk).await.map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Lỗi ghi dữ liệu vào ổ cứng: {}", e),
                )
            })?;
        }

        out_file.flush().await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Lỗi đồng bộ dữ liệu vào ổ cứng: {}", e),
            )
        })?;

        let received = ReceivedFile {
            id: generate_random_token()[0..8].to_string(),
            filename: final_filename,
            original_name: raw_filename,
            size_bytes: total_bytes,
            size_formatted: format_size(total_bytes),
            received_at: Local::now().format("%H:%M:%S").to_string(),
            path: destination_path.to_string_lossy().to_string(),
        };

        // Record in server state
        state.received_files.write().unwrap().push(received.clone());

        // Emit real-time event to Tauri Desktop window
        let _ = state.app_handle.emit("file-received", &received);

        saved_files.push(received);
    }

    Ok(Json(serde_json::json!({
        "status": "success",
        "files": saved_files
    })))
}

// ----------------- Tauri Commands -----------------

#[tauri::command]
fn get_server_info(state: tauri::State<Arc<AppState>>) -> Result<ServerInfo, String> {
    state.get_server_info()
}

#[tauri::command]
fn refresh_token(state: tauri::State<Arc<AppState>>) -> Result<ServerInfo, String> {
    state.refresh_token()
}

#[tauri::command]
fn get_received_files(state: tauri::State<Arc<AppState>>) -> Vec<ReceivedFile> {
    state.received_files.read().unwrap().clone()
}

#[tauri::command]
fn delete_received_file(id: String, state: tauri::State<Arc<AppState>>) -> Vec<ReceivedFile> {
    let mut files = state.received_files.write().unwrap();
    files.retain(|f| f.id != id);
    files.clone()
}

#[tauri::command]
fn clear_received_files(state: tauri::State<Arc<AppState>>) -> Vec<ReceivedFile> {
    let mut files = state.received_files.write().unwrap();
    files.clear();
    Vec::new()
}

#[tauri::command]
fn respond_transfer_request(
    state: tauri::State<Arc<AppState>>,
    request_id: String,
    accept: bool,
) -> Result<(), String> {
    state.respond_transfer(&request_id, accept)
}

#[tauri::command]
fn open_save_directory(state: tauri::State<Arc<AppState>>) -> Result<(), String> {
    let path = state.save_dir.read().unwrap().to_string_lossy().to_string();
    open_dir_native(&path)
}

#[tauri::command]
async fn select_save_directory(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let default_path = state.save_dir.read().unwrap().clone();
    let folder = app
        .dialog()
        .file()
        .set_directory(&default_path)
        .set_title("Chọn thư mục lưu file nhận từ Vi Drop")
        .blocking_pick_folder();

    if let Some(folder_path) = folder {
        let path = folder_path.as_path().ok_or("Đường dẫn không hợp lệ")?;
        let path_buf = path.to_path_buf();
        let path_str = path_buf.to_string_lossy().to_string();

        {
            let mut save_lock = state.save_dir.write().unwrap();
            *save_lock = path_buf;
        }

        let _ = app.emit("save-directory-changed", serde_json::json!({ "path": &path_str }));

        Ok(Some(path_str))
    } else {
        Ok(None)
    }
}

#[tauri::command]
fn set_save_directory(
    app: tauri::AppHandle,
    state: tauri::State<Arc<AppState>>,
    path: String,
) -> Result<String, String> {
    let path_buf = PathBuf::from(&path);
    if !path_buf.exists() {
        std::fs::create_dir_all(&path_buf).map_err(|e| format!("Không thể tạo thư mục: {}", e))?;
    }
    let canonical = path_buf.canonicalize().unwrap_or(path_buf);
    let path_str = canonical.to_string_lossy().to_string();
    {
        let mut save_lock = state.save_dir.write().unwrap();
        *save_lock = canonical;
    }
    let _ = app.emit("save-directory-changed", serde_json::json!({ "path": &path_str }));
    Ok(path_str)
}

fn find_received_path(state: &AppState, id: &str) -> Result<PathBuf, String> {
    let files = state.received_files.read().unwrap();
    let file = files
        .iter()
        .find(|f| f.id == id)
        .ok_or_else(|| "Không tìm thấy file trong danh sách".to_string())?;
    let path = PathBuf::from(&file.path);
    if !path.exists() {
        return Err("File không còn tồn tại trên ổ đĩa (có thể đã bị xóa hoặc di chuyển)".to_string());
    }
    Ok(path)
}

/// Mở file bằng ứng dụng mặc định của hệ điều hành (ảnh -> trình xem ảnh, pdf -> trình đọc pdf...)
#[tauri::command]
fn open_received_file(id: String, state: tauri::State<Arc<AppState>>) -> Result<(), String> {
    let path = find_received_path(&state, &id)?;
    open_with_default_app(&path)
}

/// Mở thư mục chứa file và bôi đen (chọn) file đó
#[tauri::command]
fn reveal_received_file(id: String, state: tauri::State<Arc<AppState>>) -> Result<(), String> {
    let path = find_received_path(&state, &id)?;
    reveal_in_folder(&path)
}

#[derive(Clone, Serialize, Deserialize)]
pub struct UpdateMetadata {
    pub available: bool,
    pub current_version: String,
    pub version: Option<String>,
    pub date: Option<String>,
    pub body: Option<String>,
}

#[tauri::command]
async fn check_update(app: tauri::AppHandle) -> Result<UpdateMetadata, String> {
    let current_version = app.package_info().version.to_string();
    let updater = match app.updater() {
        Ok(u) => u,
        Err(_) => {
            return Ok(UpdateMetadata {
                available: false,
                current_version,
                version: None,
                date: None,
                body: None,
            })
        }
    };

    match updater.check().await {
        Ok(Some(update)) => Ok(UpdateMetadata {
            available: true,
            current_version,
            version: Some(update.version.clone()),
            date: update.date.map(|d| d.to_string()),
            body: update.body.clone(),
        }),
        Ok(None) => Ok(UpdateMetadata {
            available: false,
            current_version,
            version: None,
            date: None,
            body: None,
        }),
        Err(e) => Err(format!("Lỗi kiểm tra cập nhật: {}", e)),
    }
}

#[tauri::command]
async fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    if let Some(update) = updater.check().await.map_err(|e| e.to_string())? {
        let mut downloaded_bytes: u64 = 0;
        let app_handle = app.clone();
        update
            .download_and_install(
                move |chunk_length, content_length| {
                    downloaded_bytes += chunk_length as u64;
                    let _ = app_handle.emit(
                        "update-download-progress",
                        serde_json::json!({
                            "chunk": chunk_length,
                            "downloaded": downloaded_bytes,
                            "total": content_length,
                        }),
                    );
                },
                || {},
            )
            .await
            .map_err(|e| format!("Lỗi cài đặt bản cập nhật: {}", e))?;

        app.restart();
    } else {
        Err("Không có bản cập nhật nào khả dụng".to_string())
    }
}

fn open_dir_native(path: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("Lỗi mở thư mục: {}", e))?;
    }
    #[cfg(target_os = "windows")]
    {
        let win_path = path.replace('/', "\\");
        std::process::Command::new("explorer")
            .arg(&win_path)
            .spawn()
            .map_err(|e| format!("Lỗi mở thư mục: {}", e))?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("Lỗi mở thư mục: {}", e))?;
    }
    Ok(())
}

fn open_with_default_app(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("Lỗi mở file: {}", e))?;
    }
    #[cfg(target_os = "windows")]
    {
        // explorer.exe <đường_dẫn_file> mở file bằng ứng dụng mặc định (giống double-click).
        // Đường dẫn được truyền như MỘT đối số riêng nên Rust tự bọc ngoặc kép đúng chuẩn
        // khi có dấu cách / ký tự Unicode.
        std::process::Command::new("explorer.exe")
            .arg(path.as_os_str())
            .spawn()
            .map_err(|e| format!("Lỗi mở file: {}", e))?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("Lỗi mở file: {}", e))?;
    }
    Ok(())
}

fn reveal_in_folder(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn()
            .map_err(|e| format!("Lỗi định vị file: {}", e))?;
    }
    #[cfg(target_os = "windows")]
    {
        // Explorer yêu cầu đúng cú pháp: /select,"C:\đường dẫn\tên file.ext"
        // Không được dùng .arg() vì Rust sẽ bọc cả chuỗi "/select,..." trong ngoặc kép
        // khi đường dẫn có dấu cách -> Explorer không hiểu và mở thư mục mặc định.
        // raw_arg truyền nguyên văn chuỗi. Tên file Windows không thể chứa ký tự `"`.
        use std::os::windows::process::CommandExt;
        let win_path = path.to_string_lossy().replace('/', "\\");
        std::process::Command::new("explorer.exe")
            .raw_arg(format!("/select,\"{}\"", win_path))
            .spawn()
            .map_err(|e| format!("Lỗi định vị file: {}", e))?;
    }
    #[cfg(target_os = "linux")]
    {
        if let Some(parent) = path.parent() {
            std::process::Command::new("xdg-open")
                .arg(parent)
                .spawn()
                .map_err(|e| format!("Lỗi định vị file: {}", e))?;
        }
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Check for updates in background after 3 seconds
            let handle_for_bg_update = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                if let Ok(updater) = handle_for_bg_update.updater() {
                    if let Ok(Some(update)) = updater.check().await {
                        let _ = handle_for_bg_update.emit(
                            "update-available",
                            serde_json::json!({
                                "version": update.version,
                                "body": update.body,
                                "date": update.date.map(|d| d.to_string()),
                            }),
                        );
                    }
                }
            });

            // Determine save directory: Downloads/ViDrop
            let save_dir = dirs::download_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("ViDrop");
            if !save_dir.exists() {
                let _ = std::fs::create_dir_all(&save_dir);
            }

            // Determine LAN IP address
            let local_ip = local_ip_address::local_ip()
                .map(|ip| ip.to_string())
                .unwrap_or_else(|_| "127.0.0.1".to_string());

            // Bind to fixed port 8787 or find an open port
            let (std_listener, port) = match std::net::TcpListener::bind("0.0.0.0:8787") {
                Ok(l) => (l, 8787),
                Err(_) => {
                    let l = std::net::TcpListener::bind("0.0.0.0:0")
                        .expect("Không thể bind cổng mạng nào");
                    let p = l.local_addr().unwrap().port();
                    (l, p)
                }
            };
            std_listener.set_nonblocking(true).expect("set_nonblocking failed");

            let app_state = Arc::new(AppState::new(
                app.handle().clone(),
                port,
                local_ip,
                save_dir,
            ));

            app.manage(app_state.clone());

            // Launch Axum HTTP server in background task
            let state_for_server = app_state.clone();
            tauri::async_runtime::spawn(async move {
                let app_routes = Router::new()
                    .route("/", get(handle_web_root))
                    .route("/request-send", post(handle_request_send))
                    .route(
                        "/upload",
                        post(handle_upload).layer(DefaultBodyLimit::disable()),
                    )
                    .layer(CorsLayer::permissive())
                    .with_state(state_for_server);

                let tokio_listener = tokio::net::TcpListener::from_std(std_listener)
                    .expect("Failed to convert TcpListener to tokio");

                if let Err(e) = axum::serve(tokio_listener, app_routes).await {
                    eprintln!("Axum server error: {}", e);
                }
            });

            // Tự động tạo lại mã QR / token mới khi hết hạn 5 phút
            let state_for_rotate = app_state.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    let elapsed = state_for_rotate.created_at.read().unwrap().elapsed().as_secs();
                    if elapsed >= SESSION_DURATION_SECS {
                        let _ = state_for_rotate.refresh_token();
                    }
                }
            });

            // Tự động kiểm tra bản cập nhật sau 3 giây khi khởi động
            let handle_for_updater = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                if let Ok(updater) = handle_for_updater.updater() {
                    if let Ok(Some(update)) = updater.check().await {
                        let _ = handle_for_updater.emit(
                            "update-available",
                            serde_json::json!({
                                "version": update.version,
                                "date": update.date.map(|d| d.to_string()),
                                "body": update.body,
                            }),
                        );
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_server_info,
            refresh_token,
            get_received_files,
            delete_received_file,
            clear_received_files,
            respond_transfer_request,
            open_save_directory,
            select_save_directory,
            set_save_directory,
            open_received_file,
            reveal_received_file,
            check_update,
            install_update
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename_traversal() {
        let input = "../../../etc/passwd";
        let sanitized = sanitize_filename(input);
        assert!(!sanitized.contains('/'));
        assert!(!sanitized.contains('\\'));
        assert!(sanitized.ends_with("_passwd"));
    }

    #[test]
    fn test_sanitize_filename_special_chars() {
        let input = "video:test*file?1.mp4";
        let sanitized = sanitize_filename(input);
        assert!(!sanitized.contains(':'));
        assert!(!sanitized.contains('*'));
        assert!(!sanitized.contains('?'));
        assert!(sanitized.ends_with("_video_test_file_1.mp4"));
    }

    #[test]
    fn test_sanitize_filename_empty() {
        let input = "...";
        let sanitized = sanitize_filename(input);
        assert!(sanitized.ends_with("_file.bin"));
    }

    #[test]
    fn test_qr_code_svg_generation() {
        let url = "http://192.168.1.100:8787/?token=abcdefghijklmnopqrstuvwxyz123456";
        let svg = generate_qr_svg(url).expect("QR generation failed");
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1024 * 1024 * 15), "15.0 MB");
        assert_eq!(format_size(1024 * 1024 * 1024 * 2), "2.00 GB");
    }

    #[test]
    fn test_token_randomness() {
        let t1 = generate_random_token();
        let t2 = generate_random_token();
        assert_eq!(t1.len(), 32);
        assert_eq!(t2.len(), 32);
        assert_ne!(t1, t2);
    }

    #[test]
    fn test_transfer_approval_channel() {
        let (tx, rx) = tokio::sync::oneshot::channel::<bool>();
        let mut pending = std::collections::HashMap::new();
        let req_id = "test_req_123".to_string();
        pending.insert(req_id.clone(), tx);

        let sender = pending.remove(&req_id).unwrap();
        sender.send(true).unwrap();

        let approved = rx.blocking_recv().unwrap();
        assert!(approved);
    }

    #[test]
    fn test_save_dir_rwlock() {
        let initial = PathBuf::from("/tmp/vidrop_initial");
        let dir_lock = RwLock::new(initial);
        assert_eq!(*dir_lock.read().unwrap(), PathBuf::from("/tmp/vidrop_initial"));

        let new_dir = PathBuf::from("/tmp/vidrop_custom");
        *dir_lock.write().unwrap() = new_dir.clone();
        assert_eq!(*dir_lock.read().unwrap(), new_dir);
    }
}

