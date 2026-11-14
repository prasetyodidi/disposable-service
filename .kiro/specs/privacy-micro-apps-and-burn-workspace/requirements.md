# Requirements Document

## Introduction

Fitur ini menambahkan dua micro-app privasi baru ke Disposable Service Workspace dan satu fitur penghancuran darurat ("Burn Workspace"). Micro-app pertama adalah Metadata Scrubber yang menghapus metadata sensitif (koordinat GPS, nomor seri perangkat, nama penulis) dari file JPEG, PNG, dan PDF, lalu mengirim file bersih langsung ke browser. Micro-app kedua adalah Secret Redactor dan JWT Inspector yang mendekode token JWT serta menyamarkan rahasia pada teks `.env` dan log, tanpa mengirim data ke layanan pihak ketiga.

Fitur Burn Workspace menyediakan tombol mencolok di UI dan satu tool MCP bernama "Burn Workspace Now". Saat dipicu, server mengosongkan seluruh penyimpanan memori aplikasi, membersihkan direktori sementara, menampilkan bukti audit bahwa 0 byte tersisa pada penyimpanan yang dikelola aplikasi, lalu menghentikan proses server agar container mati.

Seluruh fitur tunduk pada invarian zero-footprint: tidak ada database persisten, tidak ada file yang tertinggal di disk, dan tidak ada konten pengguna yang masuk ke log.

## Glossary

- **Workspace**: Instans server Disposable Service (crate `disposable-server`) yang berjalan di dalam satu container Docker.
- **AppState**: Struktur state in-memory pada server yang menyimpan sesi login, unduhan sementara, dan sesi MCP.
- **Metadata_Scrubber**: Modul server dan halaman frontend yang menghapus metadata dari file yang diunggah.
- **Secret_Redactor**: Modul server dan halaman frontend yang menyamarkan data sensitif pada teks.
- **JWT_Inspector**: Modul server dan halaman frontend yang mendekode dan menganalisis token JWT.
- **Burn_Service**: Modul server yang menjalankan prosedur penghancuran Workspace.
- **Burn_Button**: Tombol UI mencolok yang memicu Burn_Service.
- **Burn_Tool**: Tool MCP bernama "Burn Workspace Now" yang memicu Burn_Service.
- **MCP_Server**: Endpoint MCP via SSE (`/api/mcp/sse` dan `/api/mcp/message`) pada Workspace.
- **Authenticated_Session**: Sesi pengguna dengan JWT cookie `HttpOnly` yang valid (masa berlaku 1 jam).
- **Supported_Image_Format**: Format JPEG atau PNG.
- **Scrub_Report**: Daftar kategori metadata yang ditemukan dan dihapus dari satu file, tanpa memuat nilai metadata tersebut.
- **Sensitive_Metadata**: Metadata berupa koordinat GPS, nomor seri perangkat atau lensa, nama pemilik kamera, nama penulis, aplikasi pembuat, thumbnail tertanam, dan XMP/IPTC; pada PDF berupa Info dictionary dan XMP stream.
- **Redaction_Rule**: Aturan deteksi data sensitif: JWT, bearer token, API key, nilai variabel `.env`, alamat email, nomor telepon, alamat IPv4/IPv6, dan nomor kartu kredit.
- **Redaction_Placeholder**: Teks pengganti berformat `[REDACTED:<kategori>]`.
- **Ephemeral_Download**: Entri unduhan in-memory dengan token UUID, TTL 5 menit, dan sifat burn-after-reading.
- **TempDirGuard**: RAII guard yang menghapus direktori sementara saat di-drop.
- **Audit_Report**: Dokumen JSON hasil Burn_Service yang memuat jumlah byte sebelum dan sesudah pembersihan, waktu, dan hasil verifikasi.
- **Managed_Storage**: Seluruh penyimpanan yang dikontrol aplikasi: `sessions`, `downloads`, `mcp_sessions` pada AppState, serta direktori sementara Workspace.
- **Burn_Confirmation**: Langkah konfirmasi eksplisit pengguna sebelum Burn_Service dijalankan.
- **Demo_Mode**: Opsi pada dialog Burn_Confirmation yang mengganti pengiriman langsung setelah "Setuju" dengan hitung mundur 3 detik yang dapat dibatalkan.

## Requirements

### Requirement 1: Pembersihan Metadata File Gambar

**User Story:** Sebagai jurnalis atau whistleblower, saya ingin menghapus metadata dari foto sebelum dipublikasikan, sehingga lokasi dan identitas perangkat saya tidak bocor.

#### Acceptance Criteria

