# Implementation Plan: Privacy Micro-Apps dan Burn Workspace

## Overview

Rencana ini mengimplementasikan Metadata Scrubber, Secret Redactor, JWT Inspector, dan Burn Workspace. Bahasa implementasi mengikuti design: Rust (crate `disposable-server` dan `disposable-common`) untuk backend dan TypeScript/React (Astro) untuk frontend.

Urutan kerja: fondasi (dependensi, DTO) -> logika inti sebagai fungsi murni di `privacy/` -> adaptor route HTTP -> tool MCP -> penghentian proses -> frontend -> deploy dan dokumentasi. Setiap langkah berakhir dengan keterhubungan ke langkah sebelumnya, tanpa kode yatim.

Konvensi pengujian:
- Seluruh tes properti Rust berada di `crates/server/src/privacy/tests/` (`#[cfg(test)] mod tests;` di `privacy/mod.rs`). Task 1.1 membuat `mod.rs` dan berkas kosong `prop_NN.rs`, `gen_*.rs`, dan `unit_*.rs` agar tiap task pengujian hanya mengisi berkasnya sendiri.
- Setiap tes properti memakai `proptest` (minimal 100 iterasi) dengan komentar tag `// Feature: privacy-micro-apps-and-burn-workspace, Property N: <teks>`.
- Tes properti frontend memakai `fast-check` + `vitest` (`numRuns: 100`) di `frontend/src/lib/*.test.ts`.

## Tasks

- [x] 1. Fondasi proyek
  - [x] 1.1 Tambah dependensi dan kerangka modul `privacy/`
    - Tambah `lopdf`, `regex`, `hmac`, `zeroize` (versi dipin) dan dev-dependency `proptest`, `tempfile` di `crates/server/Cargo.toml`
    - Buat `crates/server/src/privacy/mod.rs` (submodul `scrubber`, `redactor`, `jwt_inspector`, `burn`, dan `#[cfg(test)] mod tests;`) dan daftarkan `mod privacy;` di `main.rs`
    - Buat `privacy/tests/mod.rs` yang mendeklarasikan submodul `gen_scrub`, `gen_redact`, `gen_jwt`, `gen_burn`, `prop_01` s.d. `prop_07`, `prop_10` s.d. `prop_24`, `prop_26` s.d. `prop_31`, `prop_33`, `unit_scrub`, `unit_routes`, `unit_mcp`, `unit_burn`, `smoke`, beserta berkas kosong untuk masing-masing
    - Tambah `workspace_tmp_root()` di `state.rs` (`std::env::temp_dir().join("disposable-workspace")`) dan arahkan direktori `pandoc-job-*` di `routes/pandoc.rs` ke root tersebut
    - _Requirements: 3.2, 14.1, 14.2_

  - [x] 1.2 Tambah DTO bersama di `crates/common/src/lib.rs`
    - `ScrubCategoryCount`, `ScrubReport`, `ScrubEphemeralResponse`
    - `RedactRequest`, `RedactResponse`
    - `JwtInspectRequest`, `ClaimTime`, `TokenStatus`, `SignatureCheck`, `JwtInspectResult`
    - `CategoryStat`, `FailedItem`, `AuditReport` (derive `Debug, Clone, Serialize, Deserialize, PartialEq`; enum `snake_case`)
    - _Requirements: 1.3, 2.1, 5.3, 6.1, 11.3_

