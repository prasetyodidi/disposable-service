# Requirements Document

## Introduction

Fitur ini menambahkan mini app mandiri "PDF to Markdown" ke Disposable Service Workspace. Mini app ini mengubah file PDF yang memiliki text layer menjadi teks Markdown tanpa mengirim dokumen ke layanan pihak ketiga. Mini app memiliki halaman sendiri (`/pdf-to-markdown`), tautan Navbar sendiri, rute API sendiri, dan tool MCP sendiri `pdf_to_markdown`; fitur ini bukan tab pada konverter Pandoc.

Mesin ekstraksi pada Fase 1 adalah `pdftohtml -xml` dari poppler-utils yang dijalankan sebagai subprocess, ditambah heuristik Rust untuk menyusun struktur Markdown. Arsitektur mesin bersifat pluggable: mesin lain (misalnya `pdftotext -layout`, MuPDF, atau OCR) dapat ditambahkan di balik satu antarmuka bersama dan dipilih lewat konfigurasi. Fase 1 mencakup heading, paragraf, daftar, blok kode, teks tebal/miring, serta penghapusan header/footer. Fase 1 tidak mencakup deteksi tabel, OCR, tata letak multi-kolom, maupun ekstraksi gambar.

Batas ukuran file adalah 25 MB pada server umum dan 5 MB pada AWS Lambda (payload Function URL yang di-buffer sekitar 6 MB); batas ini dapat dikonfigurasi. Seluruh fitur tunduk pada invarian zero-footprint: tidak ada database persisten, file hanya berada di direktori sementara yang dikelola TempDirGuard, dan tidak ada nama file, isi file, atau metadata pengguna yang masuk ke log.

## Glossary

- **Workspace**: Instans server Disposable Service (crate `disposable-server`) yang berjalan di dalam container Docker atau fungsi AWS Lambda.
- **PDF_Converter**: Modul server yang menerima file PDF dan menghasilkan Markdown; mencakup handler HTTP, handler MCP, dan Extraction_Engine.
- **PDF_Converter_Page**: Halaman frontend `/pdf-to-markdown`.
- **Extraction_Engine**: Implementasi antarmuka (trait) bersama yang mengubah PDF menjadi struktur dokumen perantara (blok teks beserta ukuran font, nama font, gaya, dan posisi).
- **Poppler_Engine**: Extraction_Engine bawaan yang menjalankan `pdftohtml -xml` dari poppler-utils sebagai subprocess.
- **Engine_Registry**: Komponen yang memetakan nilai konfigurasi `PDF_MD_ENGINE` ke satu Extraction_Engine.
- **Structure_Builder**: Komponen heuristik Rust yang mengubah struktur dokumen perantara menjadi teks Markdown.
- **Conversion_Options**: Objek JSON opsi konversi dengan field `page_from`, `page_to`, `page_breaks`, dan `strip_headers_footers`.
- **Conversion_Result**: Objek JSON respons sukses dengan field `markdown`, `pages`, `headings`, dan `warnings`.
- **Body_Font_Size**: Ukuran font yang paling banyak dipakai (berdasarkan jumlah karakter) pada halaman yang dikonversi.
- **Heading_Block**: Blok teks dengan ukuran font lebih besar daripada Body_Font_Size sehingga dipetakan menjadi heading Markdown level 1 sampai 6.
- **Monospace_Block**: Blok teks yang seluruh fontnya bernama font monospace (misalnya Courier, Consolas, Menlo, Monaco, atau nama yang memuat "Mono").
- **Repeated_Line**: Baris teks yang muncul pada area atas atau bawah halaman dengan isi identik (setelah angka dinormalisasi) pada paling sedikit 3 halaman dan paling sedikit 50% dari halaman yang dikonversi.
- **Page_Number_Line**: Baris pada area atas atau bawah halaman yang hanya berisi angka desimal, angka Romawi, atau pola "Page N" / "Halaman N" / "N of M".
- **Text_Layer**: Teks yang dapat diekstrak langsung dari PDF tanpa OCR.
- **Max_Bytes**: Batas ukuran file yang dibaca dari variabel lingkungan `PDF_MD_MAX_BYTES`; bawaan 26.214.400 byte (25 MB) pada server umum dan 5.242.880 byte (5 MB) pada Lambda.
- **Max_Pages**: Batas 300 halaman per konversi.
- **Max_Response_Bytes**: Batas ukuran body JSON Conversion_Result yang diserialisasi, dibaca dari variabel lingkungan `PDF_MD_MAX_RESPONSE_BYTES`; bawaan 5.767.168 byte (5,5 MiB) pada Lambda_Runtime dan tanpa batas pada server umum.
- **Subprocess_Timeout**: Batas waktu 30 detik untuk satu eksekusi subprocess ekstraksi.
- **Ephemeral_Download**: Entri unduhan in-memory dengan token UUID, TTL 5 menit, dan sifat burn-after-reading (sama dengan definisi pada spec privacy-micro-apps-and-burn-workspace).
- **TempDirGuard**: RAII guard yang menghapus direktori sementara saat di-drop.
- **Authenticated_Session**: Sesi pengguna dengan JWT cookie `HttpOnly` yang valid atau token akses MCP yang valid.
- **Burn_Gate**: Mekanisme `burn_gate` yang menolak permintaan micro-app dengan HTTP 503 selama atau setelah Burn Workspace dijalankan.
- **MCP_Server**: Endpoint MCP via SSE (`/api/mcp/sse` dan `/api/mcp/message`) pada Workspace.
- **Navbar**: Komponen frontend `frontend/src/components/Navbar.tsx` beserta `PageShell.tsx`.
- **Lambda_Runtime**: Lingkungan AWS Lambda arm64 yang dibangun dari `deploy/Dockerfile.lambda`.

## Requirements

### Requirement 1: Konversi PDF ke Markdown via API

**User Story:** Sebagai pengguna, saya ingin mengunggah PDF dan menerima teks Markdown, sehingga saya dapat menggunakan isi dokumen tanpa menyalinnya secara manual.

#### Acceptance Criteria

1. WHEN pengguna terautentikasi mengirim permintaan `POST /api/apps/pdf-to-markdown` bertipe multipart berisi satu part `file` PDF yang valid dengan Text_Layer dan ukuran 1 byte sampai Max_Bytes, THE PDF_Converter SHALL mengembalikan status HTTP 200 dengan Conversion_Result berisi `markdown` (string tidak kosong), `pages` (jumlah halaman yang dikonversi, bilangan bulat ≥ 1 dan ≤ Max_Pages), `headings` (jumlah Heading_Block yang dihasilkan, bilangan bulat ≥ 0), dan `warnings` (daftar string, daftar kosong jika tidak ada peringatan).
2. WHERE permintaan memuat part `options` berisi Conversion_Options dalam JSON, THE PDF_Converter SHALL menerapkan nilai `page_from`, `page_to`, `page_breaks`, dan `strip_headers_footers` yang diberikan pada konversi, dan field yang tidak diberikan SHALL memakai nilai bawaan pada kriteria 3.
3. WHERE part `options` tidak ada, THE PDF_Converter SHALL menggunakan nilai bawaan `page_from` = 1, `page_to` = halaman terakhir dokumen, `page_breaks` = false, dan `strip_headers_footers` = true.
4. IF part `options` berisi JSON tidak valid, field tidak dikenal, field dengan tipe salah, `page_from` atau `page_to` yang bukan bilangan bulat ≥ 1, atau `page_from` lebih besar daripada `page_to`, THEN THE PDF_Converter SHALL menolak permintaan dengan status HTTP 400 dan pesan galat dalam badan JSON yang menyebut nama field bermasalah (atau menyebut JSON tidak valid), tanpa menjalankan Extraction_Engine dan tanpa mengembalikan Conversion_Result.
5. IF `page_to` melebihi jumlah halaman dokumen dan `page_from` tidak melebihi jumlah halaman dokumen, THEN THE PDF_Converter SHALL mengonversi halaman `page_from` sampai halaman terakhir, mengembalikan status HTTP 200, dan menambahkan tepat satu entri pada `warnings` yang menyebut bahwa `page_to` disesuaikan beserta nilai akhirnya.
6. IF `page_from` melebihi jumlah halaman dokumen, THEN THE PDF_Converter SHALL mengembalikan status HTTP 400 dengan pesan galat yang menyebut jumlah halaman dokumen, tanpa mengembalikan Conversion_Result.
7. IF permintaan tidak memuat part `file` pada tahap pembacaan multipart, THEN THE PDF_Converter SHALL mengembalikan status HTTP 400 dengan kode galat `file_required` dan pesan galat yang menyatakan bahwa file wajib disertakan, tanpa menjalankan Extraction_Engine; part `file` yang ada tetapi berukuran 0 byte atau tidak diawali `%PDF-` SHALL ditangani oleh Requirement 2 kriteria 3 (HTTP 415, kode galat `not_a_pdf`).
8. WHEN permintaan memuat lebih dari satu part `file`, THE PDF_Converter SHALL memproses hanya part `file` pertama sesuai urutan dalam permintaan, mengabaikan part `file` lainnya, dan menambahkan tepat satu entri pada `warnings` yang menyebut bahwa file tambahan diabaikan.
9. WHEN konversi dokumen dengan ukuran sampai 25 MB dan jumlah halaman sampai Max_Pages diminta, THE PDF_Converter SHALL mengembalikan respons (sukses atau galat) dalam waktu paling lama 35 detik sejak body permintaan diterima penuh, yaitu Subprocess_Timeout 30 detik ditambah 5 detik pemrosesan heuristik.