1. WHEN pengguna terautentikasi mengunggah satu file dengan Supported_Image_Format dan berukuran 1 byte sampai 25 MB ke endpoint Metadata_Scrubber, THE Metadata_Scrubber SHALL mengembalikan satu file dengan format yang sama tanpa Sensitive_Metadata, dalam waktu paling lama 10 detik untuk file hingga 25 MB.
2. WHEN pembersihan file gambar selesai, THE Metadata_Scrubber SHALL mempertahankan nilai piksel (lebar, tinggi, dan nilai setiap piksel) identik dengan file asli setelah keduanya didekode.
3. WHEN pembersihan selesai, THE Metadata_Scrubber SHALL mengembalikan Scrub_Report yang berisi daftar kategori Sensitive_Metadata yang dihapus dan jumlah entri yang dihapus untuk setiap kategori. IF tidak ada Sensitive_Metadata yang ditemukan, THEN THE Metadata_Scrubber SHALL mengembalikan Scrub_Report dengan daftar kategori kosong.
4. IF pengguna yang tidak terautentikasi mengunggah file ke endpoint Metadata_Scrubber, THEN THE Metadata_Scrubber SHALL menolak permintaan dengan status HTTP 401 tanpa memproses atau menyimpan file.
5. IF file yang diunggah bukan Supported_Image_Format atau PDF, THEN THE Metadata_Scrubber SHALL mengembalikan status HTTP 415 dengan pesan galat yang menyebutkan bahwa tipe file tidak didukung dan tanpa menyimpan file tersebut.
6. IF file yang diunggah memiliki Supported_Image_Format atau PDF tetapi rusak sehingga tidak dapat di-parse, THEN THE Metadata_Scrubber SHALL mengembalikan status HTTP 422 dengan pesan galat yang menyebutkan bahwa file tidak dapat dibaca, tanpa mengembalikan file hasil parsial dan tanpa menyimpan file asli.
7. IF ukuran file melebihi 25 MB (26.214.400 byte), THEN THE Metadata_Scrubber SHALL menolak unggahan dengan status HTTP 413 dengan pesan galat yang menyebutkan batas ukuran 25 MB, tanpa memproses file.
8. FOR ALL file gambar valid dengan Supported_Image_Format, memproses hasil Metadata_Scrubber sekali lagi SHALL menghasilkan file yang identik byte demi byte dengan hasil pertama (idempotensi).
9. FOR ALL file gambar valid dengan Supported_Image_Format, pemindaian metadata pada file hasil SHALL menemukan 0 entri Sensitive_Metadata.

### Requirement 2: Pembersihan Metadata File PDF

**User Story:** Sebagai pengguna yang membagikan dokumen, saya ingin menghapus nama penulis dan properti dokumen dari PDF, sehingga identitas saya tidak terungkap.

#### Acceptance Criteria

1. WHEN pengguna terautentikasi mengunggah file PDF valid dengan ukuran 1 byte sampai 25 MB ke endpoint Metadata_Scrubber, THE Metadata_Scrubber SHALL menghasilkan PDF tanpa Info dictionary dan tanpa XMP metadata stream, serta mengembalikan Scrub_Report yang mencantumkan setiap field metadata yang dihapus (misalnya Author, Creator, Producer, Title, Subject, Keywords, CreationDate, ModDate) dalam waktu maksimal 10 detik untuk file sampai 25 MB.
2. WHEN pembersihan PDF selesai, THE Metadata_Scrubber SHALL menghasilkan PDF yang dapat dibuka oleh pembaca PDF standar tanpa error, dengan jumlah halaman yang sama dengan file asli dan teks yang dapat diekstrak pada setiap halaman identik dengan file asli.
3. IF PDF terenkripsi dengan kata sandi, THEN THE Metadata_Scrubber SHALL mengembalikan status HTTP 422 dengan pesan error yang menyatakan bahwa PDF terenkripsi tidak didukung, tanpa menyimpan file yang diunggah maupun menghasilkan file keluaran.
4. FOR ALL PDF yang valid, memproses hasil Metadata_Scrubber sekali lagi SHALL menghasilkan dokumen yang byte-nya identik dengan masukan kedua dan Scrub_Report dengan nol field metadata yang dihapus (idempotensi).
5. IF file yang diunggah bukan PDF valid (header `%PDF-` tidak ada, struktur rusak, atau ukuran 0 byte), THEN THE Metadata_Scrubber SHALL mengembalikan status HTTP 422 dengan pesan error yang menyatakan bahwa file bukan PDF valid, tanpa menyimpan file yang diunggah maupun menghasilkan file keluaran.
6. IF file yang diunggah berukuran lebih dari 25 MB, THEN THE Metadata_Scrubber SHALL menolak permintaan dengan status HTTP 413 dan pesan error yang menyatakan batas ukuran 25 MB terlampaui, tanpa menyimpan file yang diunggah.
7. IF pengguna tidak terautentikasi mengunggah file ke endpoint Metadata_Scrubber, THEN THE Metadata_Scrubber SHALL mengembalikan status HTTP 401 tanpa memproses file.

### Requirement 3: Pemrosesan Zero-Footprint dan Streaming Hasil Scrubber

**User Story:** Sebagai pengguna sadar privasi, saya ingin file diproses di memori dan langsung dikirim kembali ke browser, sehingga tidak ada salinan yang tertinggal di server.

#### Acceptance Criteria