- [x] 2. Inti Metadata Scrubber (fungsi murni)
  - [x] 2.1 Definisikan tipe dan deteksi format di `privacy/scrubber/mod.rs`
    - `MAX_UPLOAD_BYTES`, `FileKind`, `ScrubError`, `ScrubOutput`, dan `sniff(bytes)` berbasis magic bytes (bukan nama file atau Content-Type)
    - Kerangka tanda tangan `scrub` dan `scan`
    - _Requirements: 1.5, 1.7, 2.5, 2.6_

  - [x] 2.2 Implementasikan scrubber JPEG di `privacy/scrubber/jpeg.rs`
    - Iterasi segmen SOI sampai SOS; hapus APP1 (EXIF/XMP), APP13, APP12, APP15, COM; pertahankan APP0, APP2, DQT/SOF/DHT/DRI/SOS dan data entropy
    - Parser TIFF-IFD minimal untuk menghitung kategori (gps, device_serial, author, software, thumbnail, xmp, iptc) tanpa menyimpan nilai
    - Kembalikan `Unreadable` untuk JPEG terpotong atau tidak valid
    - _Requirements: 1.1, 1.2, 1.3, 1.6, 1.9_

  - [x] 2.3 Implementasikan scrubber PNG di `privacy/scrubber/png.rs`
    - Validasi signature dan CRC; hapus `eXIf`, `tEXt`, `zTXt`, `iTXt`, `tIME`; pertahankan chunk lain termasuk IDAT tanpa perubahan
    - Hitung `iTXt` berkeyword `XML:com.adobe.xmp` sebagai kategori XMP
    - _Requirements: 1.1, 1.2, 1.3, 1.6, 1.9_

  - [x] 2.4 Implementasikan scrubber PDF di `privacy/scrubber/pdf.rs`
    - Periksa header `%PDF-`, muat dengan `lopdf::Document::load_mem`, tolak PDF terenkripsi (`EncryptedPdf`)
    - Hapus `/Info` dari trailer beserta objeknya dan `/Metadata` dari katalog/halaman, `prune_objects`, serialisasi deterministik; catat nama field yang dihapus
    - Pastikan serialisasi tidak menyisipkan `/Producer` atau ID baru; bila `lopdf` tidak deterministik, tulis ulang trailer manual
    - _Requirements: 2.1, 2.2, 2.3, 2.4, 2.5_

  - [x] 2.5 Hubungkan `scrub` dan `scan` pada `privacy/scrubber/mod.rs`
    - `scrub` memanggil `sniff` lalu modul format, mengembalikan `ScrubOutput` dengan `ScrubReport` (hanya kategori `count > 0`)
    - `scan` menghitung sisa Sensitive_Metadata pada file hasil
    - Tolak > 26.214.400 byte dengan `TooLarge` sebelum parse
    - _Requirements: 1.1, 1.3, 1.7, 2.1, 2.6, 3.1_

  - [x] 2.6 Tulis generator file sintetis di `privacy/tests/gen_scrub.rs`
    - Generator JPEG (APP0, APP1 EXIF dengan tag GPS/serial/owner/software/thumbnail, XMP, APP13, COM, DQT/SOF/SOS, EOI) dan PNG (IHDR/IDAT/IEND dengan CRC valid + chunk metadata acak)
    - Generator PDF dengan `lopdf` (N halaman, teks acak, Info dictionary subset acak, XMP opsional)
    - Generator korupsi (potong, ubah header, rusak CRC) dan byte acak non-format
    - _Requirements: 1.1, 2.1_

  - [x] 2.7 Tulis property test untuk scrub gambar
    - **Property 1: Scrub gambar menghapus metadata dan mempertahankan piksel**
    - Berkas: `privacy/tests/prop_01.rs`
    - **Validates: Requirements 1.1, 1.2, 1.9**

  - [x] 2.8 Tulis property test untuk akurasi Scrub_Report
    - **Property 2: Scrub_Report akurat**
    - Berkas: `privacy/tests/prop_02.rs`
    - **Validates: Requirements 1.3, 2.1**

  - [x] 2.9 Tulis property test untuk idempotensi scrub
    - **Property 3: Idempotensi scrub**
    - Berkas: `privacy/tests/prop_03.rs`
    - **Validates: Requirements 1.8, 2.4**

  - [x] 2.10 Tulis property test untuk struktur dan konten PDF
    - **Property 4: Scrub PDF mempertahankan struktur dan konten**
    - Berkas: `privacy/tests/prop_04.rs`
    - **Validates: Requirements 2.1, 2.2**

  - [x] 2.11 Tulis unit test inti scrubber
    - Fixture PDF terenkripsi menghasilkan `EncryptedPdf`; batas 26.214.400 diterima dan 26.214.401 `TooLarge`; file 0 byte `Unreadable`
    - Berkas: `privacy/tests/unit_scrub.rs`
    - _Requirements: 1.6, 1.7, 2.3, 2.5, 2.6_