### Requirement 2: Validasi Input dan Kode Galat

**User Story:** Sebagai pengguna, saya ingin pesan galat yang jelas saat file tidak dapat dikonversi, sehingga saya tahu apa yang harus diperbaiki.

#### Acceptance Criteria

1. IF ukuran file melebihi Max_Bytes, THEN THE PDF_Converter SHALL mengembalikan status HTTP 413 dengan kode galat `file_too_large` dan pesan galat yang menyebut batas ukuran Max_Bytes yang berlaku, tanpa menjalankan Extraction_Engine dan tanpa meninggalkan berkas pada direktori sementara.
2. WHEN file berukuran tepat Max_Bytes dikirim, THE PDF_Converter SHALL menerima file tersebut untuk diproses; IF file berukuran Max_Bytes + 1 byte, THEN THE PDF_Converter SHALL menolaknya dengan status HTTP 413 dan kode galat `file_too_large`.
3. IF file berukuran 0 byte atau 5 byte pertamanya bukan magic bytes `%PDF-`, THEN THE PDF_Converter SHALL mengembalikan status HTTP 415 dengan kode galat `not_a_pdf` dan pesan galat yang menyatakan bahwa file bukan PDF, tanpa menjalankan Extraction_Engine.
4. IF file diawali `%PDF-` tetapi rusak sehingga Extraction_Engine melaporkan galat `corrupt`, THEN THE PDF_Converter SHALL mengembalikan status HTTP 422 dengan kode galat `corrupt_pdf` dan badan respons yang tidak memuat `markdown` parsial.
5. IF PDF terenkripsi dengan kata sandi sehingga Extraction_Engine melaporkan galat `encrypted`, THEN THE PDF_Converter SHALL mengembalikan status HTTP 422 dengan kode galat `encrypted_pdf` dan pesan galat yang menyatakan bahwa PDF terenkripsi tidak didukung.
6. IF PDF valid tetapi Extraction_Engine tidak menghasilkan teks (0 karakter selain spasi putih) pada seluruh halaman yang dikonversi, misalnya PDF hasil pindai, THEN THE PDF_Converter SHALL mengembalikan status HTTP 422 dengan kode galat `no_text_layer` dan pesan galat yang menyatakan bahwa OCR belum didukung.
7. IF jumlah halaman yang akan dikonversi (setelah penerapan `page_from` dan `page_to`) melebihi Max_Pages sebesar 300 halaman, THEN THE PDF_Converter SHALL mengembalikan status HTTP 422 dengan kode galat `too_many_pages` dan pesan galat yang menyebut batas 300 halaman, tanpa menjalankan konversi; jumlah tepat 300 halaman SHALL diterima.
8. IF subprocess ekstraksi tidak selesai dalam Subprocess_Timeout sebesar 30 detik, THEN THE PDF_Converter SHALL menghentikan subprocess dan seluruh proses turunannya dalam 1 detik, lalu mengembalikan status HTTP 504 dengan kode galat `timeout` tanpa keluaran parsial.
9. IF permintaan ke rute PDF_Converter mana pun dikirim tanpa Authenticated_Session, THEN THE PDF_Converter SHALL mengembalikan status HTTP 401 tanpa membaca badan permintaan, tanpa menjalankan Extraction_Engine, dan tanpa membuat direktori sementara.
10. WHILE Burn_Gate aktif, THE PDF_Converter SHALL menolak setiap permintaan ke rute PDF_Converter dengan status HTTP 503 tanpa membaca badan permintaan, tanpa menjalankan Extraction_Engine, dan tanpa membuat direktori sementara.
11. THE PDF_Converter SHALL menyertakan field kode galat (string) dan field pesan (string tidak kosong) dalam badan JSON setiap respons galat, tanpa memuat nama file, isi file, atau metadata dokumen.
12. IF lebih dari satu kondisi galat berlaku pada permintaan yang sama, THEN THE PDF_Converter SHALL mengembalikan galat dengan urutan prioritas: 401, 503, 413, 415, 400, 422, 504, sehingga hasil pengujian deterministik.

### Requirement 3: Heading, Paragraf, dan Gaya Teks

**User Story:** Sebagai pengguna, saya ingin struktur dokumen terbaca pada Markdown, sehingga hasilnya langsung dapat dipakai sebagai dokumen.

#### Acceptance Criteria

1. WHEN sebuah blok teks memiliki ukuran font yang lebih besar daripada Body_Font_Size sebesar paling sedikit 15% (ukuran font blok ≥ 1,15 × Body_Font_Size), THE Structure_Builder SHALL mengubah blok tersebut menjadi Heading_Block dengan awalan `#` sebanyak 1 sampai 6 diikuti satu spasi, dan WHEN jumlah ukuran font berbeda di atas ambang melebihi 6, THE Structure_Builder SHALL memberi level 6 pada semua ukuran font yang lebih kecil daripada enam ukuran terbesar.
2. THE Structure_Builder SHALL memetakan ukuran font yang berbeda ke level heading secara monoton: untuk dua Heading_Block dengan ukuran font A > B, level heading A SHALL lebih kecil atau sama dengan level heading B, dan untuk dua Heading_Block dengan ukuran font sama, level heading SHALL sama.
3. WHEN dua baris teks berurutan memiliki ukuran font yang sama dan jarak vertikal antar baris tidak lebih dari 1,5 kali tinggi baris, THE Structure_Builder SHALL menggabungkannya ke dalam satu paragraf dengan satu spasi sebagai pemisah.
4. WHEN dua baris teks berurutan memiliki jarak vertikal lebih dari 1,5 kali tinggi baris, THE Structure_Builder SHALL memisahkannya menjadi paragraf berbeda dengan tepat satu baris kosong di antaranya.
5. WHEN sebuah baris diakhiri satu tanda hubung `-` yang langsung mengikuti huruf dan baris berikutnya dalam paragraf yang sama diawali huruf kecil, THE Structure_Builder SHALL menggabungkan kedua baris menjadi satu kata dengan menghapus tanda hubung, jeda baris, dan spasi pemisah, serta menghitungnya sebagai satu hyphenation join.
6. WHEN sebuah rentang teks yang bukan seluruh isi blok memakai font bergaya tebal, THE Structure_Builder SHALL membungkus rentang tersebut dengan `**` tanpa spasi di dalam pembungkus; WHEN rentang memakai font bergaya miring, THE Structure_Builder SHALL membungkusnya dengan `*`; WHEN rentang memakai keduanya, THE Structure_Builder SHALL membungkusnya dengan `***`.
7. WHEN seluruh teks dalam satu blok bergaya tebal atau miring, THE Structure_Builder SHALL membungkus blok tersebut dengan satu pasang penanda yang sama pada awal dan akhir blok, bukan per kata.
8. THE Structure_Builder SHALL meloloskan (escape) dengan awalan `\` karakter Markdown khusus `\`, `` ` ``, `*`, `_`, `[`, dan `]` di mana pun pada teks biasa, serta `#` dan `>` hanya bila berada di awal baris, sehingga teks yang dirender kembali identik dengan teks sumber.
9. IF dokumen tidak memiliki blok dengan ukuran font di atas ambang pada kriteria 1, THEN THE Structure_Builder SHALL menghasilkan nol Heading_Block dan `headings` pada Conversion_Result bernilai 0.
10. THE Structure_Builder SHALL menghasilkan teks Markdown yang diakhiri tepat satu karakter baris baru dan tanpa lebih dari satu baris kosong berturut-turut.
11. WHEN seluruh teks sebuah Heading_Block bergaya tebal, THE Structure_Builder SHALL menghasilkan heading berbentuk `# **Judul**` (awalan `#` diikuti teks yang dibungkus penanda tebal) sesuai aturan umum kriteria 7, dan perilaku ini SHALL didokumentasikan sebagai batasan Fase 1, bukan cacat.