1. THE Metadata_Scrubber SHALL memproses file yang diunggah sepenuhnya di memori tanpa menulis konten file ke disk, dengan batas ukuran file unggahan maksimum 25 MB per permintaan.
2. WHERE pemrosesan membutuhkan file sementara, THE Metadata_Scrubber SHALL menempatkan file tersebut hanya dalam direktori yang dikelola TempDirGuard dan menghapus direktori tersebut beserta seluruh isinya dalam waktu 1 detik setelah pemrosesan selesai, gagal, atau dibatalkan.
3. WHEN pembersihan berhasil, THE Metadata_Scrubber SHALL mengirim file bersih sebagai respons HTTP dengan header `Content-Disposition` bernilai attachment dan header `Cache-Control` bernilai `no-store`.
4. WHEN respons file bersih selesai dikirim, THE Metadata_Scrubber SHALL membebaskan buffer file asli dan file bersih dari memori dalam waktu 1 detik.
5. IF koneksi klien terputus sebelum pengiriman selesai, THEN THE Metadata_Scrubber SHALL menghentikan pengiriman dan membebaskan seluruh buffer terkait dalam waktu 1 detik, tanpa menyimpan file bersih maupun file asli.
6. THE Metadata_Scrubber SHALL tidak menuliskan nama file, isi file, atau nilai metadata ke log server, baik pada pemrosesan yang berhasil maupun gagal.
7. WHERE klien meminta mode Ephemeral_Download, THE Metadata_Scrubber SHALL menyimpan file bersih sebagai Ephemeral_Download dengan TTL 5 menit (300 detik) dan mengembalikan token unduhan.
8. IF pemrosesan gagal karena file rusak, format tidak didukung, atau ukuran melebihi 25 MB, THEN THE Metadata_Scrubber SHALL menolak permintaan dengan respons error yang menyebutkan alasan kegagalan, membebaskan seluruh buffer terkait dalam waktu 1 detik, dan tidak menyimpan file apa pun.
9. WHEN TTL 5 menit suatu Ephemeral_Download berakhir atau file telah diunduh satu kali, THE Metadata_Scrubber SHALL menghapus file bersih tersebut dan menolak permintaan unduhan berikutnya dengan token yang sama menggunakan respons error yang menyatakan token tidak valid atau kedaluwarsa.

### Requirement 4: Akses dan Antarmuka Metadata Scrubber

**User Story:** Sebagai pengguna, saya ingin halaman khusus untuk Metadata Scrubber di UI, sehingga saya dapat menggunakannya tanpa alat tambahan.

#### Acceptance Criteria

1. THE Workspace SHALL menyediakan halaman frontend `/scrubber` yang dapat diakses melalui satu tautan navigasi di Navbar dan satu tautan atau kartu di DashboardView, masing-masing mengarahkan ke `/scrubber` dengan satu klik.
2. IF pengguna tanpa Authenticated_Session mengakses endpoint Metadata_Scrubber, THEN THE Workspace SHALL menolak permintaan dengan status HTTP 401 tanpa memproses file yang dikirim dan tanpa menyimpan data file.
3. WHEN pengguna menjatuhkan atau memilih file pada halaman `/scrubber`, THE halaman `/scrubber` SHALL menampilkan nama file dan ukuran file (dalam satuan B, KB, atau MB dengan maksimum 2 digit desimal) dalam waktu 500 ms dan sebelum permintaan pengunggahan dikirim.
4. WHEN pengguna menjatuhkan atau memilih lebih dari satu file sekaligus, THE halaman `/scrubber` SHALL hanya menerima file pertama dan menampilkan pemberitahuan bahwa hanya satu file yang diproses per pembersihan.
5. WHEN pembersihan selesai, THE halaman `/scrubber` SHALL menampilkan Scrub_Report dan memulai unduhan file bersih dalam waktu 2 detik setelah respons sukses diterima.
6. IF pembersihan gagal, THEN THE halaman `/scrubber` SHALL menampilkan pesan galat yang diterima dari server (atau pesan galat umum jika server tidak merespons dalam 60 detik atau koneksi terputus), tidak memulai unduhan, mempertahankan file yang dipilih sebelumnya, dan mengaktifkan kembali kontrol pemilihan serta pengunggahan dalam waktu 1 detik.
7. THE halaman `/scrubber` SHALL menggunakan token desain Tailwind v4 yang didefinisikan di `frontend/src/styles/global.css` untuk warna, spasi, dan tipografi, tanpa nilai warna heksadesimal yang ditulis langsung pada komponen halaman.

### Requirement 5: Redaksi Rahasia pada Teks, .env, dan Log

**User Story:** Sebagai engineer, saya ingin menyamarkan rahasia dan data pelanggan pada log dan file `.env` sebelum dibagikan, sehingga saya tidak perlu menempelkannya ke layanan publik.

#### Acceptance Criteria

1. WHEN pengguna terautentikasi mengirim teks berukuran 1 sampai 5.242.880 byte (5 MB) ke endpoint Secret_Redactor, THE Secret_Redactor SHALL mengganti setiap kecocokan Redaction_Rule yang kategorinya aktif dengan Redaction_Placeholder sesuai kategorinya dan mengembalikan hasil dalam waktu maksimal 5 detik untuk masukan hingga 5 MB.
2. WHEN teks berformat `.env` dikirim, THE Secret_Redactor SHALL mempertahankan nama variabel, tanda `=`, dan baris komentar (baris diawali `#`) tanpa perubahan, serta mengganti nilai setiap pasangan `NAMA=nilai` yang nilainya tidak kosong dengan Redaction_Placeholder.
3. WHEN redaksi selesai, THE Secret_Redactor SHALL mengembalikan teks hasil dan jumlah penggantian per kategori, dengan nilai 0 untuk setiap kategori aktif yang tidak memiliki kecocokan.
4. WHERE pengguna mengaktifkan atau menonaktifkan kategori tertentu, THE Secret_Redactor SHALL menerapkan hanya kategori yang aktif dan membiarkan teks yang cocok dengan kategori nonaktif tidak berubah.
5. WHERE pengguna menyediakan daftar kata kustom berisi 1 sampai 100 kata dengan panjang masing-masing 1 sampai 256 karakter, THE Secret_Redactor SHALL mengganti setiap kemunculan kata tersebut (pencocokan sensitif huruf besar/kecil) dengan Redaction_Placeholder berkategori `custom`.
6. IF teks masukan melebihi 5.242.880 byte (5 MB), THEN THE Secret_Redactor SHALL menolak permintaan dengan status HTTP 413, tidak memproses teks, dan mengembalikan indikasi kesalahan bahwa ukuran melebihi batas.
7. IF permintaan berisi daftar kata kustom yang melebihi 100 kata, memuat kata kosong, atau memuat kata lebih dari 256 karakter, THEN THE Secret_Redactor SHALL menolak permintaan dengan status HTTP 400 dan mengembalikan indikasi kesalahan yang menyebutkan batasan yang dilanggar, tanpa memproses teks.
8. IF permintaan dikirim oleh pengguna yang tidak terautentikasi, THEN THE Secret_Redactor SHALL menolak permintaan dengan status HTTP 401 tanpa memproses teks.
9. FOR ALL teks masukan, memproses hasil Secret_Redactor sekali lagi dengan konfigurasi kategori dan daftar kata kustom yang sama SHALL menghasilkan teks yang identik dengan hasil pertama (idempotensi).
10. FOR ALL teks masukan, hasil Secret_Redactor SHALL mempertahankan jumlah baris yang sama dengan masukan, dengan baris dihitung berdasarkan jumlah karakter pemisah baris (`\n`).
11. THE Secret_Redactor SHALL tidak menuliskan teks masukan, teks hasil, maupun isi daftar kata kustom ke log server.

