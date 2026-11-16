let serverInfo = null;
let remainingSeconds = 300;
let totalSeconds = 300;
let timerInterval = null;
let receivedFiles = [];

// Fallback Tauri API helpers
const getTauriInvoke = () => {
  if (window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke) {
    return window.__TAURI__.core.invoke;
  }
  return null;
};

const getTauriListen = () => {
  if (window.__TAURI__ && window.__TAURI__.event && window.__TAURI__.event.listen) {
    return window.__TAURI__.event.listen;
  }
  return null;
};

// UI Elements
const qrContainer = document.getElementById('qrContainer');
const qrOverlay = document.getElementById('qrOverlay');
const qrStatusBadge = document.getElementById('qrStatusBadge');
const timerCountdown = document.getElementById('timerCountdown');
const timerBar = document.getElementById('timerBar');
const networkText = document.getElementById('networkText');
const dirPath = document.getElementById('dirPath');
const emptyState = document.getElementById('emptyState');
const filesList = document.getElementById('filesList');
const filesCountBadge = document.getElementById('filesCountBadge');
const totalSizeText = document.getElementById('totalSizeText');
const btnClearAll = document.getElementById('btnClearAll');
const toastEl = document.getElementById('toast');
const themeToggleBtn = document.getElementById('themeToggleBtn');
const themeIcon = document.getElementById('themeIcon');
const themeText = document.getElementById('themeText');

// ----------------- Theme Management -----------------
function initTheme() {
  const savedTheme = localStorage.getItem('vidrop-theme') || 'light';
  applyTheme(savedTheme);
}

function applyTheme(theme) {
  document.documentElement.setAttribute('data-theme', theme);
  localStorage.setItem('vidrop-theme', theme);
  if (theme === 'dark') {
    themeIcon.textContent = '☀️';
    themeText.textContent = 'Giao diện sáng';
  } else {
    themeIcon.textContent = '🌙';
    themeText.textContent = 'Giao diện tối';
  }
}

function toggleTheme() {
  const currentTheme = document.documentElement.getAttribute('data-theme') || 'light';
  const newTheme = currentTheme === 'light' ? 'dark' : 'light';
  applyTheme(newTheme);
  showToast(newTheme === 'dark' ? '🌙 Đã bật Giao diện tối' : '☀️ Đã bật Giao diện sáng');
}

// ----------------- Notifications -----------------
function showToast(message) {
  toastEl.textContent = message;
  toastEl.classList.add('show');
  setTimeout(() => {
    toastEl.classList.remove('show');
  }, 2500);
}

function formatTime(secs) {
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return `${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`;
}

function updateTimerUI() {
  if (remainingSeconds <= 0) {
    timerCountdown.textContent = '00:00';
    timerCountdown.className = 'timer-countdown expired';
    timerBar.className = 'timer-bar-fill expired';
    timerBar.style.width = '0%';
    qrOverlay.classList.add('visible');
    qrStatusBadge.textContent = 'Đã hết hạn';
    qrStatusBadge.className = 'badge expired';
    return;
  }

  timerCountdown.textContent = formatTime(remainingSeconds);
  const pct = Math.max(0, Math.min(100, (remainingSeconds / totalSeconds) * 100));
  timerBar.style.width = `${pct}%`;

  if (remainingSeconds <= 60) {
    timerCountdown.className = 'timer-countdown warning';
    timerBar.className = 'timer-bar-fill warning';
    qrStatusBadge.textContent = 'Sắp hết hạn';
    qrStatusBadge.className = 'badge expired';
  } else {
    timerCountdown.className = 'timer-countdown';
    timerBar.className = 'timer-bar-fill';
    qrStatusBadge.textContent = 'Đang hiệu lực';
    qrStatusBadge.className = 'badge';
  }
  qrOverlay.classList.remove('visible');
}

function startCountdown() {
  if (timerInterval) clearInterval(timerInterval);
  updateTimerUI();
  timerInterval = setInterval(() => {
    if (remainingSeconds > 0) {
      remainingSeconds--;
      updateTimerUI();
    } else {
      clearInterval(timerInterval);
      updateTimerUI();
    }
  }, 1000);
}

function updateServerInfoUI(info) {
  serverInfo = info;
  remainingSeconds = info.remaining_seconds;
  totalSeconds = info.expires_in_seconds || 300;

  networkText.textContent = `Wi-Fi: ${info.local_ip}:${info.port}`;
  dirPath.textContent = info.save_dir;
  dirPath.title = info.save_dir;

  // Render SVG QR code
  if (info.qr_svg) {
    qrContainer.innerHTML = info.qr_svg;
  }

  startCountdown();
}

function getFileIconMeta(filename) {
  const ext = (filename.split('.').pop() || '').toLowerCase();
  if (['mp4', 'mov', 'avi', 'mkv', 'webm', 'wmv'].includes(ext)) {
    return { icon: '🎬', className: 'file-icon-video' };
  }
  if (['jpg', 'jpeg', 'png', 'heic', 'webp', 'gif', 'svg', 'bmp'].includes(ext)) {
    return { icon: '🖼️', className: 'file-icon-image' };
  }
  if (['mp3', 'm4a', 'wav', 'aac', 'flac', 'ogg'].includes(ext)) {
    return { icon: '🎵', className: 'file-icon-audio' };
  }
  if (['zip', 'rar', '7z', 'tar', 'gz'].includes(ext)) {
    return { icon: '📦', className: 'file-icon-zip' };
  }
  if (['pdf', 'doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx', 'txt', 'csv', 'md'].includes(ext)) {
    return { icon: '📄', className: 'file-icon-doc' };
  }
  return { icon: '📁', className: 'file-icon-other' };
}