- [x] 3. Inti Secret Redactor (fungsi murni)
  - [x] 3.1 Implementasikan `privacy/redactor.rs`
    - Tipe `Category`, `Format`, `RedactOptions`, `RedactOutcome`, konstanta batas, `validate` (400: > 100 kata, kata kosong, > 256 karakter per `char`)
    - `redact`: span terlindungi untuk placeholder yang sudah ada, mode `.env` (auto/env/text), aturan teks (JWT, bearer, API key, email, IPv4, IPv6, telepon, kartu kredit + Luhn, custom dengan `regex::escape`), resolusi tumpang tindih greedy `(start asc, panjang desc, prioritas)`
    - Penjagaan jumlah baris untuk kata kustom berisi `\n`; `counts` bernilai 0 untuk kategori aktif tanpa kecocokan
    - Tidak ada logging masukan atau kata kustom
    - _Requirements: 5.1, 5.2, 5.3, 5.4, 5.5, 5.7, 5.9, 5.10, 5.11_

  - [x] 3.2 Tulis generator teks redaksi di `privacy/tests/gen_redact.rs`
    - Kalimat acak dengan rahasia disisipkan, kartu kredit lolos Luhn, berkas `.env` acak, kata kustom acak (termasuk karakter regex-meta, `REDACTED`, `\n`), konfigurasi kategori acak
    - _Requirements: 5.1, 5.2, 5.5_

  - [x] 3.3 Tulis property test redaksi kategori aktif dan penghitungan
    - **Property 10: Redaksi mengganti semua rahasia kategori aktif dan menghitungnya**
    - Berkas: `privacy/tests/prop_10.rs`
    - **Validates: Requirements 5.1, 5.3**

  - [x] 3.4 Tulis property test struktur `.env`
    - **Property 11: Redaksi `.env` mempertahankan struktur**
    - Berkas: `privacy/tests/prop_11.rs`
    - **Validates: Requirements 5.2**

  - [x] 3.5 Tulis property test kategori nonaktif
    - **Property 12: Kategori nonaktif tidak disentuh**
    - Berkas: `privacy/tests/prop_12.rs`
    - **Validates: Requirements 5.4**

  - [x] 3.6 Tulis property test kata kustom
    - **Property 13: Kata kustom diganti dengan pencocokan sensitif huruf besar/kecil**
    - Berkas: `privacy/tests/prop_13.rs`
    - **Validates: Requirements 5.5**

  - [x] 3.7 Tulis property test validasi kata kustom
    - **Property 14: Validasi daftar kata kustom**
    - Berkas: `privacy/tests/prop_14.rs`
    - **Validates: Requirements 5.7**

  - [x] 3.8 Tulis property test idempotensi redaksi
    - **Property 15: Idempotensi redaksi**
    - Berkas: `privacy/tests/prop_15.rs`
    - **Validates: Requirements 5.9**

  - [x] 3.9 Tulis property test jumlah baris
    - **Property 16: Redaksi mempertahankan jumlah baris**
    - Berkas: `privacy/tests/prop_16.rs`
    - **Validates: Requirements 5.10**

- [x] 4. Inti JWT Inspector (fungsi murni)
  - [x] 4.1 Implementasikan `privacy/jwt_inspector.rs`
    - `inspect(token, opts, now_unix)`: tepat 3 segmen, dekode base64url (`URL_SAFE_NO_PAD`) + `serde_json`, galat menyebut segmen (`header`/`payload`/`signature`) tanpa hasil parsial, batas 1..=8192 karakter dan secret 1..=1024 byte
    - Klaim waktu `exp`/`iat`/`nbf` ke ISO 8601 UTC atau nilai asli dengan `valid=false`; status `expired` didahulukan, lalu `not_yet_valid`, lalu `active`
    - Peringatan `alg: none` (tanpa membedakan huruf besar/kecil); verifikasi HMAC HS256/384/512 dengan `verify_slice`; algoritma lain -> `unsupported`
    - Redaksi klaim identitas rekursif (`email`, `phone_number`, `phone`, `sub`, `name`, `preferred_username`); `header_pretty`/`payload_pretty` berindentasi 2 spasi
    - Tidak ada logging token, payload, atau secret
    - _Requirements: 6.1, 6.2, 6.3, 6.4, 6.5, 6.6, 6.7, 6.8, 6.9, 6.11, 6.12_

  - [x] 4.2 Tulis generator JWT di `privacy/tests/gen_jwt.rs`
    - Header/payload JSON acak (kedalaman <= 5), klaim waktu acak, kunci identitas pada kedalaman acak, tanda tangan HMAC sungguhan HS256/384/512, perusak segmen
    - _Requirements: 6.1, 6.5, 6.7_

  - [x] 4.3 Tulis property test round trip JWT
    - **Property 17: Round trip inspeksi JWT**
    - Berkas: `privacy/tests/prop_17.rs`
    - **Validates: Requirements 6.1, 6.12**

  - [x] 4.4 Tulis property test klaim waktu dan status
    - **Property 18: Klaim waktu dan status token**
    - Berkas: `privacy/tests/prop_18.rs`
    - **Validates: Requirements 6.2, 6.3**

  - [x] 4.5 Tulis property test peringatan `alg: none`
    - **Property 19: Peringatan algoritma none**
    - Berkas: `privacy/tests/prop_19.rs`
    - **Validates: Requirements 6.4**

  - [x] 4.6 Tulis property test token cacat
    - **Property 20: Token cacat ditolak dengan menyebut segmen**
    - Berkas: `privacy/tests/prop_20.rs`
    - **Validates: Requirements 6.5, 6.6**

  - [x] 4.7 Tulis property test verifikasi HMAC
    - **Property 21: Verifikasi tanda tangan HMAC**
    - Berkas: `privacy/tests/prop_21.rs`
    - **Validates: Requirements 6.7, 6.8**

  - [x] 4.8 Tulis property test redaksi klaim identitas
    - **Property 22: Redaksi klaim identitas JWT**
    - Berkas: `privacy/tests/prop_22.rs`
    - **Validates: Requirements 6.9**

- [x] 5. Checkpoint - Inti scrubber, redactor, dan JWT
  - Ensure all tests pass, ask the user if questions arise.