### Requirement 6: Inspeksi JWT

**User Story:** Sebagai engineer, saya ingin mendekode token JWT produksi tanpa mengirimkannya ke jwt.io, sehingga token aktif saya tidak bocor ke pihak ketiga.

#### Acceptance Criteria

1. WHEN pengguna terautentikasi mengirim token JWT dengan panjang 1 sampai 8.192 karakter ke endpoint JWT_Inspector, THE JWT_Inspector SHALL mengembalikan header dan payload dalam bentuk JSON terformat dengan indentasi 2 spasi dalam waktu 500 ms untuk token hingga 8.192 karakter.
2. WHEN payload memuat klaim `exp`, `iat`, atau `nbf` bernilai bilangan bulat detik Unix, THE JWT_Inspector SHALL menampilkan setiap klaim tersebut sebagai waktu UTC ISO 8601 dan menampilkan status token sebagai "kedaluwarsa" jika waktu server saat ini lebih besar atau sama dengan `exp`, atau "belum berlaku" jika waktu server saat ini lebih kecil dari `nbf`, atau "aktif" jika tidak keduanya.
3. IF klaim `exp`, `iat`, atau `nbf` bukan bilangan bulat detik Unix yang valid, THEN THE JWT_Inspector SHALL menampilkan nilai asli klaim tersebut tanpa konversi dan menandainya sebagai tidak valid, tanpa menggagalkan inspeksi.
4. IF header JWT menggunakan algoritma `none` (tanpa membedakan huruf besar/kecil), THEN THE JWT_Inspector SHALL menyertakan peringatan keamanan pada hasil inspeksi yang menyatakan bahwa token tidak ditandatangani.
5. IF token tidak terdiri dari tepat tiga segmen yang dipisahkan titik, salah satu segmen header atau payload bukan base64url valid, atau segmen header atau payload berisi JSON tidak valid, THEN THE JWT_Inspector SHALL mengembalikan status HTTP 422 dengan pesan galat yang menyebut segmen bermasalah (header, payload, atau signature) dan tidak mengembalikan data hasil dekode parsial.
6. IF token kosong atau melebihi 8.192 karakter, THEN THE JWT_Inspector SHALL menolak permintaan dengan status HTTP 422 dengan pesan galat yang menyebut batas panjang token.
7. WHERE pengguna menyediakan secret HMAC dengan panjang 1 sampai 1.024 byte dan header menyatakan algoritma HS256, HS384, atau HS512, THE JWT_Inspector SHALL memverifikasi tanda tangan token dan melaporkan hasilnya "valid" atau "tidak valid".
8. IF pengguna menyediakan secret HMAC dan algoritma header bukan HS256, HS384, atau HS512, THEN THE JWT_Inspector SHALL melaporkan bahwa verifikasi tanda tangan tidak didukung untuk algoritma tersebut dan tetap mengembalikan hasil dekode.
9. WHERE pengguna mengaktifkan opsi redaksi, THE JWT_Inspector SHALL mengganti nilai klaim email, nomor telepon, dan identitas pengguna (`email`, `phone_number`, `phone`, `sub`, `name`, `preferred_username`) pada payload yang ditampilkan dengan Redaction_Placeholder, termasuk pada klaim bersarang hingga kedalaman 5 tingkat.
10. WHEN respons inspeksi telah dikirim, THE JWT_Inspector SHALL menghapus secret HMAC dan token dari memori dalam waktu 1 detik.
11. THE JWT_Inspector SHALL tidak menuliskan token, payload, atau secret ke log server, termasuk pada jalur galat.
12. FOR ALL JWT yang dibentuk dari header dan payload JSON valid, menyandikan kembali hasil dekode JWT_Inspector ke segmen base64url SHALL menghasilkan segmen header dan payload yang ekuivalen secara semantik JSON dengan token asli (round-trip), dengan opsi redaksi dinonaktifkan.

### Requirement 7: Antarmuka Secret Redactor dan JWT Inspector

**User Story:** Sebagai pengguna, saya ingin satu halaman yang menggabungkan JWT Inspector dan Redactor, sehingga saya dapat memeriksa dan menyamarkan data dalam satu tempat.

#### Acceptance Criteria