function renderFilesList() {
  if (!receivedFiles || receivedFiles.length === 0) {
    emptyState.style.display = 'flex';
    filesList.style.display = 'none';
    btnClearAll.style.display = 'none';
    filesCountBadge.textContent = '0 file';
    totalSizeText.textContent = 'Tổng: 0 MB';
    return;
  }

  emptyState.style.display = 'none';
  filesList.style.display = 'flex';
  btnClearAll.style.display = 'inline-flex';
  filesCountBadge.textContent = `${receivedFiles.length} file`;

  // Calculate total bytes
  const totalBytes = receivedFiles.reduce((acc, f) => acc + (f.size_bytes || 0), 0);
  const totalMB = (totalBytes / (1024 * 1024)).toFixed(1);
  totalSizeText.textContent = `Tổng: ${totalMB} MB`;

  filesList.innerHTML = '';
  // Show newest on top
  const sorted = [...receivedFiles].reverse();

  sorted.forEach((file) => {
    const meta = getFileIconMeta(file.filename);
    const row = document.createElement('div');
    row.className = 'file-row';
    row.innerHTML = `
      <div class="file-row-left">
        <div class="file-icon-box ${meta.className}">${meta.icon}</div>
        <div class="file-info">
          <div class="file-name" title="${file.filename}">${file.filename}</div>
          <div class="file-meta">
            <span>${file.size_formatted}</span>
            <span>•</span>
            <span>${file.received_at}</span>
          </div>
        </div>
      </div>
      <div class="file-row-actions">
        <button class="btn btn-outline btn-sm" onclick="handleOpenFile('${file.path.replace(/\\/g, '\\\\')}')" title="Định vị file">
          <span>Xem file</span>
        </button>
        <button class="btn-icon-danger" onclick="handleDeleteFile('${file.id}')" title="Xóa khỏi danh sách">
          <svg width="16" height="16" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
          </svg>
        </button>
      </div>
    `;
    filesList.appendChild(row);
  });
}

function addReceivedFile(file) {
  receivedFiles.push(file);
  renderFilesList();
  showToast(`📥 Đã nhận: ${file.original_name || file.filename}`);
}

// ----------------- File Deletion Actions -----------------
async function handleDeleteFile(id) {
  const invoke = getTauriInvoke();
  if (!invoke) return;

  try {
    const updated = await invoke('delete_received_file', { id });
    receivedFiles = updated || [];
    renderFilesList();
    showToast('🗑️ Đã xóa file khỏi danh sách');
  } catch (err) {
    console.error('Delete file error:', err);
    showToast('❌ Lỗi khi xóa file');
  }
}

async function handleClearAll() {
  if (!receivedFiles || receivedFiles.length === 0) return;
  if (!confirm('Bạn có chắc chắn muốn xóa toàn bộ danh sách file đã nhận không? (File trên đĩa vẫn được giữ nguyên)')) {
    return;
  }

  const invoke = getTauriInvoke();
  if (!invoke) return;

  try {
    const updated = await invoke('clear_received_files');
    receivedFiles = updated || [];
    renderFilesList();
    showToast('🧹 Đã dọn sạch danh sách file');
  } catch (err) {
    console.error('Clear files error:', err);
    showToast('❌ Lỗi khi xóa danh sách');
  }
}

// ----------------- Global Actions -----------------
async function handleRefreshToken() {
  const invoke = getTauriInvoke();
  if (!invoke) return;

  try {
    const info = await invoke('refresh_token');
    updateServerInfoUI(info);
    showToast('⚡ Đã tạo mã QR và Token mới!');
  } catch (err) {
    console.error('Refresh token error:', err);
    showToast('❌ Không thể tạo mã mới');
  }
}

async function handleCopyLink() {
  if (!serverInfo || !serverInfo.url) return;
  try {
    await navigator.clipboard.writeText(serverInfo.url);
    showToast('📋 Đã sao chép liên kết vào Clipboard!');
  } catch (err) {
    console.error('Copy link error:', err);
    showToast('❌ Không thể sao chép');
  }
}

async function handleOpenDirectory() {
  const invoke = getTauriInvoke();
  if (!invoke) return;
  try {
    await invoke('open_save_directory');
  } catch (err) {
    console.error('Open directory error:', err);
    showToast('❌ Không thể mở thư mục');
  }
}

async function handleOpenFile(path) {
  const invoke = getTauriInvoke();
  if (!invoke) return;
  try {
    await invoke('open_file', { path });
  } catch (err) {
    console.error('Open file error:', err);
    showToast('❌ Không thể mở file');
  }
}

// App Initialization
window.addEventListener('DOMContentLoaded', async () => {
  initTheme();

  const invoke = getTauriInvoke();
  if (!invoke) {
    console.warn('Tauri API not found yet, retrying...');
    setTimeout(initTauri, 300);
    return;
  }

  await initTauri();
});

async function initTauri() {
  const invoke = getTauriInvoke();
  const listen = getTauriListen();

  if (!invoke) {
    console.error('Tauri invoke API unavailable');
    return;
  }

  try {
    // 1. Fetch server info
    const info = await invoke('get_server_info');
    updateServerInfoUI(info);

    // 2. Fetch existing files
    const files = await invoke('get_received_files');
    receivedFiles = files || [];
    renderFilesList();

    // 3. Listen to real-time events
    if (listen) {
      await listen('file-received', (event) => {
        if (event && event.payload) {
          addReceivedFile(event.payload);
        }
      });

      await listen('token-refreshed', (event) => {
        if (event && event.payload) {
          updateServerInfoUI(event.payload);
        }
      });
    }
  } catch (err) {
    console.error('Init error:', err);
  }
}
