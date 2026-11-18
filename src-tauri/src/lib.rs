use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path as AxumPath, Query, State},
    http::{header::{CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE}, StatusCode},
    response::{Html, IntoResponse, Json, Response},
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

#[derive(Clone, Serialize, Deserialize)]
pub struct ConnectedPeer {
    pub session_id: String,
    pub client_name: String,
    pub device_info: String,
    pub connected_at: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SharedFileForMobile {
    pub id: String,
    pub filename: String,
    pub path: String,
    pub size_bytes: u64,
    pub size_formatted: String,
    pub added_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairRequestInput {
    pub client_name: String,
    pub device_info: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairRequestEvent {
    pub request_id: String,
    pub client_name: String,
    pub device_info: String,
    pub requested_at: String,
}

pub struct AppState {
    pub token: RwLock<String>,
    pub port: u16,
    pub local_ip: String,
    pub save_dir: RwLock<PathBuf>,
    pub received_files: RwLock<Vec<ReceivedFile>>,
    pub app_handle: AppHandle,
    pub active_peer: RwLock<Option<ConnectedPeer>>,
    pub pending_pair: Mutex<Option<(String, String, oneshot::Sender<bool>)>>, // (request_id, client_name, tx)
    pub shared_to_mobile: RwLock<Vec<SharedFileForMobile>>,
}

impl AppState {
    pub fn new(app_handle: AppHandle, port: u16, local_ip: String, save_dir: PathBuf) -> Self {
        let token = generate_random_token();
        Self {
            token: RwLock::new(token),
            port,
            local_ip,
            save_dir: RwLock::new(save_dir),
            received_files: RwLock::new(Vec::new()),
            app_handle,
            active_peer: RwLock::new(None),
            pending_pair: Mutex::new(None),
            shared_to_mobile: RwLock::new(Vec::new()),
        }
    }

    pub fn get_server_info(&self) -> Result<ServerInfo, String> {
        let token = self.token.read().unwrap().clone();
        let url = format!("http://{}:{}/?token={}", self.local_ip, self.port, token);
        let qr_svg = generate_qr_svg(&url)?;
        let save_dir = self.save_dir.read().unwrap().to_string_lossy().to_string();

        Ok(ServerInfo {
            local_ip: self.local_ip.clone(),
            port: self.port,
            token,
            url,
            qr_svg,
            expires_in_seconds: 0,
            remaining_seconds: 0,
            save_dir,
        })
    }

    pub fn refresh_token(&self) -> Result<ServerInfo, String> {
        // Ngắt kết nối và reset toàn bộ trạng thái phiên hiện tại khi tạo token mới
        self.disconnect_active_peer();

        let new_token = generate_random_token();
        {
            let mut token_lock = self.token.write().unwrap();
            *token_lock = new_token.clone();
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
            expires_in_seconds: 0,
            remaining_seconds: 0,
            save_dir,
        };

        // Emit update event to frontend
        let _ = self.app_handle.emit("token-refreshed", &info);

        Ok(info)
    }

    pub fn is_token_valid(&self, query_token: Option<&str>) -> bool {
        let current_token = self.token.read().unwrap();
        if let Some(t) = query_token {
            return t == *current_token;
        }
        false
    }

    pub fn is_active_session(&self, session_id: &str) -> bool {
        if session_id.is_empty() {
            return false;
        }
        if let Some(peer) = self.active_peer.read().unwrap().as_ref() {
            peer.session_id == session_id
        } else {
            false
        }
    }

    pub fn disconnect_active_peer(&self) {
        let old_peer = {
            let mut active = self.active_peer.write().unwrap();
            active.take()
        };
        // Reset toàn bộ trạng thái hiện có trên 2 thiết bị:
        // 1. Xóa danh sách file máy tính đang chia sẻ cho điện thoại
        {
            let mut shared = self.shared_to_mobile.write().unwrap();
            shared.clear();
        }
        // 2. Xóa danh sách file nhận trong phiên trên máy tính
        {
            let mut received = self.received_files.write().unwrap();
            received.clear();
        }
        // Phát sự kiện để cập nhật giao diện desktop về trạng thái ban đầu
        let _ = self.app_handle.emit("shared-files-updated", &Vec::<SharedFileForMobile>::new());
        let _ = self.app_handle.emit("received-files-reset", ());
        let _ = self.app_handle.emit("peer-disconnected", &old_peer);
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

fn percent_encode(s: &str) -> String {
    let mut encoded = String::new();
    for byte in s.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    encoded
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

fn current_date_folder_name() -> String {
    Local::now().format("%d-%m-%Y").to_string()
}

#[derive(Deserialize)]
struct AuthQuery {
    token: Option<String>,
}

#[derive(Deserialize)]
struct AuthSessionQuery {
    token: Option<String>,
    session_id: Option<String>,
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
  <title>Mã QR không hợp lệ - Vi Drop</title>
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
    <div class="icon">❌</div>
    <div class="badge">Mã QR không khớp</div>
    <h1>Liên kết không hợp lệ</h1>
    <p>Mã QR hoặc liên kết truy cập này không khớp với mã hiện tại trên màn hình máy tính.</p>
    <p style="margin-top: 1rem; color: #38bdf8; font-weight: 500;">Vui lòng quét lại mã QR mới hiển thị trên ứng dụng Vi Drop máy tính.</p>
  </div>
</body>
</html>"#;
        return (StatusCode::FORBIDDEN, Html(expired_html.to_string()));
    }

    (StatusCode::OK, Html(MOBILE_HTML.to_string()))
}

async fn handle_check_peer(
    Query(query): Query<AuthQuery>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let is_valid = state.is_token_valid(query.token.as_deref());
    if !is_valid {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "valid": false,
                "busy": false,
                "message": "Mã QR đã hết hạn hoặc không hợp lệ"
            })),
        );
    }

    let active = state.active_peer.read().unwrap();
    if let Some(peer) = active.as_ref() {
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "valid": true,
                "busy": true,
                "active_peer_name": peer.client_name,
                "device_info": peer.device_info
            })),
        )
    } else {
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "valid": true,
                "busy": false,
                "active_peer_name": null
            })),
        )
    }
}