1. THE Workspace SHALL menyediakan halaman frontend `/redactor` yang dapat diakses melalui satu tautan navigasi di Navbar dan satu tautan atau kartu di DashboardView, dan setiap tautan SHALL membuka halaman `/redactor` dalam satu kali klik.
2. WHEN pengguna membuka halaman `/redactor`, THE halaman `/redactor` SHALL menampilkan tepat dua tab, yaitu "JWT Inspector" dan "Log & .env Redactor", dengan tab "JWT Inspector" aktif secara default.
3. WHEN pengguna memilih salah satu tab, THE halaman `/redactor` SHALL menampilkan konten tab yang dipilih dalam 200 milidetik dan menyembunyikan konten tab lainnya tanpa memuat ulang halaman.
4. WHEN pengguna menempelkan teks sepanjang 1 sampai 1.048.576 karakter pada tab "Log & .env Redactor" dan menjalankan proses redaksi, THE halaman `/redactor` SHALL menampilkan teks asli dan teks hasil berdampingan dalam dua panel pada layar selebar minimal 768 piksel.
5. IF teks yang ditempelkan pada tab "Log & .env Redactor" melebihi 1.048.576 karakter, THEN THE halaman `/redactor` SHALL menolak pengiriman, menampilkan pesan kesalahan yang menyatakan batas ukuran terlampaui, dan mempertahankan teks masukan yang ada.
6. WHEN pengguna menekan tombol salin pada hasil, THE halaman `/redactor` SHALL menyalin seluruh teks hasil ke clipboard dan menampilkan konfirmasi salin yang terlihat selama 2 detik.
7. IF penyalinan ke clipboard gagal, THEN THE halaman `/redactor` SHALL menampilkan pesan kesalahan yang menyatakan penyalinan gagal dan mempertahankan teks hasil di layar.
8. WHEN pengguna meninggalkan halaman `/redactor` melalui navigasi, penutupan tab browser, atau unmount komponen, THE halaman `/redactor` SHALL mengosongkan teks masukan, token, secret, dan hasil dari state React sehingga semuanya bernilai string kosong atau null.
9. THE halaman `/redactor` SHALL tidak menulis masukan pengguna, token, secret, maupun hasil ke `localStorage`, `sessionStorage`, atau cookie.
10. IF permintaan ke endpoint Secret_Redactor atau JWT_Inspector dikirim tanpa Authenticated_Session yang valid, THEN THE Workspace SHALL mengembalikan status HTTP 401, tidak memproses masukan, dan tidak menyertakan data masukan dalam respons.

### Requirement 8: Ekspos Micro-App sebagai Tool MCP

**User Story:** Sebagai pengguna agen AI, saya ingin memanggil scrubber dan redactor lewat MCP, sehingga agen saya dapat membersihkan data sensitif tanpa meninggalkan Workspace.

#### Acceptance Criteria

1. WHEN klien MCP dengan Authenticated_Session memanggil `tools/list`, THE MCP_Server SHALL mengembalikan daftar tool yang memuat `redact_secrets`, `inspect_jwt`, `scrub_metadata`, dan `burn_workspace_now`, masing-masing dengan nama, deskripsi, dan skema argumen, serta tetap memuat semua tool yang sudah ada sebelumnya tanpa perubahan.
2. WHEN klien MCP dengan Authenticated_Session memanggil `redact_secrets` dengan argumen teks berisi 1 sampai 1.048.576 karakter, THE MCP_Server SHALL mengembalikan teks hasil redaksi dan jumlah penggantian per kategori (bernilai 0 untuk kategori tanpa kecocokan), identik dengan keluaran Secret_Redactor untuk masukan yang sama.
3. WHEN klien MCP dengan Authenticated_Session memanggil `inspect_jwt` dengan argumen token berupa string 1 sampai 8.192 karakter, THE MCP_Server SHALL mengembalikan hasil inspeksi yang identik dengan keluaran JWT_Inspector untuk token yang sama.
4. WHEN klien MCP dengan Authenticated_Session memanggil `scrub_metadata` dengan file berenkode base64 yang valid dan ukuran hasil dekode 1 byte sampai 25 MB, THE MCP_Server SHALL menyimpan file bersih sebagai Ephemeral_Download, lalu mengembalikan URL unduhan dan Scrub_Report dalam satu respons.
5. WHEN klien MCP dengan Authenticated_Session memanggil `burn_workspace_now`, THE MCP_Server SHALL menerapkan perilaku Burn_Tool sesuai Requirement 13, termasuk kewajiban argumen `confirm` bernilai true.
6. IF argumen tool MCP tidak valid, yaitu argumen wajib hilang, bertipe salah, kosong, melebihi batas panjang/ukuran, atau base64 tidak dapat didekode, THEN THE MCP_Server SHALL mengembalikan galat JSON-RPC dengan kode -32602 beserta pesan galat yang menyebut argumen yang bermasalah, tanpa menjalankan tool dan tanpa menyimpan Ephemeral_Download.
7. IF klien MCP tanpa Authenticated_Session memanggil tool apa pun, THEN THE MCP_Server SHALL menolak permintaan dengan status HTTP 401 tanpa menjalankan tool dan tanpa membocorkan data apa pun dalam isi respons.

### Requirement 9: Tombol Burn Workspace di UI

**User Story:** Sebagai pengguna, saya ingin tombol darurat yang mencolok di UI, sehingga saya dapat menghancurkan seluruh Workspace dalam satu aksi, termasuk untuk demo.

#### Acceptance Criteria