- [x] 6. Inti Burn Service
  - [x] 6.1 Implementasikan primitif burn di `privacy/burn.rs`
    - Trait `StorageBackend` (`wipe_downloads`, `clear_mcp`, `clear_sessions`, `wipe_temp`, `measure`) beserta implementasi nyata atas `AppState` dan `workspace_tmp_root()`
    - `measure` dengan kategori tetap `sessions`, `downloads`, `mcp_sessions`, `temp_dir` dan aturan hitung byte sesuai design
    - `wipe_buffer` (nol pada seluruh panjang lewat `zeroize`), `restart_warning(policy)` (peringatan untuk `always`, `unless-stopped`, atau tidak dikenal/tidak ada)
    - _Requirements: 10.1, 10.2, 10.5, 11.1, 12.3_

  - [x] 6.2 Implementasikan `BurnCoordinator` dan integrasi `AppState`
    - `BurnCoordinator` (`AtomicBool burning`, `OnceCell<Arc<AuditReport>>`, `Notify`, `triggered_at`) dengan `run_or_join` single-flight, `is_burning`, `schedule_shutdown`, `shutdown_requested`
    - Langkah burn dengan isolasi kegagalan per item (`failed_items`, `incomplete`), `timeout(5s)` untuk `mcp_sessions` dan verifikasi, `verified=true` hanya bila semua entri dan byte `after` bernilai 0
    - Tambah field `burn: Arc<BurnCoordinator>` dan `ui_events: broadcast::Sender<Arc<AuditReport>>` pada `AppState` (dibuat di `AppState::new()`); kirim `AuditReport` ke `ui_events` dalam 2 detik sejak mulai; isi `warnings` dari env `DISPOSABLE_RESTART_POLICY`
    - _Requirements: 10.1, 10.2, 10.3, 10.4, 10.5, 10.7, 10.8, 10.9, 11.1, 11.2, 11.3, 11.4, 11.5, 11.6, 12.3, 13.5_

  - [x] 6.3 Tulis generator state burn di `privacy/tests/gen_burn.rs`
    - `AppState` dengan jumlah acak sesi/unduhan/sesi MCP, pohon file di `tempfile::tempdir`, penanda unik pada nama file/token/isi, dan `FakeBackend` yang dapat gagal atau meninggalkan residu
    - _Requirements: 10.8, 11.5, 11.6_

  - [x] 6.4 Tulis property test pengosongan Managed_Storage
    - **Property 26: Burn mengosongkan seluruh Managed_Storage dan membuktikannya**
    - Berkas: `privacy/tests/prop_26.rs`
    - **Validates: Requirements 10.1, 10.2, 10.3, 10.4, 10.5, 11.1, 11.2, 12.5**

  - [x] 6.5 Tulis property test penimpaan buffer
    - **Property 27: Penimpaan buffer dengan nol**
    - Berkas: `privacy/tests/prop_27.rs`
    - **Validates: Requirements 10.2**

  - [x] 6.6 Tulis property test single-flight
    - **Property 28: Burn single-flight**
    - Berkas: `privacy/tests/prop_28.rs`
    - **Validates: Requirements 10.7**

  - [x] 6.7 Tulis property test kegagalan sebagian
    - **Property 30: Burn tahan kegagalan sebagian dan tidak pernah memalsukan verifikasi**
    - Berkas: `privacy/tests/prop_30.rs`
    - **Validates: Requirements 10.8, 11.5, 11.6**

  - [x] 6.8 Tulis property test invarian dan privasi AuditReport
    - **Property 31: Invarian dan privasi AuditReport**
    - Berkas: `privacy/tests/prop_31.rs`
    - **Validates: Requirements 11.3, 11.4**

  - [x] 6.9 Tulis property test peringatan restart policy
    - **Property 33: Peringatan restart policy**
    - Berkas: `privacy/tests/prop_33.rs`
    - **Validates: Requirements 12.3**