async fn handle_pair_request(
    Query(query): Query<AuthQuery>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<PairRequestInput>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    if !state.is_token_valid(query.token.as_deref()) {
        return Err((
            StatusCode::FORBIDDEN,
            "Mã QR hoặc liên kết không hợp lệ. Vui lòng quét lại mã hiển thị trên máy tính.".to_string(),
        ));
    }

    let client_name = payload.client_name.trim().to_string();
    if client_name.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Vui lòng nhập tên người dùng để kết nối.".to_string(),
        ));
    }

    // Kiểm tra máy tính có đang bận với thiết bị khác không (Strict 1-to-1)
    {
        let active = state.active_peer.read().unwrap();
        if let Some(current_peer) = active.as_ref() {
            return Err((
                StatusCode::CONFLICT,
                format!(
                    "Máy tính đang bận kết nối với '{}'. Vui lòng đợi đến khi phiên kết thúc.",
                    current_peer.client_name
                ),
            ));
        }
    }

    let device_info = payload
        .device_info
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "Thiết bị di động".to_string());

    let request_id = generate_random_token()[0..12].to_string();
    let (tx, rx) = oneshot::channel::<bool>();

    {
        let mut pending = state.pending_pair.lock().unwrap();
        *pending = Some((request_id.clone(), client_name.clone(), tx));
    }

    // Focus cửa sổ ứng dụng desktop
    if let Some(window) = state.app_handle.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }

    // Phát sự kiện tới giao diện desktop để hỏi xác nhận
    let event = PairRequestEvent {
        request_id: request_id.clone(),
        client_name: client_name.clone(),
        device_info: device_info.clone(),
        requested_at: Local::now().format("%H:%M:%S").to_string(),
    };
    let _ = state.app_handle.emit("pair-request", &event);

    // Chờ người dùng máy tính phản hồi trong tối đa 60 giây
    match tokio::time::timeout(std::time::Duration::from_secs(60), rx).await {
        Ok(Ok(true)) => {
            let session_id = generate_random_token();
            let peer = ConnectedPeer {
                session_id: session_id.clone(),
                client_name: client_name.clone(),
                device_info: device_info.clone(),
                connected_at: Local::now().format("%H:%M:%S").to_string(),
            };

            // Đảm bảo dọn sạch danh sách file gửi và nhận cũ khi kết nối thiết bị mới
            {
                let mut shared = state.shared_to_mobile.write().unwrap();
                shared.clear();
            }
            {
                let mut received = state.received_files.write().unwrap();
                received.clear();
            }
            let _ = state.app_handle.emit("shared-files-updated", &Vec::<SharedFileForMobile>::new());
            let _ = state.app_handle.emit("received-files-reset", ());

            {
                let mut active = state.active_peer.write().unwrap();
                *active = Some(peer.clone());
            }

            let _ = state.app_handle.emit("peer-connected", &peer);

            Ok(Json(serde_json::json!({
                "accepted": true,
                "session_id": session_id,
                "client_name": client_name,
            })))
        }
        Ok(Ok(false)) => {
            Err((
                StatusCode::FORBIDDEN,
                "Máy tính đã từ chối yêu cầu kết nối của bạn.".to_string(),
            ))
        }
        Ok(Err(_)) => {
            Err((
                StatusCode::GONE,
                "Yêu cầu kết nối đã bị hủy.".to_string(),
            ))
        }
        Err(_) => {
            {
                let mut pending = state.pending_pair.lock().unwrap();
                if let Some((ref req_id, _, _)) = *pending {
                    if req_id == &request_id {
                        *pending = None;
                    }
                }
            }
            let _ = state.app_handle.emit("pair-request-cancelled", serde_json::json!({ "request_id": request_id }));
            Err((
                StatusCode::REQUEST_TIMEOUT,
                "Quá thời gian chờ máy tính phản hồi (60 giây).".to_string(),
            ))
        }
    }
}