1. WHILE pengguna memiliki Authenticated_Session, THE Navbar SHALL menampilkan Burn_Button pada semua halaman yang memuat Navbar, dengan label teks yang terlihat dan nama aksesibel yang menyatakan aksi burn.
2. WHEN pengguna menekan Burn_Button, THE Workspace SHALL menampilkan dialog Burn_Confirmation dalam waktu 200 milidetik yang menyatakan bahwa seluruh data memori akan dihapus dan container akan dimatikan, dengan satu kontrol "Setuju", satu kontrol "Batal", dan opsi "Demo mode".
3. WHEN pengguna menyetujui Burn_Confirmation, THE Workspace SHALL menutup dialog, menonaktifkan Burn_Button, dan mengirim tepat satu permintaan ke endpoint `POST /api/burn`.
4. WHEN pengguna membatalkan Burn_Confirmation (melalui kontrol "Batal" atau tombol Escape), THE Workspace SHALL menutup dialog dalam waktu 200 milidetik tanpa mengirim permintaan ke `POST /api/burn` dan tanpa memicu Burn_Service.
5. IF pengguna tanpa Authenticated_Session memanggil `POST /api/burn`, THEN THE Workspace SHALL mengembalikan status HTTP 401 tanpa menjalankan Burn_Service dan tanpa mengubah data Workspace.
6. WHERE pengguna mencentang opsi "Demo mode" pada dialog Burn_Confirmation, WHEN pengguna menekan "Setuju", THE Workspace SHALL menutup dialog, menampilkan hitung mundur 3 detik yang terlihat (3, 2, 1) beserta kontrol "Batal", lalu mengirim tepat satu permintaan ke `POST /api/burn` setelah hitung mundur mencapai 0. Opsi "Demo mode" SHALL tidak ditampilkan di Navbar dan SHALL tidak tercentang secara bawaan.
7. IF pengguna menekan kontrol "Batal" saat hitung mundur Demo_Mode berjalan, THEN THE Workspace SHALL menghentikan hitung mundur dalam waktu 200 milidetik, tidak mengirim permintaan ke `POST /api/burn`, dan mengembalikan Burn_Button ke kondisi aktif.
8. IF permintaan `POST /api/burn` gagal (kesalahan jaringan, timeout lebih dari 10 detik, atau respons non-2xx), THEN THE Workspace SHALL menampilkan pesan kesalahan yang menyatakan burn gagal, mengaktifkan kembali Burn_Button, dan mempertahankan Authenticated_Session serta data Workspace tanpa perubahan di sisi klien.

### Requirement 10: Prosedur Penghancuran Workspace

**User Story:** Sebagai pengguna, saya ingin server langsung membersihkan semua yang ada di memori, sehingga tidak ada jejak setelah saya selesai.

#### Acceptance Criteria

1. WHEN Burn_Service dipicu, THE Burn_Service SHALL mencatat total byte Managed_Storage sebelum pembersihan dimulai, sebagai bilangan bulat tidak negatif (0 hingga 2^64-1) pada Audit_Report.
2. WHEN Burn_Service dipicu, THE Burn_Service SHALL mengosongkan `downloads` pada AppState sehingga jumlah entri menjadi 0, dan menimpa setiap buffer data dengan byte nol pada seluruh panjangnya sebelum buffer dibebaskan.
3. WHEN Burn_Service dipicu, THE Burn_Service SHALL menutup seluruh koneksi MCP SSE yang aktif dan mengosongkan `mcp_sessions` sehingga jumlah entri menjadi 0, dalam waktu maksimal 5 detik sejak pemicuan.
4. WHEN Burn_Service dipicu, THE Burn_Service SHALL mengosongkan `sessions` sehingga jumlah entri menjadi 0, dan menyertakan pada respons pemicu sebuah header penghapusan cookie JWT (cookie dikosongkan dengan masa berlaku kedaluwarsa).
5. WHEN Burn_Service dipicu, THE Burn_Service SHALL menghapus seluruh file dan subdirektori di dalam direktori sementara Workspace, sehingga direktori tersebut kosong setelah prosedur selesai.
6. WHILE prosedur burn sedang berjalan atau telah selesai namun proses server belum berhenti, THE Burn_Service SHALL menolak setiap permintaan baru ke endpoint micro-app dengan status HTTP 503 tanpa memproses isi permintaan.
7. IF dua atau lebih permintaan burn tiba bersamaan atau saat prosedur sedang berjalan, THEN THE Burn_Service SHALL menjalankan prosedur tepat satu kali dan mengembalikan Audit_Report yang identik kepada seluruh permintaan tersebut.
8. IF penghapusan sebuah item gagal, THEN THE Burn_Service SHALL melanjutkan penghapusan seluruh item lain, mencatat setiap item yang gagal (identitas item dan alasan kegagalan) pada Audit_Report, dan menandai status Audit_Report sebagai tidak lengkap.
9. WHEN seluruh langkah pembersihan selesai, THE Burn_Service SHALL mengembalikan Audit_Report kepada pemicu dalam waktu maksimal 10 detik sejak pemicuan, berisi total byte sebelum pembersihan, jumlah item yang berhasil dihapus, dan daftar item yang gagal (kosong jika tidak ada).

### Requirement 11: Bukti Audit Nol Byte

**User Story:** Sebagai penonton demo atau auditor, saya ingin bukti yang dapat diverifikasi bahwa tidak ada data tersisa, sehingga klaim zero-footprint dapat dipercaya.

#### Acceptance Criteria

