# Vi Drop Desktop

Ứng dụng desktop nhận file bảo mật từ điện thoại (iOS & Android) qua mạng Wi-Fi nội bộ bằng cách quét mã QR, xây dựng bằng **Tauri v2 (Rust + Vanilla HTML/CSS/JS)**.

## 🚀 Tính năng nổi bật

1. **Đổi mới & Giao diện:**
   - Hỗ trợ cả **Giao diện sáng (Light mode - Mặc định)** và **Giao diện tối (Dark mode)** với nút chuyển đổi 1-click ở thanh tiêu đề (tự động ghi nhớ qua `localStorage`).
   - Thiết kế 2 cột hiện đại, sắc nét, hỗ trợ đầy đủ animation chuyển động mượt mà.
2. **Quản lý danh sách file nhận:**
   - Cho phép **xoá từng file** khỏi danh sách bằng nút thùng rác 🗑️ trên từng dòng.
   - Nút **"Xóa tất cả"** xuất hiện trên tiêu đề danh sách khi có file, giúp dọn sạch danh sách file phiên làm việc nhanh chóng.
   - Nút **"Xem file"** giúp mở trực tiếp vị trí file trong Finder (macOS) hoặc File Explorer (Windows).
3. **Local Web Server (Rust Axum & Tokio):**
   - Lắng nghe trên tất cả interface mạng (`0.0.0.0`) tại cổng `8787` (hoặc tự động chọn cổng trống nếu bị trùng).
   - Tự động nhận diện địa chỉ IP mạng nội bộ (LAN) của máy tính (`local-ip-address`).
   - Sinh **Session Token ngẫu nhiên 32 ký tự** có thời hạn **5 phút (300 giây)**.
   - Sinh mã QR dạng chuỗi SVG sắc nét chứa URL đầy đủ: `http://<LOCAL_IP>:<PORT>/?token=<SESSION_TOKEN>`.
4. **Xác thực & Bảo mật chặt chẽ:**
   - Mọi request (`GET /` và `POST /upload`) đều kiểm tra token và thời hạn 5 phút; nếu sai hoặc hết hạn -> từ chối với HTTP `403 Forbidden`.
   - Nút **"Tạo mã QR mới"** trên giao diện desktop giúp sinh token mới và vô hiệu hóa token cũ ngay lập tức.
   - Lọc tên file (sanitize filename) chống Path Traversal (`../`), tự động gán tiền tố thời gian `YYYYMMDD_HHMMSS_` tránh ghi đè file.
   - Stream dữ liệu multipart trực tiếp vào ổ cứng từng chunk (không nạp toàn bộ file vào RAM), nhận mượt mà các file/video lớn tới **10GB+**.
   - Lưu trữ tự động vào thư mục `Downloads/ViDrop` (tự động tạo thư mục nếu chưa có).
5. **Giao diện Web di động (`GET /`):**
   - Tối ưu cho cả iOS Safari & Android Chrome với hỗ trợ Light mode / Dark mode tương thích hệ thống.
   - Chọn nhiều file cùng lúc (`multiple`), kéo thả file.
   - Hiển thị thanh tiến trình tải lên kèm phần trăm (`XMLHttpRequest`), dung lượng đã truyền và tốc độ truyền (MB/s).

---

## 🛠 Hướng dẫn phát triển & Chạy ứng dụng

### 1. Khởi chạy ở chế độ Development
```bash
npm run dev
# hoặc
npx tauri dev
```

### 2. Biên dịch bản đóng gói hoàn chỉnh
- **Chế độ Debug:**
  ```bash
  npm run tauri -- build --debug
  ```
- **Chế độ Release:**
  ```bash
  npm run tauri -- build
  ```
  File chạy sẽ nằm tại thư mục:
  - macOS: `src-tauri/target/release/bundle/macos/Vi Drop.app`
  - Windows: `src-tauri/target/release/bundle/msi/` hoặc `nsis/`
