let serverInfo = null;
let receivedFiles = [];
let sharedFiles = [];
let activePeer = null;

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

// Peer Status UI Elements
const peerStatusPill = document.getElementById('peerStatusPill');
const peerStatusDot = document.getElementById('peerStatusDot');
const peerStatusText = document.getElementById('peerStatusText');
const btnDisconnectPeer = document.getElementById('btnDisconnectPeer');

// PC to Mobile Sharing UI Elements
const sharedCountBadge = document.getElementById('sharedCountBadge');
const shareDropzone = document.getElementById('shareDropzone');
const emptyShareState = document.getElementById('emptyShareState');
const sharedFilesList = document.getElementById('sharedFilesList');

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

// Confirmation Modal Elements
const confirmModal = document.getElementById('confirmModal');
const modalTitle = document.getElementById('modalTitle');
const modalDesc = document.getElementById('modalDesc');
const modalBtnConfirm = document.getElementById('modalBtnConfirm');
let onModalConfirmCallback = null;

// Pairing Request Modal Elements
const pairConfirmModal = document.getElementById('pairConfirmModal');
const pairDeviceBadge = document.getElementById('pairDeviceBadge');
const pairClientName = document.getElementById('pairClientName');
let activePairRequestId = null;

function showPairConfirm(data) {
  if (!data || !data.request_id) return;
  activePairRequestId = data.request_id;
  pairClientName.textContent = data.client_name || 'Người dùng';
  pairDeviceBadge.textContent = data.device_info || 'Thiết bị di động';
  pairConfirmModal.classList.add('show');
}

async function handlePairResponse(accept) {
  if (!activePairRequestId) return;
  const requestId = activePairRequestId;
  activePairRequestId = null;
  pairConfirmModal.classList.remove('show');

  const invoke = getTauriInvoke();
  if (invoke) {
    try {
      await invoke('respond_pair_request', { requestId, accept });
      if (accept) {
        showToast('🤝 Đã chấp nhận kết nối với thiết bị!');
      } else {
        showToast('❌ Đã từ chối yêu cầu kết nối.');
      }
    } catch (err) {
      console.error('Respond pair request error:', err);
    }
  }
}

function updatePeerUI(peer) {
  activePeer = peer;
  const btnSelectShareFiles = document.getElementById('btnSelectShareFiles');

  if (peer && peer.client_name) {
    peerStatusPill.classList.add('connected');
    peerStatusText.textContent = `🟢 ${peer.client_name} (${peer.device_info || 'Di động'})`;
    btnDisconnectPeer.style.display = 'inline-block';
    if (btnSelectShareFiles) {
      btnSelectShareFiles.disabled = false;
      btnSelectShareFiles.title = `Chọn file từ máy tính để gửi sang ${peer.client_name}`;
    }
    showToast(`📱 Thiết bị '${peer.client_name}' đã kết nối!`);
  } else {
    peerStatusPill.classList.remove('connected');
    peerStatusText.textContent = 'Sẵn sàng (Chưa có thiết bị)';
    btnDisconnectPeer.style.display = 'none';
    if (btnSelectShareFiles) {
      btnSelectShareFiles.disabled = true;
      btnSelectShareFiles.title = 'Cần kết nối điện thoại trước khi gửi file';
    }

    // Khi ngắt kết nối: reset toàn bộ trạng thái hiện có trên giao diện
    sharedFiles = [];
    receivedFiles = [];
    renderFilesList();
  }
  renderSharedFiles();
}