1. WHEN pembersihan selesai, THE Burn_Service SHALL memverifikasi ulang seluruh Managed_Storage, menghitung total byte dan jumlah entri yang tersisa per kategori, dan menyelesaikan verifikasi dalam 5 detik.
2. WHEN verifikasi menunjukkan 0 byte dan 0 entri tersisa di semua kategori, THE Burn_Service SHALL mengembalikan Audit_Report dengan `remaining_bytes` bernilai 0, `verified` bernilai true, dan daftar kategori tersisa kosong.
3. THE Audit_Report SHALL memuat jumlah entri dan jumlah byte per kategori sebelum dan sesudah pembersihan, durasi pembersihan dalam milidetik (bilangan bulat ≥ 0), dan stempel waktu UTC dalam format ISO 8601.
4. THE Audit_Report SHALL tidak memuat nama file, isi file, token, atau data pengguna lain; nama kategori yang dimuat hanya berasal dari daftar kategori Managed_Storage yang tetap.
5. IF verifikasi menemukan byte tersisa lebih dari 0, THEN THE Burn_Service SHALL mengembalikan Audit_Report dengan `verified` bernilai false, `remaining_bytes` bernilai jumlah byte tersisa, dan daftar kategori yang masih memiliki byte tersisa beserta jumlah byte masing-masing.
6. IF verifikasi gagal dijalankan karena Managed_Storage tidak dapat dibaca atau melebihi batas waktu 5 detik, THEN THE Burn_Service SHALL mengembalikan Audit_Report dengan `verified` bernilai false dan indikasi bahwa verifikasi tidak lengkap, serta tidak pernah mengembalikan `verified` bernilai true.
7. WHEN Audit_Report diterima dan `verified` bernilai true, THE Workspace SHALL menampilkan layar audit yang memuat tabel sebelum dan sesudah per kategori serta indikator teks "0 bytes remaining".
8. IF Audit_Report diterima dengan `verified` bernilai false, THEN THE Workspace SHALL menampilkan layar audit yang memuat tabel sebelum dan sesudah, jumlah byte tersisa, daftar kategori tersisa, dan tidak menampilkan indikator "0 bytes remaining".
9. WHEN Audit_Report diterima, THE Workspace SHALL menampilkan animasi hitung mundur penghancuran dengan total durasi antara 1 dan 10 detik, dan menampilkan layar audit segera setelah animasi selesai.
10. THE layar audit SHALL menampilkan pernyataan teks bahwa cakupan verifikasi adalah Managed_Storage dan bukan memori proses secara keseluruhan.

### Requirement 12: Penghentian Container

**User Story:** Sebagai pengguna, saya ingin container mati setelah burn, sehingga Workspace benar-benar lenyap.

#### Acceptance Criteria

1. WHEN Audit_Report telah terkirim ke klien, THE Burn_Service SHALL menghentikan proses server dengan kode keluar 0 dalam waktu 2 detik sejak pengiriman Audit_Report selesai.
2. WHEN proses server berhenti dengan kode keluar 0 dan kebijakan restart container adalah `no` atau `on-failure`, THE Workspace SHALL tetap berada dalam status berhenti dan tidak dijalankan ulang oleh Docker, sesuai konfigurasi `deploy/docker-compose.yml`.
3. IF kebijakan restart container adalah `always` atau `unless-stopped`, THEN THE Burn_Service SHALL menyertakan satu peringatan pada Audit_Report yang menyatakan bahwa container dapat dijalankan ulang oleh Docker, dan THE Burn_Service SHALL tetap melanjutkan penghentian proses server sesuai kriteria 1.
4. IF pengiriman Audit_Report gagal karena klien terputus, THEN THE Burn_Service SHALL menghentikan proses server dengan kode keluar 0 dalam waktu 5 detik sejak burn dipicu, tanpa mencoba mengirim ulang Audit_Report.
5. WHEN proses server berhenti setelah burn, THE Workspace SHALL meninggalkan 0 file pengguna pada filesystem container yang dapat ditulis.
6. IF proses server tidak berhenti dalam batas waktu pada kriteria 1 atau 4 karena kesalahan internal, THEN THE Burn_Service SHALL memaksa penghentian proses dengan kode keluar bukan 0 dalam waktu 3 detik tambahan dan mencatat kegagalan penghentian normal ke keluaran standar error.

### Requirement 13: Tool MCP "Burn Workspace Now"

**User Story:** Sebagai pengguna agen AI, saya ingin memicu penghancuran dari klien MCP, sehingga agen dapat mengakhiri sesi dengan aman.

#### Acceptance Criteria