async fn handle_session_status(
    Query(query): Query<AuthSessionQuery>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let session_id = query.session_id.as_deref().unwrap_or("");
    let is_connected = state.is_active_session(session_id);

    if !is_connected {
        return Json(serde_json::json!({
            "connected": false,
            "message": "Không có kết nối nào đang hoạt động hoặc máy tính đã ngắt kết nối."
        }));
    }

    let active_name = state
        .active_peer
        .read()
        .unwrap()
        .as_ref()
        .map(|p| p.client_name.clone())
        .unwrap_or_default();

    let shared_files = state.shared_to_mobile.read().unwrap().clone();

    Json(serde_json::json!({
        "connected": true,
        "client_name": active_name,
        "shared_files": shared_files
    }))
}

async fn handle_disconnect_session(
    Query(query): Query<AuthSessionQuery>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let session_id = query.session_id.as_deref().unwrap_or("");
    let is_valid = state.is_active_session(session_id);

    if is_valid {
        state.disconnect_active_peer();
        Json(serde_json::json!({ "status": "disconnected" }))
    } else {
        Json(serde_json::json!({ "status": "already_disconnected" }))
    }
}

async fn handle_upload(
    Query(query): Query<AuthSessionQuery>,
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    if !state.is_token_valid(query.token.as_deref()) {
        return Err((
            StatusCode::FORBIDDEN,
            "Phiên làm việc đã hết hạn hoặc token không hợp lệ. Vui lòng quét lại mã QR trên máy tính.".to_string(),
        ));
    }

    let session_id = query.session_id.as_deref().unwrap_or("");
    if !state.is_active_session(session_id) {
        return Err((
            StatusCode::FORBIDDEN,
            "Thiết bị chưa được kết nối hoặc đã bị máy tính ngắt kết nối.".to_string(),
        ));
    }

    let mut saved_files = Vec::new();
    let base_dir = state.save_dir.read().unwrap().clone();
    let daily_dir = base_dir.join(current_date_folder_name());
    tokio::fs::create_dir_all(&daily_dir).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Không thể tạo thư mục lưu theo ngày: {}", e),
        )
    })?;

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
        let destination_path = daily_dir.join(&final_filename);

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