- [x] 7. Adaptor route HTTP
  - [x] 7.1 Implementasikan `routes/scrubber.rs`
    - `POST /api/apps/scrubber` dengan `AuthenticatedSession` sebagai argumen pertama (401 sebelum body dibaca), `DefaultBodyLimit` 26 MiB, urutan validasi ukuran 413 -> tipe 415 -> parse 422
    - Mode `direct` (header `Content-Disposition: attachment`, `Cache-Control: no-store`, `X-Scrub-Report` base64url) dan `ephemeral` (simpan ke `downloads` TTL 300 detik, JSON `ScrubEphemeralResponse`)
    - `scrub` dijalankan lewat `spawn_blocking`, tangkap `JoinError` menjadi 422 generik; buffer dilepas saat respons selesai atau future di-drop; tanpa logging nama file/isi
    - `GET /api/apps/scrubber/download/:token` memakai ulang handler unduhan pandoc dengan tambahan `Cache-Control: no-store`
    - _Requirements: 1.1, 1.4, 1.5, 1.6, 1.7, 2.7, 3.1, 3.3, 3.4, 3.5, 3.6, 3.7, 3.8, 3.9_

  - [x] 7.2 Implementasikan `routes/redactor.rs` dan `routes/jwt.rs`
    - `POST /api/apps/redactor`: 401, 413 untuk teks > 5.242.880 byte, 400 dari `validate`, pemetaan `RedactRequest` ke `RedactOptions`, `DefaultBodyLimit` 16 MiB
    - `POST /api/apps/jwt`: 401, 422 untuk galat, token dan secret dibungkus `Zeroizing` dan dihapus sebelum respons dikembalikan, `now` dari jam server
    - Tanpa logging masukan pada jalur sukses maupun galat
    - _Requirements: 5.1, 5.6, 5.7, 5.8, 5.11, 6.1, 6.5, 6.6, 6.10, 6.11, 7.10_

  - [x] 7.3 Implementasikan `routes/burn.rs` dan middleware `burn_gate`
    - `POST /api/burn` (401 tanpa sesi), memanggil `run_or_join`, mengembalikan `AuditReport` dengan `Set-Cookie` penghapusan (pola `logout`), body dibungkus stream yang memberi sinyal saat habis atau di-drop untuk `schedule_shutdown`
    - `GET /api/burn/events` (SSE terautentikasi) meneruskan `ui_events` sebagai `event: burn`
    - `burn_gate`: saat `is_burning()` kembalikan 503 untuk `/api/apps/*` dan `/api/mcp/*` tanpa membaca body; kecualikan `/api/burn`, `/api/burn/events`, `/api/auth/status`
    - _Requirements: 9.5, 10.4, 10.6, 10.7, 10.9, 12.1, 12.4, 13.5_

  - [x] 7.4 Daftarkan rute dan middleware di `routes/mod.rs`
    - Tambah `pub mod scrubber/redactor/jwt/burn`, daftarkan semua rute baru di `create_api_router`, pasang `burn_gate` sebagai layer terluar relatif terhadap auth
    - _Requirements: 14.3, 10.6_

  - [x] 7.5 Tulis property test penolakan masukan tidak valid
    - **Property 5: Input tidak valid ditolak tanpa keluaran atau penyimpanan**
    - Mencakup `scrub` dan handler (tidak ada entri baru di `downloads`, tidak ada file di temp dir)
    - Berkas: `privacy/tests/prop_05.rs`
    - **Validates: Requirements 1.5, 1.6, 2.5, 3.8**

  - [x] 7.6 Tulis property test Ephemeral_Download scrubber
    - **Property 6: Ephemeral_Download scrubber bersifat sekali pakai dan bertuntun**
    - Mencakup REST ephemeral dan tool `scrub_metadata`
    - Berkas: `privacy/tests/prop_06.rs`
    - **Validates: Requirements 3.7, 3.9, 8.4**

  - [x] 7.7 Tulis property test tidak ada konten pengguna di log
    - **Property 7: Tidak ada konten pengguna di log**
    - Tangkap keluaran `tracing` selama pemrosesan sukses dan gagal untuk scrubber, redactor, dan JWT
    - Berkas: `privacy/tests/prop_07.rs`
    - **Validates: Requirements 3.6, 5.11, 6.11**

  - [x] 7.8 Tulis property test gate 503 saat burning
    - **Property 29: Gate 503 selama burning**
    - Berkas: `privacy/tests/prop_29.rs`
    - **Validates: Requirements 10.6**

  - [x] 7.9 Tulis unit test route
    - 401 tanpa auth untuk `/api/apps/scrubber`, `/api/apps/redactor`, `/api/apps/jwt`, `/api/burn`
    - Batas persis: scrubber 26.214.400 vs 26.214.401, redactor 5.242.880 vs 5.242.881, token 8.192 vs 8.193
    - Header `Content-Disposition`/`Cache-Control`; TempDirGuard bersih setelah sukses, gagal, dan drop future; jalur sukses dan gagal untuk tiap fitur
    - Berkas: `privacy/tests/unit_routes.rs` dan `privacy/tests/unit_burn.rs`
    - _Requirements: 1.4, 1.7, 2.6, 2.7, 3.2, 3.3, 5.6, 5.8, 6.6, 7.10, 9.5, 14.2, 14.4_