### Requirement 4: Daftar dan Blok Kode

**User Story:** Sebagai pengguna, saya ingin daftar dan potongan kode terdeteksi, sehingga dokumen teknis tetap terbaca.

#### Acceptance Criteria

1. WHEN sebuah baris diawali salah satu penanda bullet (`•`, `◦`, `▪`, `-`, atau `*`) yang diikuti tepat satu spasi atau lebih, THE Structure_Builder SHALL mengubahnya menjadi item daftar tak berurutan Markdown berawalan `- ` diikuti teks item tanpa penanda sumber dan tanpa spasi awal tambahan.
2. WHEN sebuah baris diawali pola satu sampai sembilan digit angka desimal diikuti `.` atau `)` dan satu spasi atau lebih (misalnya `1.` atau `2)`), THE Structure_Builder SHALL mengubahnya menjadi item daftar berurutan Markdown berawalan `N. ` dengan N sama dengan angka sumber, dan penanda `)` pada sumber SHALL dikeluarkan sebagai `.`.
3. WHEN item daftar memiliki indentasi horizontal lebih besar daripada item sebelumnya, THE Structure_Builder SHALL menjadikannya item bersarang satu tingkat lebih dalam dengan indentasi 2 spasi per tingkat, sampai kedalaman maksimum 4 tingkat; WHEN indentasi sama dengan item sebelumnya, THE Structure_Builder SHALL menempatkannya pada tingkat yang sama; WHEN indentasi lebih kecil, THE Structure_Builder SHALL menempatkannya pada tingkat item terdekat sebelumnya yang indentasinya lebih kecil atau sama.
4. IF indentasi item daftar akan menghasilkan kedalaman lebih dari 4 tingkat, THEN THE Structure_Builder SHALL menempatkan item tersebut pada tingkat 4 tanpa menghapus teksnya.
5. WHEN baris lanjutan tanpa penanda mengikuti item daftar dengan jarak vertikal tidak lebih dari 1,5 kali tinggi baris dan ukuran font yang sama dengan item tersebut, THE Structure_Builder SHALL menggabungkannya ke item daftar tersebut dengan satu spasi sebagai pemisah.
6. WHEN satu atau lebih baris berurutan merupakan Monospace_Block, THE Structure_Builder SHALL menempatkannya dalam satu blok kode berpagar tiga backtick dengan mempertahankan urutan baris dan indentasi relatif, dan SHALL menempatkan baris pembuka dan baris penutup pagar masing-masing pada barisnya sendiri.
7. WHEN dua Monospace_Block dipisahkan oleh baris yang bukan Monospace_Block, THE Structure_Builder SHALL menghasilkan dua blok kode terpisah.
8. IF isi blok kode memuat deretan tiga backtick atau lebih, THEN THE Structure_Builder SHALL memakai pagar dengan jumlah backtick sama dengan jumlah backtick deretan terpanjang di dalam blok ditambah 1.
9. THE Structure_Builder SHALL tidak menerapkan penggabungan hyphenation, penanda gaya (`**`, `*`, `***`), maupun escape karakter Markdown pada isi blok kode, sehingga isi blok kode identik karakter demi karakter dengan teks sumber.
10. IF baris diawali penanda bullet atau pola angka tetapi merupakan Monospace_Block, THEN THE Structure_Builder SHALL memperlakukannya sebagai isi blok kode dan tidak mengubahnya menjadi item daftar.

### Requirement 5: Penghapusan Header, Footer, dan Pemisah Halaman

**User Story:** Sebagai pengguna, saya ingin header, footer, dan nomor halaman berulang dibuang, sehingga hasil Markdown tidak berisi gangguan.

#### Acceptance Criteria

1. WHILE `strip_headers_footers` bernilai true dan dokumen yang dikonversi memiliki paling sedikit 3 halaman, THE Structure_Builder SHALL menghapus setiap Repeated_Line dari hasil, dengan area atas didefinisikan sebagai 10% tinggi halaman teratas dan area bawah sebagai 10% tinggi halaman terbawah.
2. WHILE `strip_headers_footers` bernilai true, THE Structure_Builder SHALL menghapus setiap Page_Number_Line pada area atas atau bawah halaman dari hasil, tanpa memandang jumlah halaman dokumen.
3. WHILE `strip_headers_footers` bernilai false, THE Structure_Builder SHALL mempertahankan Repeated_Line dan Page_Number_Line pada hasil dan SHALL tidak menambahkan entri `warnings` tentang penghapusan baris.
4. WHEN satu atau lebih Repeated_Line atau Page_Number_Line dihapus, THE PDF_Converter SHALL menambahkan tepat satu entri pada `warnings` yang menyebut jumlah total baris yang dihapus (bilangan bulat ≥ 1) tanpa memuat isi baris tersebut; IF tidak ada baris yang dihapus, THEN THE PDF_Converter SHALL tidak menambahkan entri tersebut.
5. WHERE `page_breaks` bernilai true, THE Structure_Builder SHALL menyisipkan tepat satu baris pemisah `---` yang dikelilingi satu baris kosong di antara konten dua halaman berurutan yang dikonversi, dan SHALL tidak menyisipkannya sebelum halaman pertama atau setelah halaman terakhir, sehingga jumlah pemisah sama dengan `pages` dikurangi 1.
6. WHERE `page_breaks` bernilai false, THE Structure_Builder SHALL menggabungkan konten halaman berurutan tanpa baris pemisah `---`; IF baris terakhir halaman sebelumnya tidak berakhir dengan salah satu tanda baca penutup kalimat (`.`, `!`, `?`, `:`) dan blok pertama halaman berikutnya adalah paragraf (bukan Heading_Block, item daftar, atau Monospace_Block), THEN THE Structure_Builder SHALL menggabungkan keduanya menjadi satu paragraf dengan satu spasi sebagai pemisah.
7. WHEN `page_from` dan `page_to` ditentukan, THE PDF_Converter SHALL hanya mengonversi halaman `page_from` sampai `page_to` (inklusif), `pages` pada Conversion_Result SHALL bernilai `page_to - page_from + 1` setelah `page_to` disesuaikan dengan jumlah halaman dokumen, dan perhitungan Repeated_Line (ambang 3 halaman dan 50%) SHALL hanya memakai halaman dalam rentang tersebut.
8. WHEN teks berada pada area isi halaman (di luar area atas dan bawah sebagaimana didefinisikan pada kriteria 1), THE Structure_Builder SHALL mempertahankannya meskipun isinya sama dengan Repeated_Line.

### Requirement 6: Preservasi Konten

**User Story:** Sebagai pengguna, saya ingin tidak ada kata yang hilang atau berubah urutannya, sehingga hasil konversi dapat dipercaya.

#### Acceptance Criteria