async fn handle_download_file(
    AxumPath(file_id): AxumPath<String>,
    Query(query): Query<AuthSessionQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Response, (StatusCode, String)> {
    if !state.is_token_valid(query.token.as_deref()) {
        return Err((StatusCode::FORBIDDEN, "Token không hợp lệ hoặc đã hết hạn".to_string()));
    }

    let session_id = query.session_id.as_deref().unwrap_or("");
    if !state.is_active_session(session_id) {
        return Err((StatusCode::FORBIDDEN, "Phiên kết nối không hợp lệ hoặc đã kết thúc".to_string()));
    }

    let (file_path, filename) = {
        let shared = state.shared_to_mobile.read().unwrap();
        let item = shared.iter().find(|f| f.id == file_id).ok_or((
            StatusCode::NOT_FOUND,
            "File không tồn tại trong danh sách chia sẻ".to_string(),
        ))?;
        (PathBuf::from(&item.path), item.filename.clone())
    };

    if !file_path.exists() {
        return Err((StatusCode::NOT_FOUND, "File không còn tồn tại trên máy tính".to_string()));
    }

    let file = tokio::fs::File::open(&file_path).await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Không thể mở file: {}", e))
    })?;

    let metadata = file.metadata().await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Không thể đọc thông tin file: {}", e))
    })?;
    let file_size = metadata.len();

    let stream = tokio_util::io::ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let ascii_fallback = filename
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
        .collect::<String>();
    let encoded_filename = percent_encode(&filename);
    let disposition = format!(
        "attachment; filename=\"{}\"; filename*=UTF-8''{}",
        ascii_fallback, encoded_filename
    );

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "application/octet-stream")
        .header(CONTENT_DISPOSITION, disposition)
        .header(CONTENT_LENGTH, file_size.to_string())
        .body(body)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(response)
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
fn respond_pair_request(
    state: tauri::State<Arc<AppState>>,
    request_id: String,
    accept: bool,
) -> Result<(), String> {
    let mut pending = state.pending_pair.lock().unwrap();
    if let Some((req_id, _, tx)) = pending.take() {
        if req_id == request_id {
            let _ = tx.send(accept);
            return Ok(());
        }
    }
    Err("Không tìm thấy yêu cầu kết nối hoặc đã hết thời gian chờ".to_string())
}

#[tauri::command]
fn disconnect_peer(state: tauri::State<Arc<AppState>>) -> Result<(), String> {
    state.disconnect_active_peer();
    Ok(())
}

#[tauri::command]
fn get_active_peer(state: tauri::State<Arc<AppState>>) -> Option<ConnectedPeer> {
    state.active_peer.read().unwrap().clone()
}