- [x] 8. Integrasi MCP
  - [x] 8.1 Siapkan kerangka tool baru di `routes/mcp.rs`
    - Tambah 4 tool ke `tools/list` (`redact_secrets`, `inspect_jwt`, `scrub_metadata`, `burn_workspace_now` dengan `title: "Burn Workspace Now"` dan deskripsi permanen/tidak dapat dibatalkan) tanpa mengubah tool lama
    - Perluas cabang `tools/call` agar dapat menghasilkan objek JSON-RPC `error` (-32602 untuk argumen tidak valid dengan menyebut nama argumen, -32603 untuk kegagalan burn) sementara tool lama tetap memakai `result.isError`
    - _Requirements: 8.1, 8.6, 13.1, 13.3_

  - [x] 8.2 Implementasikan tool `redact_secrets`, `inspect_jwt`, dan `scrub_metadata`
    - Panggil fungsi murni yang sama dengan REST; validasi argumen sebelum eksekusi (`text` 1..1.048.576, `token` 1..8.192, `file_base64` hasil dekode 1..26.214.400)
    - `scrub_metadata` menyimpan Ephemeral_Download TTL 300 detik dan mengembalikan URL unduhan + `ScrubReport`; tidak menyimpan apa pun bila argumen tidak valid
    - _Requirements: 8.2, 8.3, 8.4, 8.6, 8.7_

  - [x] 8.3 Implementasikan tool `burn_workspace_now` termasuk jalur SSE
    - Wajibkan `confirm` boolean `true` (string "true" ditolak dengan -32602); panggil `run_or_join` tepat sekali
    - Untuk jalur SSE: klon `Sender` dari `mcp_sessions[sid]` sebelum burn, kirim event hasil lengkap lewat klon, lalu drop agar stream berakhir setelah event terkirim; tunggu `Sender::closed()` dengan batas 5 detik sebelum `schedule_shutdown`
    - Burn yang sedang berjalan menghasilkan galat JSON-RPC "penghancuran sedang berlangsung"; kegagalan burn -> -32603 dan proses tetap berjalan
    - _Requirements: 8.5, 13.2, 13.3, 13.4, 13.6, 13.7_

  - [x] 8.4 Tulis property test paritas MCP dan REST
    - **Property 23: Paritas MCP dan REST**
    - Berkas: `privacy/tests/prop_23.rs`
    - **Validates: Requirements 8.2, 8.3**

  - [x] 8.5 Tulis property test argumen MCP tidak valid
    - **Property 24: Argumen tool MCP tidak valid ditolak tanpa efek samping**
    - Berkas: `privacy/tests/prop_24.rs`
    - **Validates: Requirements 8.6, 13.3**

  - [x] 8.6 Tulis unit test MCP
    - `tools/list` memuat 4 tool baru dan 2 tool lama tidak berubah; 401 tanpa sesi; burn via SSE (event lengkap sebelum stream tutup); event `burn` ke langganan UI; kegagalan burn -> -32603; burn kedua saat berjalan -> galat "sedang berlangsung"
    - Berkas: `privacy/tests/unit_mcp.rs`
    - _Requirements: 8.1, 8.7, 13.1, 13.2, 13.4, 13.5, 13.6, 13.7_

- [x] 9. Penghentian proses server
  - [x] 9.1 Ubah `main.rs` untuk shutdown setelah burn
    - `tokio::select!` antara `axum::serve(...).with_graceful_shutdown(shutdown_signal())` dan `state.burn.shutdown_requested()`; cabang kedua mengembalikan `Ok(())` (exit 0)
    - Watchdog `std::thread::spawn`: setelah shutdown diminta tidur 3 detik, tulis kegagalan ke stderr, `std::process::exit(1)`
    - Timer 5 detik sejak pemicuan selalu memanggil `notify_one()` (klien terputus); tambah hook tes (env var khusus tes) yang menyumbat jalur shutdown normal untuk menguji watchdog
    - _Requirements: 12.1, 12.4, 12.5, 12.6_

  - [x] 9.2 Tulis tes integrasi proses anak
    - Jalankan binary server pada port acak, picu `POST /api/burn`, harapkan exit 0 dalam 2 detik; skenario klien terputus exit 0 dalam 5 detik; skenario hook penyumbat exit 1 dan pesan di stderr
    - Berkas: `crates/server/tests/burn_shutdown.rs` (gunakan `CARGO_BIN_EXE_<nama-binary>` sesuai `Cargo.toml`)
    - _Requirements: 12.1, 12.4, 12.6_

- [x] 10. Checkpoint - Backend lengkap
  - Ensure all tests pass, ask the user if questions arise.

- [x] 11. Pustaka logika frontend
  - [x] 11.1 Siapkan vitest dan fungsi murni di `frontend/src/lib/`
    - Tambah dev-dependency `vitest`, `fast-check`, `@testing-library/react`, `jsdom` (versi dipin) dan skrip `test` (`vitest --run`) di `frontend/package.json`
    - Tambah tipe TS `AuditReport`, `ScrubReport`, `JwtInspectResult` di `frontend/src/types/`
    - Implementasikan `formatFileSize`, `pickFirstFile`, `validateRedactorInput` (batas 1.048.576 karakter), `shouldShowZeroBytes`
    - _Requirements: 4.3, 4.4, 7.5, 11.7, 11.8_

  - [x] 11.2 Tulis property test format ukuran file
    - **Property 8: Format ukuran file**
    - Berkas: `frontend/src/lib/formatFileSize.test.ts`
    - **Validates: Requirements 4.3**

  - [x] 11.3 Tulis property test pemilih file pertama
    - **Property 9: Hanya file pertama yang diterima**
    - Berkas: `frontend/src/lib/pickFirstFile.test.ts`
    - **Validates: Requirements 4.4**

  - [x] 11.4 Tulis property test validasi masukan redactor
    - **Property 25: Validasi masukan redactor di klien**
    - Berkas: `frontend/src/lib/validateRedactorInput.test.ts`
    - **Validates: Requirements 7.5**

  - [x] 11.5 Tulis property test indikator "0 bytes remaining"
    - **Property 32: Indikator "0 bytes remaining" mengikuti verified**
    - Berkas: `frontend/src/lib/shouldShowZeroBytes.test.ts`
    - **Validates: Requirements 11.7, 11.8**