1. FOR ALL PDF valid dengan Text_Layer, deret kata pada `markdown` SHALL sama dengan deret kata pada teks sumber sesuai urutan baca, dengan ketentuan: (a) kata adalah deretan karakter non-spasi yang dipisahkan spasi, tab, atau baris baru; (b) sintaks Markdown yang ditambahkan Structure_Builder (awalan `#`, penanda `**`/`*`/`***`, awalan item daftar, pagar blok kode, pemisah `---`, dan karakter escape `\`) dihapus sebelum perbandingan; (c) pengecualian hanya berlaku untuk kata yang digabung oleh hyphenation join (Requirement 3 kriteria 5) dan baris yang dihapus oleh opsi `strip_headers_footers` (Requirement 5).
2. WHERE `strip_headers_footers` bernilai false, THE PDF_Converter SHALL menghasilkan `markdown` dengan jumlah kata sama dengan jumlah kata teks sumber pada halaman yang dikonversi dikurangi jumlah hyphenation join, dengan definisi kata seperti pada kriteria 1.
3. WHEN halaman dokumen satu kolom dikonversi, THE Structure_Builder SHALL mengurutkan baris teks pada satu halaman dari atas ke bawah berdasarkan posisi vertikal, dan baris dengan posisi vertikal sama dari kiri ke kanan berdasarkan posisi horizontal.
4. THE PDF_Converter SHALL mempertahankan karakter Unicode non-ASCII (termasuk aksara Latin beraksen, Cyrillic, dan CJK) pada `markdown` sebagai titik kode Unicode yang identik dengan sumber, dalam encoding UTF-8, tanpa konversi ke entitas HTML.
5. WHEN keluaran `pdftohtml -xml` memuat entitas XML (`&amp;`, `&lt;`, `&gt;`, `&quot;`, `&apos;`, referensi numerik desimal `&#N;`, atau referensi numerik heksadesimal `&#xH;`), THE PDF_Converter SHALL mendekodenya menjadi karakter aslinya tepat satu kali sebelum penyusunan Markdown, sehingga teks sumber `&lt;` (yang dikodekan `&amp;lt;` pada XML) menghasilkan `&lt;` pada `markdown` dan bukan `<`.
6. THE PDF_Converter SHALL menghasilkan `markdown` yang identik byte demi byte pada dua eksekusi berturut-turut atas PDF dan Conversion_Options yang sama (determinisme).
7. FOR ALL dokumen Markdown M (terdiri dari heading dan paragraf) yang dikonversi ke PDF oleh konverter Markdown ke PDF yang sudah ada lalu dikonversi oleh PDF_Converter dengan Conversion_Options bawaan, teks setiap heading M SHALL muncul pada `markdown` dengan urutan relatif yang sama, dan urutan teks paragraf M SHALL tetap sama; perbandingan dilakukan setelah sintaks Markdown dihapus dan spasi berurutan dinormalisasi menjadi satu spasi (round trip). Batas paragraf tidak dijamin terpertahankan untuk PDF yang dihasilkan LaTeX karena paragraf dibedakan hanya oleh indentasi baris pertama; perbandingan SHALL dilakukan atas deret kata heading dan paragraf secara berurutan (teks heading dan urutan relatif kata), bukan atas batas paragraf.

### Requirement 7: Pemilihan Extraction_Engine yang Pluggable

**User Story:** Sebagai maintainer, saya ingin mesin ekstraksi dapat diganti lewat konfigurasi, sehingga mesin lain (misalnya OCR) dapat ditambahkan tanpa mengubah API.

#### Acceptance Criteria

1. THE PDF_Converter SHALL mengakses semua mesin ekstraksi melalui satu antarmuka Extraction_Engine yang menerima jalur file PDF di direktori sementara, rentang halaman (`page_from` dan `page_to`, keduanya bilangan bulat ≥ 1), dan Subprocess_Timeout (30 detik), serta mengembalikan struktur dokumen perantara atau tepat satu galat bertipe dari himpunan tetap: `corrupt`, `encrypted`, `no_text_layer`, `timeout`, `too_many_pages`, `engine_unavailable`, `output_too_large`, `page_from_out_of_range`; galat `engine_unavailable` dipetakan ke HTTP 503 sesuai kriteria 7 di bawah, galat `output_too_large` dipetakan ke HTTP 422 sesuai Requirement 8 kriteria 6, dan galat `page_from_out_of_range` dipetakan ke HTTP 400 sesuai Requirement 1 kriteria 6.
2. THE Engine_Registry SHALL memilih tepat satu Extraction_Engine berdasarkan nilai variabel lingkungan `PDF_MD_ENGINE` yang dibaca satu kali saat server mulai, dan SHALL tidak mengubah pilihan tersebut selama proses server berjalan.
3. WHERE `PDF_MD_ENGINE` tidak diatur atau berupa string kosong atau hanya berisi spasi, THE Engine_Registry SHALL memilih Poppler_Engine.
4. IF `PDF_MD_ENGINE` berisi nama mesin yang tidak terdaftar, THEN THE Workspace SHALL menghentikan proses start dengan kode keluar bukan nol dan pesan galat yang menyebut nama variabel `PDF_MD_ENGINE` serta daftar nilai yang valid, SHALL tidak membuka port HTTP, dan SHALL tidak memakai mesin lain secara diam-diam.
5. THE Structure_Builder SHALL beroperasi hanya pada struktur dokumen perantara dan SHALL tidak bergantung pada Extraction_Engine tertentu, sehingga penambahan mesin baru tidak mengubah Structure_Builder, kontrak API (rute, field Conversion_Result, dan kode status HTTP), kode galat pada Requirement 2, maupun kontrak tool MCP `pdf_to_markdown`.
6. THE Workspace SHALL menyertakan satu pengujian yang mendaftarkan Extraction_Engine tiruan pada Engine_Registry dan memverifikasi bahwa PDF_Converter menghasilkan Conversion_Result (status HTTP 200 dengan `markdown` yang berasal dari keluaran mesin tiruan) tanpa memanggil poppler, dengan pengujian tersebut berjalan juga saat `pdftohtml` tidak tersedia.
7. IF Extraction_Engine yang dipilih bergantung pada executable eksternal yang tidak ditemukan atau tidak dapat dijalankan saat konversi, THEN THE PDF_Converter SHALL mengembalikan status HTTP 503 dengan kode galat `engine_unavailable` dan pesan yang tidak memuat jalur filesystem, SHALL menghapus direktori sementara milik permintaan, dan SHALL tidak mengembalikan keluaran parsial.

### Requirement 8: Poppler_Engine dan Isolasi Subprocess

**User Story:** Sebagai operator, saya ingin subprocess ekstraksi terisolasi dan aman, sehingga dokumen berbahaya tidak dapat memengaruhi server.

#### Acceptance Criteria

1. THE Poppler_Engine SHALL menjalankan `pdftohtml` dengan daftar argumen yang tersusun hanya dari opsi tetap (`-xml`, `-i`, `-q`, `-stdout` atau berkas keluaran di direktori sementara milik permintaan, serta `-f` dan `-l` untuk rentang halaman) dan jalur berkas masukan yang dihasilkan server, tanpa argumen apa pun yang berasal dari nama file atau isi permintaan pengguna. Selain `pdftohtml`, THE Poppler_Engine SHALL diizinkan menjalankan `pdfinfo` dari poppler-utils dengan argumen tetap hanya berupa jalur berkas masukan yang dihasilkan server, dengan aturan isolasi yang sama seperti `pdftohtml` dan tanpa argumen yang dikendalikan pengguna, untuk memperoleh jumlah halaman dan mengklasifikasikan dokumen terenkripsi atau rusak sebelum konversi; anggaran Subprocess_Timeout 30 detik SHALL dipakai bersama oleh `pdfinfo` dan `pdftohtml` dalam satu permintaan.
2. THE Poppler_Engine SHALL memberikan nilai `-f` dan `-l` hanya berupa bilangan bulat desimal ≥ 1 yang telah divalidasi pada Requirement 1 kriteria 4 dan 5, dengan `-f` ≤ `-l` dan `-l` ≤ jumlah halaman dokumen.
3. THE Poppler_Engine SHALL menjalankan subprocess dengan variabel lingkungan `HOME`, `XDG_CACHE_HOME`, `XDG_CONFIG_HOME`, dan `XDG_DATA_HOME` yang masing-masing mengarah ke jalur di dalam direktori sementara milik permintaan yang dikelola TempDirGuard, dan dengan nama file masukan yang dihasilkan server (bukan nama dari pengguna).
4. THE Poppler_Engine SHALL menjalankan subprocess langsung dari executable dengan daftar argumen terpisah, tanpa shell dan tanpa interpolasi string perintah.
5. WHEN subprocess selesai, gagal, atau dibatalkan (termasuk oleh Subprocess_Timeout 30 detik atau pemutusan koneksi klien), THE Poppler_Engine SHALL memastikan subprocess dan seluruh proses turunannya telah berhenti dalam 1 detik, sehingga jumlah proses turunan yang tersisa adalah 0.
6. IF keluaran subprocess melebihi 100 MB (104.857.600 byte), THEN THE Poppler_Engine SHALL menghentikan subprocess beserta seluruh proses turunannya, menghentikan pembacaan keluaran, dan THE PDF_Converter SHALL mengembalikan status HTTP 422 dengan kode galat `output_too_large` tanpa keluaran parsial.
7. IF `pdfinfo` atau `pdftohtml` keluar dengan kode non-nol dan standar-error subprocess menunjukkan bahwa kata sandi diperlukan, THEN THE Poppler_Engine SHALL melaporkan galat `encrypted`; IF `pdfinfo` atau `pdftohtml` keluar dengan kode non-nol dan standar-error menunjukkan struktur dokumen tidak valid, THEN THE Poppler_Engine SHALL melaporkan galat `corrupt`; IF `pdfinfo` atau `pdftohtml` keluar dengan kode non-nol dan standar-error tidak sesuai kedua pola tersebut, THEN THE Poppler_Engine SHALL melaporkan galat `corrupt`.
8. THE Poppler_Engine SHALL tidak menyalin standar-error subprocess ke log server maupun ke respons, dan SHALL hanya memakainya di dalam memori untuk klasifikasi galat pada kriteria 7.
9. THE Workspace SHALL memperlakukan poppler-utils (berlisensi GPL) hanya sebagai executable subprocess terpisah dan SHALL tidak menautkannya sebagai pustaka ke binary `disposable-server`, sehingga `Cargo.toml` pada workspace tidak memuat dependensi pustaka poppler.

### Requirement 9: Batas Ukuran dan Konfigurasi Lambda

**User Story:** Sebagai operator, saya ingin batas ukuran yang sesuai lingkungan, sehingga deployment Lambda tidak gagal pada payload besar.

#### Acceptance Criteria

1. WHEN Workspace mulai, THE Workspace SHALL membaca Max_Bytes dari variabel lingkungan `PDF_MD_MAX_BYTES` yang berisi bilangan bulat desimal dalam satuan byte, tanpa spasi, tanda, atau sufiks satuan.
2. WHERE `PDF_MD_MAX_BYTES` tidak diatur atau berisi string kosong, THE Workspace SHALL memakai Max_Bytes sebesar 26.214.400 byte pada server umum dan 5.242.880 byte pada Lambda_Runtime.
3. IF `PDF_MD_MAX_BYTES` diatur dengan nilai selain bilangan bulat desimal positif (misalnya `0`, nilai negatif, desimal pecahan, teks non-numerik, atau nilai yang melebihi rentang `u64`), THEN THE Workspace SHALL menolak start dengan kode keluar non-nol dan pesan galat yang menyebut nama variabel `PDF_MD_MAX_BYTES`, tanpa memakai nilai bawaan secara diam-diam.
4. IF `PDF_MD_MAX_BYTES` berupa bilangan bulat positif yang lebih besar daripada 26.214.400, THEN THE Workspace SHALL memakai Max_Bytes sebesar 26.214.400 byte dan mencatat tepat satu peringatan pada log saat start yang menyebut nama variabel dan batas yang dipakai, tanpa memuat data pengguna; IF nilainya tepat 26.214.400 atau kurang, THEN THE Workspace SHALL memakai nilai tersebut tanpa peringatan.
5. THE Workspace SHALL menerapkan batas body permintaan pada setiap rute PDF_Converter yang menerima unggahan sebesar Max_Bytes + 65.536 byte (64 KB untuk overhead multipart).
6. IF part `file` berukuran lebih dari Max_Bytes dan body permintaan masih dalam batas Max_Bytes + 65.536 byte, atau body permintaan melebihi batas tersebut, THEN THE PDF_Converter SHALL mengembalikan status HTTP 413 sesuai Requirement 2 kriteria 1, tanpa menjalankan Extraction_Engine.
7. WHEN PDF_Converter_Page dimuat, THE PDF_Converter_Page SHALL mengambil batas dari `GET /api/apps/pdf-to-markdown/limits` dan menampilkan Max_Bytes yang diformat dengan `formatFileSize` sebelum pengguna memilih file; IF pengambilan batas gagal, THEN THE PDF_Converter_Page SHALL menampilkan pesan galat dan tidak menampilkan angka batas tebakan.
8. WHEN pengguna dengan Authenticated_Session mengirim `GET /api/apps/pdf-to-markdown/limits`, THE PDF_Converter SHALL mengembalikan status HTTP 200 dengan Max_Bytes (bilangan bulat byte yang berlaku setelah penerapan kriteria 2 dan 4) dan Max_Pages (300).
9. IF `GET /api/apps/pdf-to-markdown/limits` dikirim tanpa Authenticated_Session, THEN THE PDF_Converter SHALL mengembalikan status HTTP 401, dan WHILE Burn_Gate aktif, THE PDF_Converter SHALL mengembalikan status HTTP 503 untuk rute tersebut.
10. WHERE Max_Response_Bytes berlaku (bawaan 5.767.168 byte pada Lambda_Runtime, dapat diubah lewat `PDF_MD_MAX_RESPONSE_BYTES`; tanpa batas pada server umum), IF ukuran body JSON Conversion_Result yang diserialisasi melebihi Max_Response_Bytes, THEN THE PDF_Converter SHALL mengembalikan status HTTP 422 dengan kode galat `output_too_large` dan pesan yang menganjurkan penggunaan rentang halaman (`page_from` dan `page_to`), tanpa mengembalikan body yang terpotong dan tanpa membuat Ephemeral_Download.

### Requirement 10: Ephemeral Download Hasil Konversi

**User Story:** Sebagai pengguna, saya ingin mengunduh hasil sebagai berkas `.md` satu kali pakai, sehingga hasil besar dapat diambil tanpa tersimpan di server.

#### Acceptance Criteria

1. WHERE permintaan `POST /api/apps/pdf-to-markdown` memuat query `download=true`, THE PDF_Converter SHALL menyimpan teks Markdown sebagai Ephemeral_Download dengan token UUID dan TTL 300 detik, serta menambahkan field `download_token` (string UUID) pada Conversion_Result yang berstatus HTTP 200.
2. WHERE permintaan konversi tidak memuat query `download=true` atau memuat nilai selain `true`, THE PDF_Converter SHALL tidak membuat Ephemeral_Download dan SHALL tidak menyertakan `download_token` pada Conversion_Result.
3. IF konversi gagal dengan galat apa pun, THEN THE PDF_Converter SHALL tidak membuat Ephemeral_Download, meskipun query `download=true` disertakan.
4. WHEN pengguna dengan Authenticated_Session mengirim `GET /api/apps/pdf-to-markdown/download/:token` dengan token valid yang belum kedaluwarsa dan belum pernah dipakai, THE PDF_Converter SHALL mengembalikan status HTTP 200 dengan badan berisi Markdown, header `Content-Type: text/markdown; charset=utf-8`, header `Content-Disposition` bertipe attachment dengan nama berkas `document.md`, dan header `Cache-Control: no-store`, lalu menghapus entri tersebut dari memori sehingga permintaan berikutnya dengan token yang sama mengembalikan HTTP 404.
5. IF token pada `GET /api/apps/pdf-to-markdown/download/:token` sudah pernah dipakai, telah melewati TTL 300 detik, tidak pernah dibuat, atau bukan UUID yang valid, THEN THE PDF_Converter SHALL mengembalikan status HTTP 404 dengan kode galat dan pesan yang menyatakan bahwa token tidak valid atau kedaluwarsa, tanpa membedakan keempat kondisi tersebut pada respons.
6. IF `GET /api/apps/pdf-to-markdown/download/:token` dikirim tanpa Authenticated_Session, THEN THE PDF_Converter SHALL mengembalikan status HTTP 401 dan SHALL mempertahankan entri Ephemeral_Download sehingga masih dapat diunduh oleh pengguna terautentikasi sampai TTL berakhir.
7. WHILE Burn_Gate aktif, THE PDF_Converter SHALL menolak `GET /api/apps/pdf-to-markdown/download/:token` dengan status HTTP 503 tanpa menghapus atau mengembalikan isi entri.
8. WHEN Burn Workspace dijalankan, THE Workspace SHALL menghapus seluruh Ephemeral_Download milik PDF_Converter bersama `downloads` lain, dengan buffer ditimpa nol sebelum dibebaskan, sehingga jumlah entri Ephemeral_Download PDF_Converter setelah burn adalah 0 dan token apa pun yang sebelumnya valid mengembalikan HTTP 404 atau 503.
9. WHEN TTL 300 detik sebuah Ephemeral_Download berakhir tanpa diunduh, THE PDF_Converter SHALL menimpa buffer entri dengan nol dan membebaskannya, sehingga entri tidak dapat diambil kembali.
10. THE PDF_Converter SHALL menamai berkas unduhan dengan nama tetap `document.md` pada header `Content-Disposition` tanpa memakai nama file unggahan.
11. FOR ALL Ephemeral_Download yang berhasil diunduh, THE PDF_Converter SHALL mengembalikan isi yang identik byte demi byte (encoding UTF-8) dengan `markdown` pada Conversion_Result yang menerbitkan token tersebut.
12. THE PDF_Converter SHALL menyertakan header `Cache-Control: no-store` pada setiap respons rute unduhan, termasuk respons 401, 404, dan 503.

### Requirement 11: Tool MCP `pdf_to_markdown`

**User Story:** Sebagai pengguna agen AI, saya ingin memanggil konversi PDF lewat MCP, sehingga agen dapat membaca dokumen tanpa meninggalkan Workspace.

#### Acceptance Criteria

1. WHEN klien MCP dengan Authenticated_Session memanggil `tools/list`, THE MCP_Server SHALL menyertakan tepat satu tool bernama `pdf_to_markdown` dengan deskripsi tidak kosong dan skema argumen: `file_base64` (wajib, string), `page_from` (opsional, bilangan bulat ≥ 1), `page_to` (opsional, bilangan bulat ≥ 1), `page_breaks` (opsional, boolean), dan `strip_headers_footers` (opsional, boolean), serta tetap memuat seluruh tool yang sudah ada (`convert_markdown_to_pdf`, `get_host_metrics`, `redact_secrets`, `inspect_jwt`, `scrub_metadata`, `burn_workspace_now`) dengan nama, deskripsi, dan skema yang tidak berubah.
2. WHEN klien MCP memanggil `pdf_to_markdown` dengan `file_base64` berupa base64 valid yang hasil dekodenya berukuran 1 byte sampai Max_Bytes dan berupa PDF valid dengan Text_Layer, THE MCP_Server SHALL mengembalikan hasil sukses berisi `markdown`, `pages`, `headings`, dan `warnings` yang nilainya identik dengan Conversion_Result endpoint `POST /api/apps/pdf-to-markdown` untuk PDF dan Conversion_Options yang sama, serta menerapkan nilai bawaan Conversion_Options yang sama dengan Requirement 1 kriteria 3 untuk argumen opsional yang tidak diberikan.
3. IF panjang `markdown` melebihi 100.000 karakter, THEN THE MCP_Server SHALL menyimpan hasil sebagai Ephemeral_Download (TTL 300 detik) dan mengembalikan URL unduhan, pratinjau berisi tepat 2.000 karakter pertama `markdown`, `pages`, `headings`, `warnings`, dan `truncated` bernilai true, tanpa mengembalikan seluruh teks.
4. IF panjang `markdown` tidak melebihi 100.000 karakter (termasuk tepat 100.000 karakter), THEN THE MCP_Server SHALL mengembalikan seluruh teks `markdown` tanpa URL unduhan, dengan `truncated` bernilai false dan tanpa membuat Ephemeral_Download.
5. IF argumen tidak valid (`file_base64` hilang atau bukan string, base64 tidak dapat didekode, ukuran hasil dekode 0 byte atau lebih dari Max_Bytes, `page_from` atau `page_to` bukan bilangan bulat ≥ 1, `page_from` lebih besar daripada `page_to`, `page_breaks` atau `strip_headers_footers` bukan boolean, atau argumen tidak dikenal bertipe salah), THEN THE MCP_Server SHALL mengembalikan galat JSON-RPC kode -32602 dengan pesan yang menyebut nama argumen bermasalah, tanpa menjalankan Extraction_Engine dan tanpa membuat direktori sementara.
6. IF hasil dekode berukuran 1 byte sampai Max_Bytes tetapi bukan PDF (tidak diawali `%PDF-`), rusak, terenkripsi, tanpa Text_Layer, melebihi Max_Pages, melewati Subprocess_Timeout, `page_from` melebihi jumlah halaman dokumen, keluaran melebihi batas 100 MB, atau mesin tidak tersedia, THEN THE MCP_Server SHALL mengembalikan galat JSON-RPC kode -32000 dengan field `data` berisi kode galat yang sama dengan Requirement 2 (`corrupt_pdf`, `encrypted_pdf`, `no_text_layer`, `too_many_pages`, `timeout`), Requirement 7 kriteria 7 (`engine_unavailable`), Requirement 8 kriteria 6 (`output_too_large`), atau `not_a_pdf` untuk bukan PDF, tanpa memuat nama file, isi file, atau jalur filesystem.
7. IF klien MCP tanpa Authenticated_Session memanggil `tools/call` untuk `pdf_to_markdown`, THEN THE MCP_Server SHALL menolak dengan status HTTP 401 tanpa membaca argumen, tanpa menjalankan Extraction_Engine, dan tanpa membuat direktori sementara.
8. WHILE Burn_Gate aktif, THE MCP_Server SHALL menolak setiap pemanggilan `pdf_to_markdown` dengan galat yang menyatakan bahwa Workspace sedang dihancurkan, tanpa memproses argumen dan tanpa menjalankan Extraction_Engine.
9. WHEN pemanggilan `pdf_to_markdown` selesai (berhasil atau gagal), THE MCP_Server SHALL menghapus direktori sementara beserta isinya dalam 1 detik dan SHALL tidak menuliskan `file_base64`, isi dokumen, teks `markdown`, atau standar-error subprocess ke log.

### Requirement 12: Zero-Footprint dan Privasi Log

**User Story:** Sebagai pengguna sadar privasi, saya ingin dokumen saya tidak tertinggal di server maupun log, sehingga privasi terjaga.

#### Acceptance Criteria

1. THE PDF_Converter SHALL menulis file PDF unggahan dan keluaran antara hanya di dalam direktori yang dikelola TempDirGuard, dan SHALL tidak menulis data pengguna ke jalur filesystem mana pun di luar direktori tersebut.
2. WHEN permintaan konversi selesai (berhasil, gagal, dibatalkan, atau timeout), THE PDF_Converter SHALL menghapus direktori sementara beserta seluruh isinya dalam waktu 1 detik sejak respons dikembalikan atau permintaan dibatalkan, sehingga jumlah berkas dan direktori sisa milik permintaan tersebut adalah 0.
3. IF klien memutus koneksi sebelum respons terkirim, THEN THE PDF_Converter SHALL menghentikan subprocess ekstraksi beserta seluruh proses turunannya, menghapus direktori sementara, dan membebaskan buffer terkait dalam waktu 1 detik sejak pemutusan terdeteksi, sehingga jumlah proses turunan dan berkas sisa milik permintaan tersebut adalah 0.
4. THE PDF_Converter SHALL tidak menuliskan nama file, isi file, teks hasil, atau metadata dokumen (termasuk isi standar-error subprocess dan nilai field `options`) ke log server pada tingkat log mana pun, baik pada jalur sukses maupun jalur galat.
5. THE Workspace SHALL tidak memakai database persisten untuk PDF_Converter dan SHALL tidak menyimpan hasil konversi di luar Ephemeral_Download, sehingga setelah Ephemeral_Download dikonsumsi, kedaluwarsa (300 detik), atau dihapus oleh Burn Workspace, jumlah salinan hasil konversi yang tersimpan adalah 0.
6. IF kegagalan I/O terjadi saat membuat atau menulis ke direktori sementara, THEN THE PDF_Converter SHALL menghapus berkas yang sempat dibuat dalam waktu 1 detik, tidak menjalankan Extraction_Engine, dan mengembalikan status HTTP 500 dengan kode galat `internal_error` pada badan JSON tanpa memuat jalur filesystem, nama file, atau isi dokumen.
7. THE PDF_Converter SHALL mengembalikan header `Cache-Control: no-store` pada setiap respons rute PDF_Converter, termasuk respons sukses (200) dan seluruh respons galat (400, 401, 404, 413, 415, 422, 500, 503, 504).

### Requirement 13: Halaman Frontend PDF to Markdown

**User Story:** Sebagai pengguna, saya ingin halaman khusus untuk mengonversi PDF, sehingga saya dapat memakainya tanpa alat tambahan.

#### Acceptance Criteria

1. THE Workspace SHALL menyediakan halaman frontend `/pdf-to-markdown` yang memakai PageShell sebagai kerangka halaman.
2. WHEN pengguna menjatuhkan atau memilih satu file pada PDF_Converter_Page, THE PDF_Converter_Page SHALL menampilkan nama file dan ukuran file yang diformat dengan `formatFileSize` dalam 500 ms setelah pemilihan dan sebelum permintaan konversi dikirim.
3. WHEN pengguna memilih lebih dari satu file sekaligus, THE PDF_Converter_Page SHALL hanya menerima file pertama (memakai `pickFirstFile`), mengabaikan file lainnya, dan menampilkan pemberitahuan bahwa hanya satu file yang diproses dalam 500 ms.
4. IF file yang dipilih berukuran lebih dari batas yang diterima dari server (Max_Bytes), atau ekstensinya bukan `.pdf` (tanpa membedakan huruf besar/kecil) dan tipe MIME-nya bukan `application/pdf`, THEN THE PDF_Converter_Page SHALL menampilkan pesan galat yang menyebut penyebab (ukuran melebihi batas atau bukan PDF), tidak mengirim permintaan, tidak menyimpan file tersebut sebagai file terpilih, dan mempertahankan kontrol pemilihan file dalam keadaan aktif.
5. THE PDF_Converter_Page SHALL menyediakan kontrol opsi: isian `page_from` dan `page_to` (bilangan bulat ≥ 1, kosong berarti nilai bawaan Requirement 1 kriteria 3), kotak centang `page_breaks` (tidak tercentang secara bawaan), dan kotak centang `strip_headers_footers` (tercentang secara bawaan).
6. IF isian `page_from` atau `page_to` berisi nilai yang bukan bilangan bulat ≥ 1, atau `page_from` lebih besar daripada `page_to`, THEN THE PDF_Converter_Page SHALL menampilkan pesan galat yang menyebut isian bermasalah, tidak mengirim permintaan, dan mempertahankan file yang dipilih.
7. WHILE permintaan konversi berjalan, THE PDF_Converter_Page SHALL menonaktifkan tombol konversi dan menampilkan indikator proses.
8. WHEN konversi berhasil, THE PDF_Converter_Page SHALL menampilkan hasil dengan dua mode tampilan yang dapat dipilih, yaitu pratinjau ter-render dan Markdown mentah (mode awal: pratinjau ter-render), serta menampilkan nilai `pages` dan `headings` dari Conversion_Result.
9. WHEN `warnings` pada Conversion_Result tidak kosong, THE PDF_Converter_Page SHALL menampilkan setiap entri warning sebagai satu butir pada satu daftar yang terlihat, dengan urutan yang sama seperti pada Conversion_Result.
10. WHEN pengguna menekan tombol salin, THE PDF_Converter_Page SHALL menyalin seluruh teks Markdown ke clipboard dan menampilkan konfirmasi selama 2 detik; IF penyalinan gagal, THEN THE PDF_Converter_Page SHALL menampilkan pesan kegagalan dan mempertahankan hasil di layar.
11. WHEN pengguna menekan tombol unduh `.md`, THE PDF_Converter_Page SHALL membuat berkas `document.md` dari teks Markdown di sisi klien dan memulai unduhan, dengan isi identik byte demi byte dengan `markdown` pada Conversion_Result.
12. IF server mengembalikan galat (HTTP 401, 413, 415, 422, 503, 504) atau permintaan gagal karena kesalahan jaringan, THEN THE PDF_Converter_Page SHALL menampilkan pesan yang berbeda untuk setiap status atau kode galat dan untuk kesalahan jaringan, mempertahankan file yang dipilih, menghapus hasil konversi sebelumnya dari layar, dan mengaktifkan kembali tombol konversi dalam 1 detik setelah galat diterima.
13. IF server mengembalikan galat `no_text_layer`, THEN THE PDF_Converter_Page SHALL menampilkan pesan yang menyatakan bahwa dokumen tampak berupa hasil pindai dan OCR belum didukung.
14. WHEN server mengembalikan status HTTP 401, THE PDF_Converter_Page SHALL mengarahkan pengguna ke halaman login dalam 1 detik.
15. WHEN pengguna meninggalkan halaman melalui navigasi atau unmount komponen, THE PDF_Converter_Page SHALL mengosongkan file, hasil, warnings, dan pesan dari state React.
16. THE PDF_Converter_Page SHALL tidak menulis nama file, isi file, atau hasil ke `localStorage`, `sessionStorage`, maupun cookie.
17. THE PDF_Converter_Page SHALL memakai token desain Tailwind v4 dari `frontend/src/styles/global.css` dan gaya `PRIMARY_BUTTON` dari `frontend/src/lib/ui.ts`, tanpa nilai warna heksadesimal yang ditulis langsung pada komponen.
18. WHEN PDF_Converter_Page me-render pratinjau Markdown, THE PDF_Converter_Page SHALL menampilkan tag HTML dan skrip yang terkandung pada teks hasil sebagai teks biasa, sehingga tidak ada elemen HTML atau skrip dari teks hasil yang dieksekusi atau muncul sebagai elemen DOM.

### Requirement 14: Navigasi pada Navbar

**User Story:** Sebagai pengguna, saya ingin menemukan PDF to Markdown dari Navbar tanpa Navbar menjadi penuh, sehingga navigasi tetap rapi.

#### Acceptance Criteria

1. THE Navbar SHALL menampilkan tepat satu tautan dengan teks "PDF to Markdown" yang atribut `href`-nya mengarah ke `/pdf-to-markdown`, sehingga jumlah tautan navigasi halaman menjadi 6 dan tautan yang sudah ada sebelumnya tetap ada tanpa perubahan tujuan.
2. WHILE lebar viewport 1024 piksel atau lebih (breakpoint `lg`), THE Navbar SHALL menampilkan keenam tautan navigasi, tombol Burn, dan kontrol akun secara bersamaan dalam satu baris, tanpa teks terpotong (tanpa ellipsis atau `overflow` tersembunyi), tanpa pembungkusan baris, dan tanpa bilah gulir horizontal pada halaman.
3. WHILE lebar viewport kurang dari 1024 piksel, THE Navbar SHALL menyembunyikan tautan navigasi dari baris utama dan menampilkan keenam tautan di dalam menu "Menu" yang dapat dilipat, yang tertutup secara bawaan dan terbuka serta tertutup saat tombol "Menu" ditekan.
4. IF keenam tautan beserta tombol Burn dan kontrol akun tidak muat dalam satu baris pada lebar viewport tepat 1024 piksel, THEN THE Navbar SHALL memakai label yang lebih singkat atau mengelompokkan tautan sehingga seluruh kontrol tetap terlihat tanpa menggulir horizontal, dan tautan tersebut tetap memiliki teks yang dapat diakses (label singkat atau `aria-label`) yang memuat "PDF to Markdown".
5. WHILE path halaman saat ini adalah `/pdf-to-markdown`, THE Navbar SHALL menandai tautan "PDF to Markdown" dengan atribut `aria-current="page"` dan gaya aktif yang berbeda dari tautan lain, dan SHALL tidak menetapkan `aria-current` pada tautan lain.
6. WHILE path halaman saat ini bukan `/pdf-to-markdown`, THE Navbar SHALL tidak menetapkan `aria-current` pada tautan "PDF to Markdown".
7. THE DashboardView SHALL menampilkan tepat satu kartu atau tautan dengan `href` `/pdf-to-markdown` yang memuat teks "PDF to Markdown", sehingga halaman tujuan dapat dibuka dengan satu klik dari DashboardView.
8. WHEN pengguna memilih tautan "PDF to Markdown" pada menu "Menu" mobile, THE Navbar SHALL menutup menu tersebut dan membuka halaman `/pdf-to-markdown`.

### Requirement 15: Paket Deployment dan Dokumentasi

**User Story:** Sebagai maintainer, saya ingin fitur ini siap dideploy di Docker umum dan AWS Lambda serta terdokumentasi, sehingga operasional konsisten.

#### Acceptance Criteria

1. THE Workspace SHALL memasang paket `poppler-utils` pada image runtime `deploy/Dockerfile` dan `deploy/Dockerfile.lambda` berbasis Debian bookworm, dengan paket yang tersedia untuk arsitektur arm64 dan amd64.
2. WHEN image build `deploy/Dockerfile` atau `deploy/Dockerfile.lambda` selesai, THE Workspace SHALL menyediakan executable `pdftohtml` pada `PATH` image runtime, sehingga menjalankan `pdftohtml -v` di dalam image berakhir dengan kode keluar 0 atau 99 dan mencetak string versi ke stderr.
3. THE Workspace SHALL mendaftarkan tepat tiga rute berikut pada `create_api_router`: `POST /api/apps/pdf-to-markdown`, `GET /api/apps/pdf-to-markdown/download/:token`, dan `GET /api/apps/pdf-to-markdown/limits`. Permintaan dengan metode yang didukung ke setiap rute SHALL menghasilkan status HTTP selain 404 yang disebabkan rute tidak terdaftar, dan permintaan dengan metode lain SHALL menghasilkan status HTTP 405.
4. THE Workspace SHALL meneruskan setiap rute PDF_Converter melalui extractor autentikasi dan Burn_Gate yang sama dengan micro-app lain, sehingga permintaan tanpa Authenticated_Session ke setiap rute menghasilkan status HTTP 401 dan permintaan selama Burn_Gate aktif menghasilkan status HTTP 503.
5. THE Workspace SHALL memperbarui `README.md` dan `AGENTS.md` sehingga masing-masing memuat seluruh butir berikut: (a) deskripsi mini app PDF to Markdown, (b) ketiga rute API pada kriteria 3, (c) nama tool MCP `pdf_to_markdown`, (d) variabel lingkungan `PDF_MD_ENGINE`, `PDF_MD_MAX_BYTES`, dan `PDF_MD_MAX_RESPONSE_BYTES`, (e) batas 25 MB, 5 MB (Lambda), 300 halaman, dan 30 detik, (f) cakupan Fase 1 yang menyebut bahwa tabel, OCR, multi-kolom, dan ekstraksi gambar tidak didukung, dan (g) catatan bahwa poppler berlisensi GPL dijalankan hanya sebagai subprocess.
6. THE Workspace SHALL menambahkan `PDF_MD_ENGINE` (bawaan `poppler`), `PDF_MD_MAX_BYTES` (bawaan 26.214.400 pada server umum dan 5.242.880 pada Lambda_Runtime), dan `PDF_MD_MAX_RESPONSE_BYTES` (bawaan 5.767.168 pada Lambda_Runtime dan tanpa batas pada server umum) sebagai baris pada tabel variabel lingkungan runtime di `AGENTS.md` dan `README.md`, masing-masing dengan kolom nilai bawaan dan deskripsi terisi.
7. WHEN perintah `teardown` CLI dijalankan, THE CLI SHALL berperilaku sama seperti sebelum fitur ini ditambahkan, dengan urutan perintah `docker compose` yang sama dan tanpa memanggil atau bergantung pada PDF_Converter maupun Burn Workspace.

### Requirement 16: Pengujian Otomatis

**User Story:** Sebagai maintainer, saya ingin pengujian yang menjaga kebenaran dan privasi konversi, sehingga regresi cepat terdeteksi.

#### Acceptance Criteria

1. THE Workspace SHALL menyertakan, untuk masing-masing dari lima komponen berikut, paling sedikit satu pengujian jalur sukses dan satu pengujian jalur gagal yang dijalankan oleh `cargo test --workspace`: PDF_Converter (endpoint HTTP), Structure_Builder, Poppler_Engine, Ephemeral_Download milik PDF_Converter, dan tool MCP `pdf_to_markdown`.
2. THE Workspace SHALL menyertakan pengujian berbasis properti untuk round trip dengan paling sedikit 100 kasus acak per eksekusi: FOR ALL dokumen Markdown yang dihasilkan generator (1 sampai 5 heading dan 1 sampai 10 paragraf, tanpa tabel, gambar, atau kolom ganda) yang dikonversi ke PDF oleh konverter Markdown ke PDF yang sudah ada lalu ke Markdown oleh PDF_Converter dengan `strip_headers_footers` bernilai true, teks setiap heading SHALL muncul pada `markdown` dengan urutan relatif yang sama dan urutan teks paragraf SHALL tetap sama.
3. THE Workspace SHALL menyertakan pengujian berbasis properti untuk preservasi konten dengan paling sedikit 100 kasus acak per eksekusi: FOR ALL struktur dokumen perantara yang dihasilkan generator, deret kata pada keluaran Structure_Builder (setelah menghapus sintaks Markdown yang ditambahkan Structure_Builder) SHALL sama dengan deret kata pada masukan, kecuali kata yang digabung oleh hyphenation join dan baris yang dihapus sebagai Repeated_Line atau Page_Number_Line.
4. THE Workspace SHALL menyertakan pengujian berbasis properti untuk determinisme dan format dengan paling sedikit 100 kasus acak per eksekusi: FOR ALL struktur dokumen perantara dan Conversion_Options, menjalankan Structure_Builder dua kali dengan masukan dan opsi yang sama SHALL menghasilkan teks identik byte demi byte, dan teks hasil SHALL tidak memuat lebih dari satu baris kosong berturut-turut serta SHALL diakhiri tepat satu karakter baris baru.
5. THE Workspace SHALL menyertakan pengujian berbasis properti untuk penolakan input tidak valid dengan paling sedikit 100 kasus acak per eksekusi: FOR ALL deret byte acak berukuran 0 sampai 4.096 byte yang tidak diawali `%PDF-`, THE PDF_Converter SHALL mengembalikan status HTTP 415, tidak menjalankan Extraction_Engine, dan jumlah berkas pada direktori sementara Workspace SHALL sama dengan jumlah sebelum permintaan.
6. THE Workspace SHALL menyertakan pengujian batas ukuran tepat dengan Max_Bytes dikonfigurasi pada nilai kecil (paling banyak 1.024 byte): file berukuran tepat Max_Bytes SHALL diterima untuk diproses (tidak mengembalikan 413), dan file berukuran Max_Bytes + 1 byte SHALL ditolak dengan HTTP 413.
7. THE Workspace SHALL menyertakan pengujian bahwa untuk setiap jalur galat HTTP 400, 401, 413, 415, 422, 503, dan 504, jumlah berkas dan direktori sisa milik permintaan di direktori sementara adalah 0 dalam 1 detik setelah respons dikembalikan.
8. THE Workspace SHALL menyertakan pengujian yang menangkap seluruh keluaran log selama satu konversi sukses dan satu konversi gagal dengan PDF yang memuat penanda teks unik dan nama file unggahan unik, dan memverifikasi bahwa penanda tersebut, nama file unggahan, dan teks hasil tidak muncul pada keluaran log.
9. THE Workspace SHALL menyertakan pengujian untuk masing-masing rute PDF_Converter (`/api/apps/pdf-to-markdown`, `/api/apps/pdf-to-markdown/download/:token`, dan `/api/apps/pdf-to-markdown/limits`) yang memverifikasi bahwa permintaan tanpa Authenticated_Session mengembalikan HTTP 401 dan bahwa permintaan selama Burn_Gate aktif mengembalikan HTTP 503.
10. THE Workspace SHALL menyertakan pengujian bahwa token Ephemeral_Download milik PDF_Converter ditolak dengan HTTP 404 pada pemakaian kedua, ditolak dengan HTTP 404 setelah TTL 300 detik berakhir (memakai jam yang dapat dimajukan atau TTL yang dikonfigurasi pendek pada pengujian), dan bahwa Burn Workspace menghapus entri tersebut sehingga pemakaian token setelah burn tidak mengembalikan isi Markdown.
11. THE Workspace SHALL menyertakan pengujian untuk Engine_Registry yang memverifikasi tiga kasus: `PDF_MD_ENGINE` tidak diatur atau kosong memilih Poppler_Engine, nilai tidak terdaftar ditolak dengan galat yang menyebut nama variabel dan daftar nilai valid, dan Extraction_Engine tiruan yang terdaftar dapat dipilih serta menghasilkan Conversion_Result tanpa memanggil poppler.
12. IF `pdftohtml` tidak tersedia di lingkungan pengujian, THEN THE Workspace SHALL melewati hanya pengujian yang membutuhkan poppler dengan pesan skip yang menyebut `pdftohtml` tidak ditemukan, dan pengujian Structure_Builder serta Engine_Registry SHALL tetap berjalan dan lulus tanpa poppler.
13. THE Workspace SHALL menyertakan pengujian komponen frontend dengan vitest untuk PDF_Converter_Page yang memverifikasi masing-masing hal berikut dalam pengujian terpisah: hanya file pertama yang diterima saat beberapa file dipilih, penolakan file bukan PDF, penolakan file melebihi batas dari server tanpa mengirim permintaan, tampilan hasil beserta jumlah halaman, jumlah heading, dan daftar `warnings`, tombol salin (sukses dan gagal), tombol unduh `document.md`, tampilan pesan untuk setiap kode status galat 401, 413, 415, 422, 503, 504 dan kesalahan jaringan (termasuk pesan khusus `no_text_layer`), pengosongan state saat unmount, dan tidak adanya penulisan ke `localStorage`, `sessionStorage`, maupun cookie.
14. THE Workspace SHALL menyertakan pengujian komponen Navbar dengan vitest yang memverifikasi tiga hal dalam pengujian terpisah: tautan "PDF to Markdown" ke `/pdf-to-markdown` ada pada tampilan desktop, tautan tersebut memiliki `aria-current="page"` saat `/pdf-to-markdown` aktif, dan tautan tersebut ada pada menu "Menu" mobile.
15. IF salah satu pengujian pada requirement ini gagal, THEN THE Workspace SHALL membuat `cargo test --workspace` atau `vitest --run` berakhir dengan kode keluar bukan nol dan menampilkan nama pengujian yang gagal pada keluarannya.