#[tauri::command]
async fn select_and_share_files(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<SharedFileForMobile>, String> {
    // Chỉ cho phép chia sẻ khi có thiết bị đang kết nối
    if state.active_peer.read().unwrap().is_none() {
        return Err("Chưa có điện thoại nào kết nối. Vui lòng kết nối thiết bị trước khi gửi file.".to_string());
    }

    use tauri_plugin_dialog::DialogExt;

    let files_res = app
        .dialog()
        .file()
        .set_title("Chọn file gửi sang điện thoại")
        .blocking_pick_files();

    if let Some(file_paths) = files_res {
        // Kiểm tra lại sau khi người dùng chọn xong hộp thoại (phòng trường hợp bị ngắt kết nối giữa chừng)
        if state.active_peer.read().unwrap().is_none() {
            return Err("Thiết bị đã ngắt kết nối. Không thể gửi file.".to_string());
        }

        let mut new_shared = Vec::new();
        for file_path in file_paths {
            if let Some(path) = file_path.as_path() {
                if let Ok(meta) = std::fs::metadata(path) {
                    if meta.is_file() {
                        let filename = path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("file.bin")
                            .to_string();
                        let size_bytes = meta.len();
                        let item = SharedFileForMobile {
                            id: generate_random_token()[0..10].to_string(),
                            filename,
                            path: path.to_string_lossy().to_string(),
                            size_bytes,
                            size_formatted: format_size(size_bytes),
                            added_at: Local::now().format("%H:%M:%S").to_string(),
                        };
                        new_shared.push(item);
                    }
                }
            }
        }

        {
            let mut shared = state.shared_to_mobile.write().unwrap();
            shared.extend(new_shared);
        }

        let updated = state.shared_to_mobile.read().unwrap().clone();
        let _ = app.emit("shared-files-updated", &updated);
        Ok(updated)
    } else {
        Ok(state.shared_to_mobile.read().unwrap().clone())
    }
}

#[tauri::command]
fn share_file_paths(
    paths: Vec<String>,
    app: tauri::AppHandle,
    state: tauri::State<Arc<AppState>>,
) -> Result<Vec<SharedFileForMobile>, String> {
    // Chỉ cho phép chia sẻ khi có thiết bị đang kết nối
    if state.active_peer.read().unwrap().is_none() {
        return Err("Chưa có điện thoại nào kết nối. Vui lòng kết nối thiết bị trước khi gửi file.".to_string());
    }

    let mut new_shared = Vec::new();
    for p in paths {
        let path = PathBuf::from(&p);
        if path.is_file() {
            if let Ok(meta) = std::fs::metadata(&path) {
                let filename = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("file.bin")
                    .to_string();
                let size_bytes = meta.len();
                let item = SharedFileForMobile {
                    id: generate_random_token()[0..10].to_string(),
                    filename,
                    path: path.to_string_lossy().to_string(),
                    size_bytes,
                    size_formatted: format_size(size_bytes),
                    added_at: Local::now().format("%H:%M:%S").to_string(),
                };
                new_shared.push(item);
            }
        }
    }

    {
        let mut shared = state.shared_to_mobile.write().unwrap();
        shared.extend(new_shared);
    }

    let updated = state.shared_to_mobile.read().unwrap().clone();
    let _ = app.emit("shared-files-updated", &updated);
    Ok(updated)
}

#[tauri::command]
fn remove_shared_file(
    id: String,
    app: tauri::AppHandle,
    state: tauri::State<Arc<AppState>>,
) -> Result<Vec<SharedFileForMobile>, String> {
    let mut shared = state.shared_to_mobile.write().unwrap();
    shared.retain(|f| f.id != id);
    let updated = shared.clone();
    let _ = app.emit("shared-files-updated", &updated);
    Ok(updated)
}

#[tauri::command]
fn get_shared_files(state: tauri::State<Arc<AppState>>) -> Vec<SharedFileForMobile> {
    state.shared_to_mobile.read().unwrap().clone()
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

#[tauri::command]
fn open_received_file(id: String, state: tauri::State<Arc<AppState>>) -> Result<(), String> {
    let path = find_received_path(&state, &id)?;
    open_with_default_app(&path)
}

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
                    .route("/check-peer", get(handle_check_peer))
                    .route("/pair-request", post(handle_pair_request))
                    .route("/session-status", get(handle_session_status))
                    .route("/disconnect-session", post(handle_disconnect_session))
                    .route("/download-file/:file_id", get(handle_download_file))
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
            respond_pair_request,
            disconnect_peer,
            get_active_peer,
            select_and_share_files,
            share_file_paths,
            remove_shared_file,
            get_shared_files,
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
    fn test_percent_encode() {
        let input = "Báo cáo tài chính 2026.pdf";
        let encoded = percent_encode(input);
        assert!(encoded.contains("%"));
        assert!(!encoded.contains(' '));
    }

    #[test]
    fn test_token_validity_without_time_limit() {
        let t = "random_token_12345".to_string();
        assert_eq!(t.len(), 18);
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
    fn test_pair_channel() {
        let (tx, rx) = tokio::sync::oneshot::channel::<bool>();
        tx.send(true).unwrap();
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

    #[test]
    fn test_current_date_folder_name_format() {
        let folder = current_date_folder_name();
        let parts: Vec<&str> = folder.split('-').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].len(), 2); // DD
        assert_eq!(parts[1].len(), 2); // MM
        assert_eq!(parts[2].len(), 4); // YYYY
    }
}