- [x] 12. Komponen dan halaman frontend
  - [x] 12.1 Implementasikan `MetadataScrubber.tsx` dan `pages/scrubber.astro`
    - Dropzone/input file, tampil nama dan ukuran sebelum upload, hanya file pertama + pemberitahuan bila > 1
    - `fetch` mode `direct` dengan `AbortController` timeout 60 detik; tampil Scrub_Report dari header `X-Scrub-Report`; unduhan via `Blob` + `URL.createObjectURL` (di-revoke) dalam 2 detik
    - Galat: tampil pesan server atau pesan umum, file terpilih dipertahankan, kontrol aktif lagi dalam 1 detik; hanya token desain Tailwind, tanpa heksadesimal
    - _Requirements: 4.1, 4.3, 4.4, 4.5, 4.6, 4.7_

  - [x] 12.2 Implementasikan `JwtInspectorTab.tsx` dan `LogRedactorTab.tsx`
    - JWT: input token, secret HMAC opsional, opsi redaksi; tampil header/payload terformat, waktu ISO, status, peringatan, hasil verifikasi
    - Redactor: textarea (batas 1.048.576 dengan pesan galat dan teks dipertahankan), toggle kategori, kata kustom, dua panel berdampingan pada `md:` (>= 768 px), tombol salin dengan konfirmasi 2 detik dan pesan galat bila clipboard gagal
    - _Requirements: 6.1, 7.4, 7.5, 7.6, 7.7_

  - [x] 12.3 Implementasikan `RedactorPage.tsx` dan `pages/redactor.astro`
    - Tepat dua tab ("JWT Inspector" default, "Log & .env Redactor"), pergantian tab < 200 ms tanpa reload
    - Cleanup state saat unmount, `beforeunload`, `pagehide`; tidak menyentuh `localStorage`, `sessionStorage`, atau cookie
    - _Requirements: 7.1, 7.2, 7.3, 7.8, 7.9_

  - [x] 12.4 Implementasikan `BurnButton.tsx`, `BurnConfirmDialog.tsx`, `BurnOverlay.tsx`, `AuditScreen.tsx`
    - Tombol dengan label teks terlihat dan `aria-label` aksi burn, warna token `airbnb-error`; dialog (Setuju/Batal/Escape, muncul <= 200 ms); opsi Demo mode dengan hitung mundur 3-2-1 dan Batal
    - Tepat satu `POST /api/burn` per aksi, tombol dinonaktifkan setelah kirim, timeout 10 detik, pesan galat dan tombol aktif lagi bila gagal
    - Overlay animasi 1-10 detik lalu layar audit: tabel sebelum/sesudah per kategori, durasi, waktu UTC, peringatan, indikator "0 bytes remaining" hanya lewat `shouldShowZeroBytes`, byte dan kategori tersisa bila `verified=false`, pernyataan cakupan Managed_Storage; berlangganan `EventSource` `/api/burn/events`
    - _Requirements: 9.1, 9.2, 9.3, 9.4, 9.6, 9.7, 9.8, 11.7, 11.8, 11.9, 11.10, 13.5_

  - [x] 12.5 Hubungkan navigasi di `AppRouter.tsx`, `Navbar.tsx`, `DashboardView.tsx`
    - `AppRouter`: tambah `'scrubber' | 'redactor'` ke `page` dan `PAGE_PATH`, render komponen terkait
    - `Navbar`: dua tautan baru dengan gaya aktif yang sama dan `BurnButton`; `DashboardView`: dua kartu layanan baru
    - Pasang `BurnOverlay` agar burn yang dipicu MCP menampilkan layar audit
    - _Requirements: 4.1, 7.1, 9.1, 13.5_

  - [x] 12.6 Tulis tes komponen Scrubber
    - Tampil nama/ukuran sebelum upload, file pertama saja, laporan dan unduhan sukses, galat mempertahankan file dan mengaktifkan kontrol
    - Berkas: `frontend/src/components/MetadataScrubber.test.tsx`
    - _Requirements: 4.3, 4.4, 4.5, 4.6_

  - [x] 12.7 Tulis tes komponen Redactor dan JWT
    - Dua tab dan default, pergantian tab, cleanup state saat unmount, spy `localStorage`/`sessionStorage`/`document.cookie` tidak dipanggil, salin berhasil/gagal, penolakan > 1.048.576 karakter
    - Berkas: `frontend/src/components/RedactorPage.test.tsx`
    - _Requirements: 7.2, 7.3, 7.5, 7.6, 7.7, 7.8, 7.9_

  - [x] 12.8 Tulis tes komponen Burn
    - Dialog (Setuju kirim tepat satu `POST`, Batal/Escape tidak kirim), Demo mode 3-2-1 dan pembatalan, galat/timeout, layar audit verified true/false, pernyataan cakupan
    - Berkas: `frontend/src/components/BurnButton.test.tsx`
    - _Requirements: 9.2, 9.3, 9.4, 9.6, 9.7, 9.8, 11.7, 11.8, 11.9, 11.10_

