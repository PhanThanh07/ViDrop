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

// Update UI Elements
const btnCheckUpdate = document.getElementById('btnCheckUpdate');
const updateBtnText = document.getElementById('updateBtnText');
const updateBanner = document.getElementById('updateBanner');
const updateVersionTitle = document.getElementById('updateVersionTitle');
const updateChangelog = document.getElementById('updateChangelog');
const updateProgressModal = document.getElementById('updateProgressModal');
const updateProgressBar = document.getElementById('updateProgressBar');
const updateProgressPercent = document.getElementById('updateProgressPercent');
const updateProgressSize = document.getElementById('updateProgressSize');
const updateStatusMsg = document.getElementById('updateStatusMsg');
let currentUpdateInfo = null;
let isUpdating = false;

// Modal Elements
const confirmModal = document.getElementById('confirmModal');
const modalTitle = document.getElementById('modalTitle');
const modalDesc = document.getElementById('modalDesc');
const modalBtnConfirm = document.getElementById('modalBtnConfirm');
let onModalConfirmCallback = null;

// Transfer Request Modal Elements
const transferConfirmModal = document.getElementById('transferConfirmModal');
const transferDeviceName = document.getElementById('transferDeviceName');
const transferFileCount = document.getElementById('transferFileCount');
const transferTotalSize = document.getElementById('transferTotalSize');
const transferFilesPreview = document.getElementById('transferFilesPreview');
let activeTransferRequestId = null;

function showTransferConfirm(data) {
  if (!data || !data.request_id) return;
  activeTransferRequestId = data.request_id;
  transferDeviceName.textContent = data.device_name || 'Thiết bị di động';
  transferFileCount.textContent = `${data.files.length} file`;
  transferTotalSize.textContent = data.total_size_formatted || formatBytes(data.total_size);

  if (Array.isArray(data.files)) {
    transferFilesPreview.innerHTML = data.files.map(f => `
      <div class="transfer-file-item">
        <span class="transfer-file-name" title="${escapeHtml(f.name)}">${escapeHtml(f.name)}</span>
        <span class="transfer-file-size">${formatBytes(f.size)}</span>
      </div>
    `).join('');
  } else {
    transferFilesPreview.innerHTML = '';
  }

  transferConfirmModal.classList.add('show');
}

async function handleTransferResponse(accept) {
  if (!activeTransferRequestId) return;
  const requestId = activeTransferRequestId;
  activeTransferRequestId = null;
  transferConfirmModal.classList.remove('show');

  const invoke = getTauriInvoke();
  if (invoke) {
    try {
      await invoke('respond_transfer_request', { requestId, accept });
      if (accept) {
        showToast(' Đã đồng ý nhận. Đang chờ thiết bị tải file...');
      } else {
        showToast('❌ Đã từ chối nhận file.');
      }
    } catch (err) {
      console.error('Respond transfer error:', err);
    }
  }
}

function showConfirmModal({ title, desc, confirmText, onConfirm }) {
  modalTitle.textContent = title || 'Xác nhận';
  modalDesc.textContent = desc || 'Bạn có chắc chắn muốn thực hiện hành động này?';
  modalBtnConfirm.textContent = confirmText || 'Xác nhận';
  onModalConfirmCallback = onConfirm;
  confirmModal.classList.add('show');
}

function closeConfirmModal() {
  confirmModal.classList.remove('show');
  onModalConfirmCallback = null;
}

modalBtnConfirm.addEventListener('click', () => {
  if (onModalConfirmCallback) {
    onModalConfirmCallback();
  }
  closeConfirmModal();
});