async function handleDisconnectActivePeer() {
  if (!activePeer) return;
  const name = activePeer.client_name || 'thiết bị';
  showConfirmModal({
    title: 'Ngắt kết nối thiết bị?',
    desc: `Bạn có chắc muốn ngắt kết nối với '${name}'? Thiết bị sẽ bị ngắt và toàn bộ danh sách chia sẻ / nhận trong phiên này sẽ được làm sạch.`,
    confirmText: 'Ngắt kết nối',
    onConfirm: async () => {
      const invoke = getTauriInvoke();
      if (!invoke) return;
      try {
        await invoke('disconnect_peer');
        updatePeerUI(null);
        showToast(`🔌 Đã ngắt kết nối với '${name}' và làm sạch trạng thái`);
      } catch (err) {
        console.error('Disconnect error:', err);
        showToast('❌ Không thể ngắt kết nối');
      }
    }
  });
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

// ----------------- PC to Mobile Shared Files -----------------
function renderSharedFiles() {
  if (!activePeer) {
    emptyShareState.style.display = 'flex';
    emptyShareState.innerHTML = `
      <div style="font-size: 1.5rem; margin-bottom: 0.25rem;">📱</div>
      <div style="font-size: 0.85rem; font-weight: 600;">Chưa có thiết bị kết nối</div>
      <div style="font-size: 0.75rem; color: var(--text-muted); margin-top: 0.15rem;">
        Quét mã QR bằng điện thoại và kết nối để kích hoạt tính năng gửi file.
      </div>
    `;
    sharedFilesList.style.display = 'none';
    sharedCountBadge.textContent = '0 file';
    return;
  }

  if (!Array.isArray(sharedFiles) || sharedFiles.length === 0) {
    emptyShareState.style.display = 'flex';
    emptyShareState.innerHTML = `
      <div style="font-size: 1.5rem; margin-bottom: 0.25rem;">📤</div>
      <div style="font-size: 0.85rem; font-weight: 600;">Kéo & thả file vào đây hoặc bấm "Chọn file gửi"</div>
      <div style="font-size: 0.75rem; color: var(--text-muted); margin-top: 0.15rem;">
        File sẽ được gửi trực tiếp sang thiết bị: <strong>${escapeHtml(activePeer.client_name)}</strong>
      </div>
    `;
    sharedFilesList.style.display = 'none';
    sharedCountBadge.textContent = '0 file';
    return;
  }

  emptyShareState.style.display = 'none';
  sharedFilesList.style.display = 'flex';
  sharedCountBadge.textContent = `${sharedFiles.length} file`;

  sharedFilesList.innerHTML = sharedFiles.map((file) => {
    const meta = getFileIconMeta(file.filename);
    const safeFilename = escapeHtml(file.filename);
    return `
      <div class="shared-file-row">
        <div class="shared-file-info">
          <span style="font-size: 1.1rem;">${meta.icon}</span>
          <span class="shared-file-name" title="${safeFilename}">${safeFilename}</span>
          <span class="shared-file-size">• ${escapeHtml(file.size_formatted)}</span>
        </div>
        <button class="btn-icon-danger" onclick="handleRemoveSharedFile('${file.id}')" title="Xóa khỏi danh sách chia sẻ">
          <svg width="14" height="14" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>
    `;
  }).join('');
}

async function handleSelectShareFiles() {
  if (!activePeer) {
    showToast('⚠️ Vui lòng kết nối với điện thoại trước khi gửi file');
    return;
  }
  const invoke = getTauriInvoke();
  if (!invoke) return;

  try {
    const updated = await invoke('select_and_share_files');
    if (updated) {
      sharedFiles = updated;
      renderSharedFiles();
      if (updated.length > 0) {
        showToast(`📤 Đã gửi ${updated.length} file sang ${activePeer.client_name}`);
      }
    }
  } catch (err) {
    console.error('Select share files error:', err);
    showToast(`❌ ${err}`);
  }
}

async function handleRemoveSharedFile(id) {
  const invoke = getTauriInvoke();
  if (!invoke) return;

  try {
    const updated = await invoke('remove_shared_file', { id });
    sharedFiles = updated || [];
    renderSharedFiles();
    showToast('🗑️ Đã xóa file khỏi danh sách chia sẻ');
  } catch (err) {
    console.error('Remove shared file error:', err);
  }
}

let lastShareTime = 0;
let lastSharedPathsSignature = '';

async function handleSharePaths(paths) {
  if (!Array.isArray(paths) || paths.length === 0) return;

  if (!activePeer) {
    showToast('⚠️ Vui lòng kết nối với điện thoại trước khi gửi file');
    return;
  }

  const signature = paths.join('|');
  const now = Date.now();
  if (signature === lastSharedPathsSignature && now - lastShareTime < 1000) {
    return;
  }
  lastShareTime = now;
  lastSharedPathsSignature = signature;

  const invoke = getTauriInvoke();
  if (!invoke) return;

  try {
    const updated = await invoke('share_file_paths', { paths });
    if (updated) {
      sharedFiles = updated;
      renderSharedFiles();
      showToast(`📤 Đã thêm ${paths.length} file vào danh sách gửi sang ${activePeer.client_name}`);
    }
  } catch (err) {
    console.error('Share file paths error:', err);
    showToast(`❌ ${err}`);
  }
}

// Ngăn trình duyệt mở file trực tiếp khi kéo thả vào cửa sổ ứng dụng
window.addEventListener('dragover', (e) => {
  e.preventDefault();
}, false);
window.addEventListener('drop', (e) => {
  e.preventDefault();
}, false);

// Drag & drop support for shareDropzone
if (shareDropzone) {
  ['dragenter', 'dragover'].forEach(name => {
    shareDropzone.addEventListener(name, (e) => {
      e.preventDefault();
      if (e.dataTransfer) {
        e.dataTransfer.dropEffect = activePeer ? 'copy' : 'none';
      }
      if (activePeer) {
        shareDropzone.classList.add('dragover');
      }
    }, false);
  });

  ['dragleave'].forEach(name => {
    shareDropzone.addEventListener(name, (e) => {
      e.preventDefault();
      shareDropzone.classList.remove('dragover');
    }, false);
  });

  shareDropzone.addEventListener('drop', async (e) => {
    e.preventDefault();
    shareDropzone.classList.remove('dragover');
    if (!activePeer) {
      showToast('⚠️ Vui lòng kết nối với điện thoại trước khi gửi file');
      return;
    }

    if (e.dataTransfer && e.dataTransfer.files && e.dataTransfer.files.length > 0) {
      const paths = [];
      for (let i = 0; i < e.dataTransfer.files.length; i++) {
        const f = e.dataTransfer.files[i];
        if (f.path) {
          paths.push(f.path);
        }
      }
      if (paths.length > 0) {
        await handleSharePaths(paths);
      }
    }
  }, false);
}

if (emptyShareState) {
  emptyShareState.addEventListener('click', () => {
    handleSelectShareFiles();
  });
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

function updateServerInfoUI(info) {
  serverInfo = info;
  networkText.textContent = `Wi-Fi: ${info.local_ip}:${info.port}`;
  dirPath.textContent = info.save_dir;
  dirPath.title = info.save_dir;

  // Render SVG QR code
  if (info.qr_svg) {
    qrContainer.innerHTML = info.qr_svg;
  }
}

function getFileIconMeta(filename) {
  if (!filename) return { icon: '📁', className: 'file-icon-other' };
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

  updateProgressModal.classList.add('show');
  updateProgressBar.style.width = '0%';
  updateProgressPercent.textContent = '0%';
  updateProgressSize.textContent = 'Đang chuẩn bị tải...';
  updateStatusMsg.textContent = 'Đang kết nối đến GitHub để tải bản cập nhật...';

  try {
    await invoke('install_update');
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

    // 2. Fetch active peer if any
    try {
      const currentPeer = await invoke('get_active_peer');
      updatePeerUI(currentPeer);
    } catch (e) {
      console.warn('Failed to fetch active peer:', e);
    }

    // 3. Fetch shared files (PC -> Mobile)
    try {
      const pcFiles = await invoke('get_shared_files');
      sharedFiles = pcFiles || [];
      renderSharedFiles();
    } catch (e) {
      console.warn('Failed to fetch shared files:', e);
    }

    // 4. Fetch existing received files
    const files = await invoke('get_received_files');
    receivedFiles = files || [];
    renderFilesList();

    // 5. Listen to real-time events
    if (listen) {
      // Nhận file mới từ điện thoại
      await listen('file-received', (event) => {
        if (event && event.payload) {
          addReceivedFile(event.payload);
        }
      });

      // Token / QR được làm mới
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

      // Lắng nghe yêu cầu kết nối từ điện thoại
      await listen('pair-request', (event) => {
        if (event && event.payload) {
          showPairConfirm(event.payload);
        }
      });

      // Yêu cầu kết nối bị hủy / timeout
      await listen('pair-request-cancelled', (event) => {
        if (event && event.payload && event.payload.request_id === activePairRequestId) {
          pairConfirmModal.classList.remove('show');
          activePairRequestId = null;
          showToast('⏳ Yêu cầu kết nối đã hết thời gian chờ.');
        }
      });

      // Peer đã kết nối thành công
      await listen('peer-connected', (event) => {
        if (event && event.payload) {
          updatePeerUI(event.payload);
        }
      });

      // Peer đã ngắt kết nối
      await listen('peer-disconnected', () => {
        updatePeerUI(null);
        showToast('🔌 Thiết bị đã ngắt kết nối');
      });

      // Cập nhật danh sách file PC chia sẻ
      await listen('shared-files-updated', (event) => {
        if (event && event.payload) {
          sharedFiles = event.payload;
          renderSharedFiles();
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

      // Lắng nghe sự kiện Kéo & Thả file từ OS (Tauri Native Drag & Drop)
      try {
        const currentWin = window.__TAURI__?.window?.getCurrentWindow?.()
          || window.__TAURI__?.webviewWindow?.getCurrentWebviewWindow?.()
          || window.__TAURI__?.webview?.getCurrentWebview?.();

        if (currentWin && typeof currentWin.onDragDropEvent === 'function') {
          await currentWin.onDragDropEvent((event) => {
            const payload = event?.payload;
            if (!payload) return;

            if (payload.type === 'enter' || payload.type === 'over') {
              if (activePeer && shareDropzone) {
                shareDropzone.classList.add('dragover');
              }
            } else if (payload.type === 'leave') {
              if (shareDropzone) {
                shareDropzone.classList.remove('dragover');
              }
            } else if (payload.type === 'drop') {
              if (shareDropzone) {
                shareDropzone.classList.remove('dragover');
              }
              if (payload.paths && payload.paths.length > 0) {
                handleSharePaths(payload.paths);
              }
            }
          });
        }
      } catch (e) {
        console.warn('onDragDropEvent setup failed:', e);
      }

      // Fallback cho event tauri://drag-drop nếu onDragDropEvent không dùng được
      try {
        await listen('tauri://drag-enter', () => {
          if (activePeer && shareDropzone) shareDropzone.classList.add('dragover');
        });
        await listen('tauri://drag-leave', () => {
          if (shareDropzone) shareDropzone.classList.remove('dragover');
        });
        await listen('tauri://drag-drop', (event) => {
          if (shareDropzone) shareDropzone.classList.remove('dragover');
          if (event && event.payload && event.payload.paths && event.payload.paths.length > 0) {
            handleSharePaths(event.payload.paths);
          }
        });
      } catch (e) {
        console.warn('Fallback drag listeners setup failed:', e);
      }
    }
  } catch (err) {
    console.error('Init error:', err);
  }
}