- [x] 13. Deploy, dokumentasi, dan verifikasi kepatuhan
  - [x] 13.1 Parametrisasi restart policy di `deploy/docker-compose.yml`
    - `restart: ${RESTART_POLICY:-on-failure}` dan `DISPOSABLE_RESTART_POLICY=${RESTART_POLICY:-on-failure}` pada `environment`
    - Perubahan default dari `unless-stopped` mengubah perilaku deployment yang ada; sebutkan di ringkasan akhir sebagai butuh persetujuan pengguna (Pertanyaan Terbuka 1 di design)
    - _Requirements: 12.2, 12.3_

  - [x] 13.2 Perbarui `README.md` dan `AGENTS.md`
    - Untuk tiap fitur baru: deskripsi micro-app, rute API (termasuk `/api/burn/events`), nama tool MCP, perilaku Burn Workspace, dan catatan `RESTART_POLICY`
    - _Requirements: 14.6_

  - [x] 13.3 Tulis smoke test kepatuhan
    - Semua rute baru tidak menghasilkan 404 pada `create_api_router`; compose (`include_str!`) memuat default `on-failure` dan `DISPOSABLE_RESTART_POLICY` selaras; `README.md`/`AGENTS.md` memuat nama rute dan tool; tidak ada literal heksadesimal warna pada komponen baru `MetadataScrubber`, `RedactorPage`, `JwtInspectorTab`, `LogRedactorTab`, `Burn*`, `AuditScreen`
    - Pastikan `crates/cli` (`teardown`) tidak mengimpor atau memanggil Burn_Service
    - Berkas: `privacy/tests/smoke.rs`
    - _Requirements: 4.7, 12.2, 14.3, 14.6, 14.7_

- [x] 14. Final checkpoint - Ensure all tests pass
  - Ensure all tests pass, ask the user if questions arise.

## Notes

- Tugas bertanda `*` bersifat opsional (pengujian) dan dapat dilewati untuk MVP lebih cepat.
- Setiap properti dari design menjadi satu sub-tugas tersendiri; Property 8, 9, 25, 32 berada di sisi frontend.
- Req 6.10 (penghapusan memori dalam 1 detik) hanya dapat dijamin sebagai upaya terbaik lewat `Zeroizing` dan tinjauan kode; Req 11.10 membatasi cakupan verifikasi pada Managed_Storage.
- Pertanyaan Terbuka di design (default restart policy, prioritas status JWT, parameter `format`, endpoint `/api/burn/events`, determinisme PDF) perlu dikonfirmasi pengguna; task 13.1 mengubah default compose.
- Tes Rust dijalankan dengan `cargo test --workspace`; tes frontend dengan `npm test` di `frontend/` (`vitest --run`).

## Task Dependency Graph

```json
{
  "waves": [
    { "id": 0, "tasks": ["1.1", "1.2", "11.1", "13.1"] },
    { "id": 1, "tasks": ["2.1", "3.1", "4.1", "6.1", "11.2", "11.3", "11.4", "11.5", "12.1", "12.2", "12.4"] },
    { "id": 2, "tasks": ["2.2", "2.3", "2.4", "2.6", "3.2", "4.2", "6.2", "12.3"] },
    { "id": 3, "tasks": ["2.5", "3.3", "3.4", "3.5", "3.6", "3.7", "3.8", "3.9", "4.3", "4.4", "4.5", "4.6", "4.7", "4.8", "6.3", "7.2", "7.3", "8.1", "9.1", "12.5"] },
    { "id": 4, "tasks": ["2.7", "2.8", "2.9", "2.10", "2.11", "6.4", "6.5", "6.6", "6.7", "6.8", "6.9", "7.1", "8.2", "12.6", "12.7", "12.8"] },
    { "id": 5, "tasks": ["7.4", "8.3"] },
    { "id": 6, "tasks": ["7.5", "7.6", "7.7", "7.8", "7.9", "8.4", "8.5", "8.6", "9.2", "13.2"] },
    { "id": 7, "tasks": ["13.3"] }
  ]
}
```