function escapeHtml(str) {
  if (!str) return '';
  return String(str)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}

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
    if (qrStatusBadge) {
      qrStatusBadge.textContent = 'Đang làm mới...';
      qrStatusBadge.className = 'badge expired';
    }
    return;
  }

  timerCountdown.textContent = formatTime(remainingSeconds);
  const pct = Math.max(0, Math.min(100, (remainingSeconds / totalSeconds) * 100));
  timerBar.style.width = `${pct}%`;

  if (remainingSeconds <= 60) {
    timerCountdown.className = 'timer-countdown warning';
    timerBar.className = 'timer-bar-fill warning';
    if (qrStatusBadge) {
      qrStatusBadge.textContent = 'Sắp làm mới';
      qrStatusBadge.className = 'badge expired';
    }
  } else {
    timerCountdown.className = 'timer-countdown';
    timerBar.className = 'timer-bar-fill';
    if (qrStatusBadge) {
      qrStatusBadge.textContent = 'Đang hiệu lực';
      qrStatusBadge.className = 'badge';
    }
  }
  if (qrOverlay) qrOverlay.classList.remove('visible');
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
      // Tự động tạo lại mã QR khi hết hạn 5 phút
      handleRefreshToken(true);
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
    const safeFilename = escapeHtml(file.filename);
    const row = document.createElement('div');
    row.className = 'file-row';
    row.innerHTML = `
      <div class="file-row-left">
        <div class="file-icon-box ${meta.className}">${meta.icon}</div>
        <div class="file-info">
          <div class="file-name" title="${safeFilename}">${safeFilename}</div>
          <div class="file-meta">
            <span>${escapeHtml(file.size_formatted)}</span>
            <span>•</span>
            <span>${escapeHtml(file.received_at)}</span>
          </div>
        </div>
      </div>
      <div class="file-row-actions">
        <button class="btn btn-outline btn-sm" onclick="handleOpenFileById('${file.id}')" title="Mở file bằng ứng dụng mặc định">
          <span>Xem file</span>
        </button>
        <button class="btn-icon-plain" onclick="handleRevealFileById('${file.id}')" title="Hiện trong thư mục">
          <svg width="16" height="16" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 19a2 2 0 01-2-2V7a2 2 0 012-2h4l2 2h4a2 2 0 012 2v1M5 19h14a2 2 0 002-2v-5a2 2 0 00-2-2H9a2 2 0 00-2 2v5a2 2 0 01-2 2z" />
          </svg>
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

  showConfirmModal({
    title: 'Xóa toàn bộ danh sách?',
    desc: 'Danh sách các file đã nhận trong phiên sẽ bị xóa khỏi giao diện. Các file đã tải về trong thư mục ViDrop vẫn được giữ nguyên trên đĩa.',
    confirmText: 'Xóa tất cả',
    onConfirm: async () => {
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
  });
}

async function handleOpenFileById(id) {
  const invoke = getTauriInvoke();
  if (!invoke) return;
  try {
    await invoke('open_received_file', { id });
  } catch (err) {
    console.error('Open file error:', err);
    showToast(`❌ ${err || 'Không thể mở file'}`);
  }
}

async function handleRevealFileById(id) {
  const invoke = getTauriInvoke();
  if (!invoke) return;
  try {
    await invoke('reveal_received_file', { id });
  } catch (err) {
    console.error('Reveal file error:', err);
    showToast(`❌ ${err || 'Không thể mở thư mục chứa file'}`);
  }
}

// ----------------- Global Actions -----------------
let isRefreshing = false;
async function handleRefreshToken(isAuto = false) {
  if (isRefreshing) return;
  const invoke = getTauriInvoke();
  if (!invoke) return;

  isRefreshing = true;
  const btnRefresh = document.getElementById('btnRefresh');
  if (btnRefresh) btnRefresh.classList.add('spinning');

  try {
    const info = await invoke('refresh_token');
    updateServerInfoUI(info);
    if (!isAuto) {
      showToast('⚡ Đã làm mới mã QR!');
    }
  } catch (err) {
    console.error('Refresh token error:', err);
    if (!isAuto) {
      showToast('❌ Không thể tạo mã mới');
    }
  } finally {
    isRefreshing = false;
    setTimeout(() => {
      if (btnRefresh) btnRefresh.classList.remove('spinning');
    }, 450);
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

async function handleChangeDirectory() {
  const invoke = getTauriInvoke();
  if (!invoke) return;

  try {
    const selected = await invoke('select_save_directory');
    if (selected) {
      dirPath.textContent = selected;
      dirPath.title = selected;
      localStorage.setItem('vidrop-save-dir', selected);
      showToast('📁 Đã đổi thư mục lưu thành công!');
    }
  } catch (err) {
    console.error('Change directory error:', err);
    showToast(`❌ Không thể đổi thư mục: ${err}`);
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

// ----------------- Auto Updater Handlers -----------------
function formatBytes(bytes) {
  if (!bytes || bytes <= 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
}

function showUpdateBanner(info) {
  if (!info || !info.version) return;
  currentUpdateInfo = info;
  updateVersionTitle.textContent = `⚡ Đã có Vi Drop v${info.version}!`;
  if (info.body) {
    const cleanBody = info.body.replace(/\r\n/g, ' ').replace(/\n/g, ' ').trim();
    updateChangelog.textContent = cleanBody.length > 70 ? cleanBody.substring(0, 70) + '...' : cleanBody;
  } else {
    updateChangelog.textContent = 'Nhấn Cập nhật ngay để tải và cài đặt tự động.';
  }
  updateBanner.style.display = 'flex';
}

function dismissUpdateBanner() {
  updateBanner.style.display = 'none';
}

async function handleManualCheckUpdate() {
  const invoke = getTauriInvoke();
  if (!invoke) return;

  try {
    updateBtnText.textContent = 'Đang tìm...';
    btnCheckUpdate.disabled = true;

    const res = await invoke('check_update');
    if (res && res.available && res.version) {
      showUpdateBanner(res);
      showToast(`🚀 Phát hiện phiên bản mới: v${res.version}`);
    } else {
      const verText = res && res.current_version ? ` (v${res.current_version})` : '';
      showToast(`✅ Bạn đang dùng phiên bản mới nhất${verText}`);
    }
  } catch (err) {
    console.error('Check update error:', err);
    showToast(`⚠️ Không thể kiểm tra: ${err}`);
  } finally {
    updateBtnText.textContent = 'Cập nhật';
    btnCheckUpdate.disabled = false;
  }
}

async function handleStartUpdate() {
  if (isUpdating) return;
  const invoke = getTauriInvoke();
  if (!invoke) return;

  isUpdating = true;
  dismissUpdateBanner();

  // Hiển thị modal tiến trình
  updateProgressModal.classList.add('show');
  updateProgressBar.style.width = '0%';
  updateProgressPercent.textContent = '0%';
  updateProgressSize.textContent = 'Đang chuẩn bị tải...';
  updateStatusMsg.textContent = 'Đang kết nối đến GitHub để tải bản cập nhật...';

  try {
    await invoke('install_update');
    // Khi hoàn tất thành công, backend sẽ gọi app.restart() để khởi động lại ứng dụng
  } catch (err) {
    console.error('Install update error:', err);
    isUpdating = false;
    updateStatusMsg.textContent = `❌ Lỗi: ${err}`;
    setTimeout(() => {
      updateProgressModal.classList.remove('show');
      showToast(`❌ Cập nhật thất bại: ${err}`);
    }, 4000);
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

    // 1.1 Restore custom save directory if previously set
    const savedDir = localStorage.getItem('vidrop-save-dir');
    if (savedDir) {
      try {
        const actualDir = await invoke('set_save_directory', { path: savedDir });
        if (actualDir) {
          dirPath.textContent = actualDir;
          dirPath.title = actualDir;
        }
      } catch (e) {
        console.warn('Failed to restore custom save directory:', e);
      }
    }

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

      // Lắng nghe thay đổi thư mục lưu file
      await listen('save-directory-changed', (event) => {
        if (event && event.payload && event.payload.path) {
          dirPath.textContent = event.payload.path;
          dirPath.title = event.payload.path;
        }
      });

      // Lắng nghe sự kiện người gửi xin phép gửi file
      await listen('transfer-request', (event) => {
        if (event && event.payload) {
          showTransferConfirm(event.payload);
        }
      });

      // Lắng nghe sự kiện yêu cầu bị hủy hoặc hết hạn
      await listen('transfer-request-cancelled', (event) => {
        if (event && event.payload && event.payload.request_id === activeTransferRequestId) {
          transferConfirmModal.classList.remove('show');
          activeTransferRequestId = null;
          showToast('⏳ Yêu cầu gửi file đã hết thời gian chờ.');
        }
      });

      // Lắng nghe thông báo cập nhật tự động
      await listen('update-available', (event) => {
        if (event && event.payload) {
          showUpdateBanner(event.payload);
        }
      });

      // Lắng nghe tiến trình tải bản cập nhật
      await listen('update-download-progress', (event) => {
        if (event && event.payload) {
          const { downloaded, total } = event.payload;
          if (total && total > 0) {
            const pct = Math.min(100, Math.round((downloaded / total) * 100));
            updateProgressBar.style.width = `${pct}%`;
            updateProgressPercent.textContent = `${pct}%`;
            updateProgressSize.textContent = `${formatBytes(downloaded)} / ${formatBytes(total)}`;
            if (pct >= 100) {
              updateStatusMsg.textContent = 'Tải xong! Đang cài đặt và khởi động lại ứng dụng...';
            }
          }
        }
      });
    }
  } catch (err) {
    console.error('Init error:', err);
  }
}