1. THE MCP_Server SHALL mendaftarkan tool `burn_workspace_now` dengan judul tampilan "Burn Workspace Now" dan deskripsi yang menyatakan bahwa tool menghapus secara permanen seluruh data Workspace dan tidak dapat dibatalkan, sehingga tool tersebut muncul dalam respons daftar tool (`tools/list`) dengan ketiga atribut tersebut.
2. WHEN klien MCP memanggil `burn_workspace_now` dengan argumen `confirm` bernilai boolean true, THE MCP_Server SHALL menjalankan Burn_Service tepat satu kali, mengembalikan Audit_Report sebagai hasil tool, lalu menghentikan proses setelah hasil tool terkirim penuh ke klien atau setelah 5 detik sejak hasil dibuat, mana yang lebih dulu.
3. IF argumen `confirm` tidak ada, bernilai boolean false, atau bertipe selain boolean (termasuk string "true"), THEN THE MCP_Server SHALL mengembalikan galat JSON-RPC dengan kode -32602 yang menunjukkan bahwa konfirmasi diperlukan, tidak menjalankan Burn_Service, dan mempertahankan seluruh data Workspace serta proses server tetap berjalan.
4. WHEN Burn_Tool dipanggil dengan `confirm` bernilai true melalui koneksi SSE, THE MCP_Server SHALL mengirim hasil tool sebagai event SSE lengkap dan menutup koneksi hanya setelah event tersebut selesai dikirim.
5. WHEN Burn_Service dipicu oleh Burn_Tool, THE Workspace SHALL mengirim event SSE `burn` yang memuat Audit_Report ke setiap sesi UI yang terbuka dalam 2 detik sejak Burn_Service dimulai, dan THE Workspace SHALL menampilkan layar audit pada sesi UI tersebut.
6. IF Burn_Service gagal atau tidak selesai saat dipicu oleh Burn_Tool, THEN THE MCP_Server SHALL mengembalikan galat JSON-RPC dengan kode -32603 yang menunjukkan kegagalan penghancuran, menyertakan daftar langkah yang belum selesai bila tersedia, dan membiarkan proses server tetap berjalan.
7. IF `burn_workspace_now` dipanggil saat Burn_Service sedang berjalan, THEN THE MCP_Server SHALL mengembalikan galat JSON-RPC yang menunjukkan bahwa penghancuran sedang berlangsung, tanpa menjalankan Burn_Service kedua kali.

### Requirement 14: Kepatuhan Zero-Footprint dan Dokumentasi

**User Story:** Sebagai maintainer, saya ingin fitur baru mematuhi aturan proyek dan terdokumentasi, sehingga basis kode tetap konsisten.

#### Acceptance Criteria

1. THE Workspace SHALL menjalankan seluruh fitur baru (Metadata_Scrubber, Secret_Redactor, JWT_Inspector, dan Burn_Service) tanpa database persisten dan tanpa menulis konten pengguna ke disk di luar direktori yang dikelola TempDirGuard.
2. WHEN pemrosesan sebuah permintaan fitur baru selesai, baik berhasil maupun gagal, THE Workspace SHALL menghapus seluruh berkas sementara milik permintaan tersebut dari direktori yang dikelola TempDirGuard dalam waktu 5 detik, sehingga jumlah berkas sisa milik permintaan tersebut adalah 0.
3. THE Workspace SHALL mendaftarkan rute `/api/apps/scrubber`, `/api/apps/redactor`, `/api/apps/jwt`, dan `/api/burn` pada `create_api_router`, sehingga setiap rute menghasilkan respons selain "tidak ditemukan" saat diminta dengan metode HTTP yang didukung.
4. THE Workspace SHALL menyertakan minimal satu pengujian otomatis jalur sukses dan satu pengujian otomatis jalur gagal untuk masing-masing Metadata_Scrubber, Secret_Redactor, JWT_Inspector, dan Burn_Service, yang seluruhnya dijalankan oleh `cargo test --workspace`.
5. IF salah satu pengujian otomatis pada kriteria 4 gagal, THEN THE Workspace SHALL membuat `cargo test --workspace` berakhir dengan kode keluar bukan nol dan menampilkan nama pengujian yang gagal.
6. THE Workspace SHALL memperbarui `README.md` dan `AGENTS.md` sehingga masing-masing berkas memuat, untuk setiap fitur baru, deskripsi micro-app, rute API, nama tool MCP, dan deskripsi perilaku Burn Workspace.
7. WHERE perintah `teardown` CLI dijalankan, THE CLI SHALL tetap berfungsi dengan perilaku yang sama seperti sebelum fitur baru ditambahkan, tanpa memanggil atau bergantung pada Burn_Service.

### Requirement 15: Manajemen Token Akses MCP di Dashboard

**User Story:** Sebagai pengguna, saya ingin membuat token akses MCP dari Dashboard, sehingga saya dapat menghubungkan klien MCP tanpa memanggil API secara manual.

#### Acceptance Criteria

1. WHILE pengguna memiliki Authenticated_Session, THE Dashboard SHALL menampilkan panel "MCP access tokens" dengan isian label (maksimal 64 karakter), pilihan masa berlaku (1, 7, 30, 90, atau 365 hari), dan tombol "Create token".
2. WHEN pengguna menekan "Create token", THE Workspace SHALL mengirim tepat satu permintaan `POST /api/auth/token` dengan `lifetime_seconds` sesuai pilihan dan `label` yang telah dipangkas spasinya, lalu menampilkan token yang dikembalikan, tanggal kedaluwarsa, dan konfigurasi klien MCP (URL `/api/mcp/sse` dan header `Authorization: Bearer`) dengan tombol salin.
3. THE Workspace SHALL menampilkan token hanya satu kali dengan pesan yang menyatakan bahwa token tidak dapat dilihat lagi, dan SHALL tidak menyimpan token di localStorage, sessionStorage, atau cookie.
4. IF `POST /api/auth/token` gagal, THEN THE Dashboard SHALL menampilkan pesan kesalahan dari server dan tidak menampilkan token.
5. THE Dashboard SHALL menampilkan daftar token MCP aktif (label, waktu pembuatan, waktu kedaluwarsa) dari `GET /api/sessions` dan SHALL menyatakan bahwa token belum dapat dicabut.
6. IF `label` lebih dari 64 karakter atau mengandung karakter kontrol, THEN THE Workspace SHALL menolak `POST /api/auth/token` dengan status HTTP 400.
7. THE Workspace SHALL menyertakan tipe token dan label di dalam JWT sehingga container baru dapat memulihkan keduanya saat token digunakan.
