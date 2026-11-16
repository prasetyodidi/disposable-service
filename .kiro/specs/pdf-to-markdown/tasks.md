# Implementation Plan: PDF to Markdown

## Overview

Rencana ini mengimplementasikan mini app PDF to Markdown: inti konversi murni di `crates/server/src/pdf2md/`, antarmuka mesin ekstraksi pluggable dengan Poppler_Engine, adaptor route REST, tool MCP `pdf_to_markdown`, halaman frontend `/pdf-to-markdown`, serta paket deployment dan dokumentasi. Bahasa implementasi mengikuti design: Rust (crate `disposable-server` dan `disposable-common`) untuk backend dan TypeScript/React (Astro) untuk frontend.

Urutan kerja: fondasi (dependensi, DTO, konfigurasi) -> inti murni (model, parser XML, Structure_Builder) -> mesin ekstraksi dan `convert()` -> adaptor route HTTP -> tool MCP -> frontend -> deployment, dokumentasi, dan smoke test. Setiap langkah berakhir dengan keterhubungan ke langkah sebelumnya, tanpa kode yatim.

Konvensi pengujian:
- Seluruh tes Rust berada di `crates/server/src/pdf2md/tests/` (`#[cfg(test)] mod tests;` di `pdf2md/mod.rs`). Task 1.1 membuat `mod.rs` dan berkas kosong `gen_*.rs`, `prop_01.rs` s.d. `prop_38.rs` (hanya `prop_01` s.d. `prop_33` untuk Rust), `unit_*.rs`, `mock_engine.rs`, `helpers.rs`, dan `smoke.rs` agar tiap task pengujian hanya mengisi berkasnya sendiri.
- Setiap tes properti Rust memakai `proptest` (minimal 100 kasus) dengan komentar tag `// Feature: pdf-to-markdown, Property N: <teks>`. Property 33 memakai tepat 100 kasus.
- Tes properti frontend memakai `fast-check` + `vitest` (`numRuns: 100`) di `frontend/src/lib/*.test.ts` dan `frontend/src/components/*.test.tsx`.
- Tes yang membutuhkan poppler memakai makro `skip_unless_tool!`; `PDF_MD_REQUIRE_TOOLS=1` mengubah skip menjadi panic (diatur di kontainer Docker, task 5). `cargo test --workspace` harus hijau tanpa poppler.
- Tes unit dan smoke yang diwajibkan Requirement 16 tidak diberi tanda `*`; tes properti dan tes tambahan lain bersifat opsional (`*`).

## Tasks

- [x] 1. Fondasi proyek
  - [x] 1.1 Tambah dependensi dan kerangka modul `pdf2md/`
    - Tambah `quick-xml` (seri 0.37, versi dipin dengan `=`) dan `libc` (`cfg(unix)`, dipin) di `crates/server/Cargo.toml`; jangan menambah crate poppler
    - Buat `crates/server/src/pdf2md/` dengan `mod.rs` (submodul `config`, `error`, `model`, `options`, `engine`, `proc`, `poppler`, `xml`, `structure`, `convert`, dan `#[cfg(test)] mod tests;`), berkas kerangka `structure/{mod,rows,strip,blocks,inline,render}.rs`, dan daftarkan `mod pdf2md;` di `main.rs`
    - Buat `pdf2md/tests/mod.rs` yang mendeklarasikan `gen_doc`, `gen_xml`, `gen_options`, `gen_markdown`, `gen_pdf`, `mock_engine`, `helpers`, `prop_01` s.d. `prop_33`, `unit_xml`, `unit_structure`, `unit_registry`, `unit_proc`, `unit_poppler`, `unit_convert`, `unit_routes`, `unit_download`, `unit_mcp`, `smoke`, beserta berkas kosong untuk masing-masing
    - Pindahkan isi `privacy/tests/helpers_http.rs` ke `crates/server/src/test_support/http.rs` (`#[cfg(test)]`), dan jadikan berkas lama `pub(crate) use` ulang agar tes lama tidak berubah
    - _Requirements: 8.9, 12.5, 16.1_

  - [x] 1.2 Tambah DTO bersama dan tipe TypeScript
    - Di `crates/common/src/lib.rs`: `PdfConversionResult` (dengan `download_token` opsional `skip_serializing_if`), `PdfLimits`, `PdfApiError`, `PdfMcpResult` (derive `Debug, Clone, Serialize, Deserialize, PartialEq`)
    - Buat `frontend/src/types/pdf.ts` yang mencerminkan DTO tersebut
    - _Requirements: 1.1, 9.8, 10.1, 11.2, 11.3_

  - [x] 1.3 Implementasikan `pdf2md/config.rs` dan `pdf2md/error.rs`
    - `PdfConfig::from_env_with(&dyn Fn(&str) -> Option<String>)` untuk `PDF_MD_MAX_BYTES` (hanya `^[0-9]+$`, ≥ 1, muat `u64`; kosong/tidak diatur memakai bawaan 26.214.400 atau 5.242.880 bila `AWS_LAMBDA_FUNCTION_NAME` tidak kosong; > 26.214.400 dijepit dengan satu peringatan) dan `PDF_MD_MAX_RESPONSE_BYTES` (aturan pengurai sama, tanpa batas atas; bawaan 5.767.168 pada Lambda, `None` pada server umum); `ConfigError` yang menyebut nama variabel; konstanta `MAX_PAGES`, `SUBPROCESS_TIMEOUT`, `MAX_OUTPUT_BYTES`, `DOWNLOAD_TTL_SECS`, `body_limit()`
    - `ErrorCode` (status HTTP, `code_str`, `priority`) dan `ConvertError`, `EngineError` (delapan jenis termasuk `Unavailable`, `OutputTooLarge`, `PageFromOutOfRange{total_pages}`) sesuai tabel pemetaan design; pesan galat statis tanpa nama file, isi, jalur, atau stderr
    - _Requirements: 2.11, 2.12, 7.1, 9.1, 9.2, 9.3, 9.4, 9.5, 9.10_

  - [x] 1.4 Tulis property test parsing konfigurasi
    - **Property 25: Parsing konfigurasi `PDF_MD_MAX_BYTES`**
    - Berkas: `pdf2md/tests/prop_25.rs`
    - **Validates: Requirements 9.1, 9.2, 9.3, 9.4**

- [x] 2. Inti murni: model, opsi, parser XML, Structure_Builder
  - [x] 2.1 Implementasikan `pdf2md/model.rs` dan `pdf2md/options.rs`
    - `IntermediateDoc`, `Page`, `TextLine`, `Span` (koordinat dan ukuran integer, invarian `mono == all(span.mono)`)
    - `ConversionOptions` dan `parse_options(Option<&[u8]>)` manual atas `serde_json::Value`: batas 4.096 byte, field tidak dikenal, tipe salah (`null`, float, string), nilai < 1, `page_from > page_to`; galat menyebut nama field atau `options`
    - `resolve_range(from, to, total, max_pages)` murni (`FromBeyondTotal`, `TooManyPages`, peringatan penyesuaian `page_to`)
    - _Requirements: 1.2, 1.3, 1.4, 1.5, 1.6, 2.7, 5.7_

  - [x] 2.2 Implementasikan `pdf2md/xml.rs`
    - `decode_entities(&str) -> String` satu lintasan (lima entitas bernama, `&#N;`, `&#xH;`; referensi di luar rentang atau kontrol C0 -> U+FFFD; `&` liar dan entitas tak dikenal tetap literal)
    - `parse_pdftohtml_xml(&[u8])` dengan `quick-xml` (`trim_text(false)`, `check_end_names(false)`), `<fontspec>` -> `FontInfo` (deteksi mono/bold/italic dari nama keluarga), `<page>`, `<text>`, tag inline `<b>`/`<i>`, dekode entitas hanya lewat `decode_entities`; `XmlError::{Malformed, NoPages}`
    - _Requirements: 6.4, 6.5, 8.1_

  - [x] 2.3 Implementasikan `structure/rows.rs`
    - `order_and_merge(Page) -> Page`: urut `(y, x)` stabil, penggabungan baris visual (tepi bawah sejajar), penyisipan spasi berdasarkan celah horizontal, `size` dominan dan `mono` hasil gabungan
    - _Requirements: 6.1, 6.3_

  - [x] 2.4 Implementasikan `structure/strip.rs`
    - `strip(&mut [Page]) -> u32` dengan zona atas/bawah 10% tinggi halaman, `normalize` (digit -> `#`), `is_page_number_line` (desimal, Romawi, "Page N"/"Halaman N", "N of M"), Repeated_Line (≥ 3 halaman dan ≥ 50%, hanya bila `pages ≥ 3`); baris di area isi tidak pernah dihapus
    - _Requirements: 5.1, 5.2, 5.3, 5.4, 5.7, 5.8_

  - [x] 2.5 Implementasikan `structure/blocks.rs`
    - `body_font_size`, `is_heading_size` (aritmetika integer `size*100 >= body*115`), `heading_levels` (enam ukuran terbesar, sisanya level 6, monoton, baris monospace tidak pernah heading)
    - `Block` dan klasifikasi per baris dengan prioritas kode > heading > bullet > ordered > teks; pengelompokan heading, paragraf (`2*pitch <= 3*tinggi`), item daftar dengan lanjutan, nesting daftar berbasis tumpukan indentasi (maks 4 tingkat), blok kode dengan indentasi relatif
    - `join_lines` untuk hyphenation join dan penghitung `hyphen_joins`
    - _Requirements: 3.1, 3.2, 3.3, 3.4, 3.5, 3.9, 4.1, 4.2, 4.3, 4.4, 4.5, 4.6, 4.7, 4.10_

  - [x] 2.6 Implementasikan `structure/inline.rs` dan `structure/render.rs`
    - `escape_markdown(s, at_line_start)` dan `unescape_markdown`; `render_spans` (normalisasi gaya, `**`/`*`/`***`, spasi tepi di luar pembungkus, satu pasang penanda bila seluruh blok bergaya sama, tanpa pembungkus di blok kode)
    - `render(items)`: heading `#`×level, daftar berindentasi 2 spasi per tingkat, pagar kode dengan panjang `max(3, backtick_terpanjang+1)` tanpa escape/gaya/hyphenation, `---` untuk `PageBreak`, blok digabung `\n\n`, akhir tepat satu `\n`
    - _Requirements: 3.6, 3.7, 3.8, 3.10, 3.11, 4.6, 4.8, 4.9_

  - [x] 2.7 Rangkai `structure/mod.rs` (`build`) dan `join_pages`
    - `BuildOptions`, `BuildOutput`, `build(&IntermediateDoc, &BuildOptions)` murni dan deterministik: urut/gabung -> strip -> body font -> level heading -> klasifikasi -> `join_pages` -> render; satu entri peringatan "N baris header/footer dihapus" bila N ≥ 1
    - `join_pages`: pemisah `---` tepat `P-1` bila `page_breaks`; bila tidak, paragraf lintas halaman digabung hanya bila baris terakhir tidak berakhir `.`, `!`, `?`, `:` dan blok berikutnya adalah paragraf; blok kode bersebelahan lintas halaman digabung
    - Pastikan modul `structure/` tidak mengimpor `poppler`, `xml`, atau `proc`
    - _Requirements: 3.10, 5.1, 5.2, 5.3, 5.4, 5.5, 5.6, 6.1, 6.2, 6.6_

  - [x] 2.8 Tulis generator di `pdf2md/tests/`
    - `gen_doc.rs`: `arb_text()` (ASCII, Latin beraksen, Cyrillic, CJK, karakter khusus XML dan Markdown), `arb_span`, `arb_line`, `arb_page`, `arb_intermediate_doc` (1 sampai 8 halaman) beserta varian heading/daftar/kode/jarak paragraf
    - `gen_xml.rs`: `to_pdftohtml_xml(&IntermediateDoc)`; `gen_options.rs`: `arb_valid_options`, `arb_invalid_options`; `gen_markdown.rs`: dokumen Markdown untuk round trip (heading 1 sampai 5, paragraf 1 sampai 10, tanpa karakter Markdown khusus dan tanpa `-`)
    - _Requirements: 16.2, 16.3, 16.4_

  - [x] 2.9 Tulis property test opsi valid
    - **Property 1: Opsi valid diterapkan dan bawaan diisi**
    - Berkas: `pdf2md/tests/prop_01.rs`
    - **Validates: Requirements 1.2, 1.3**

  - [x] 2.10 Tulis property test opsi tidak valid
    - **Property 2: Opsi tidak valid ditolak dengan menyebut field**
    - Berkas: `pdf2md/tests/prop_02.rs` (tingkat fungsi `parse_options`; tingkat HTTP dilengkapi di task 6.x)
    - **Validates: Requirements 1.4**

  - [x] 2.11 Tulis property test resolusi rentang halaman
    - **Property 3: Resolusi rentang halaman**
    - Berkas: `pdf2md/tests/prop_03.rs`
    - **Validates: Requirements 1.5, 1.6, 2.7, 5.7**

  - [x] 2.12 Tulis property test dekode entitas
    - **Property 9: Dekode entitas tepat satu kali**
    - Berkas: `pdf2md/tests/prop_09.rs`
    - **Validates: Requirements 6.4, 6.5**

  - [x] 2.13 Tulis property test round trip XML
    - **Property 10: Round trip penguraian XML pdftohtml**
    - Berkas: `pdf2md/tests/prop_10.rs`
    - **Validates: Requirements 6.4, 6.5, 8.1**

  - [x] 2.14 Tulis property test determinisme dan format
    - **Property 11: Determinisme dan format keluaran**
    - Berkas: `pdf2md/tests/prop_11.rs`
    - **Validates: Requirements 3.10, 6.6, 16.4**

  - [x] 2.15 Tulis property test preservasi konten
    - **Property 12: Preservasi konten**
    - Berkas: `pdf2md/tests/prop_12.rs`
    - **Validates: Requirements 6.1, 6.2, 6.4, 16.3**

  - [x] 2.16 Tulis property test urutan baca
    - **Property 13: Urutan baca**
    - Berkas: `pdf2md/tests/prop_13.rs`
    - **Validates: Requirements 6.3**

  - [x] 2.17 Tulis property test heading menurut ukuran font
    - **Property 14: Heading menurut ukuran font**
    - Berkas: `pdf2md/tests/prop_14.rs`
    - **Validates: Requirements 3.1, 3.2, 3.9**

  - [x] 2.18 Tulis property test paragraf menurut jarak vertikal
    - **Property 15: Paragraf menurut jarak vertikal**
    - Berkas: `pdf2md/tests/prop_15.rs`
    - **Validates: Requirements 3.3, 3.4**

  - [x] 2.19 Tulis property test hyphenation join
    - **Property 16: Hyphenation join**
    - Berkas: `pdf2md/tests/prop_16.rs`
    - **Validates: Requirements 3.5**

  - [x] 2.20 Tulis property test gaya teks
    - **Property 17: Gaya teks dibungkus dengan benar**
    - Berkas: `pdf2md/tests/prop_17.rs`
    - **Validates: Requirements 3.6, 3.7**

  - [x] 2.21 Tulis property test round trip escape
    - **Property 18: Round trip escape Markdown**
    - Berkas: `pdf2md/tests/prop_18.rs`
    - **Validates: Requirements 3.8**

  - [x] 2.22 Tulis property test daftar dan nesting
    - **Property 19: Daftar dan nesting**
    - Berkas: `pdf2md/tests/prop_19.rs`
    - **Validates: Requirements 4.1, 4.2, 4.3, 4.4, 4.5**

  - [x] 2.23 Tulis property test blok kode
    - **Property 20: Blok kode**
    - Berkas: `pdf2md/tests/prop_20.rs`
    - **Validates: Requirements 4.6, 4.7, 4.8, 4.9, 4.10**

  - [x] 2.24 Tulis property test penghapusan header, footer, dan nomor halaman
    - **Property 21: Penghapusan header, footer, dan nomor halaman**
    - Berkas: `pdf2md/tests/prop_21.rs`
    - **Validates: Requirements 5.1, 5.2, 5.3, 5.4, 5.8**

  - [x] 2.25 Tulis property test pemisah halaman
    - **Property 22: Pemisah halaman dan penyambungan antar halaman**
    - Berkas: `pdf2md/tests/prop_22.rs`
    - **Validates: Requirements 5.5, 5.6**

  - [x] 2.26 Tulis unit test Structure_Builder dan parser XML
    - Sukses: contoh `pdftohtml` tulisan tangan (heading, paragraf, daftar bersarang, blok kode, tebal/miring, header/footer berulang, `page_breaks`); gagal: XML tidak terbentuk -> `Malformed`, tanpa halaman -> `NoPages`, dokumen tanpa teks; kasus `&amp;lt;` menghasilkan `&lt;`; heading seluruhnya tebal menjadi `# **Judul**`
    - Berkas: `pdf2md/tests/unit_xml.rs` dan `pdf2md/tests/unit_structure.rs`
    - _Requirements: 3.11, 6.5, 16.1_

- [x] 3. Checkpoint - Inti murni
  - Ensure all tests pass, ask the user if questions arise.

- [x] 4. Mesin ekstraksi, registry, dan orkestrasi `convert()`
  - [x] 4.1 Implementasikan `pdf2md/engine.rs` dan `PdfRuntime`
    - `trait ExtractionEngine` (`async_trait`), `ExtractRequest`, `EngineRegistry` (`with_builtin`, `register`, `valid_names`, `select` dengan trim + lowercase, kosong/spasi -> `poppler`), `RegistryError` dengan pesan memuat `PDF_MD_ENGINE` dan nilai valid
    - Di `pdf2md/mod.rs`: `PdfRuntime { config, engine, job_root }`, `from_env()` (membaca `PdfConfig` dan memilih mesin; `StartupError`), `defaults()`
    - _Requirements: 7.1, 7.2, 7.3, 7.4, 7.5_

  - [x] 4.2 Implementasikan `pdf2md/proc.rs`
    - `Deadline`, `ProcSpec`, `ProcOutput`, `ProcError`, `run_bounded`: tanpa shell, `env_clear` + `PATH`/`HOME`/`XDG_*` di `work_dir`/`LC_ALL`, `stdin` null, grup proses baru (`process_group(0)`), `PgidGuard` dengan `libc::killpg(SIGKILL)` pada `Drop`, batas stdout/stderr, timeout kumulatif, `OutputTooLarge`; non-Unix mengembalikan `NotFound`
    - _Requirements: 2.8, 8.3, 8.4, 8.5, 8.6, 12.3_

  - [x] 4.3 Implementasikan `pdf2md/poppler.rs`
    - `PopplerEngine` (`with_binaries` untuk tes) dengan `pdfinfo` (jumlah halaman, `parse_pages_line`, klasifikasi awal) lalu `pdftohtml -xml -i -q -stdout -f <from> -l <last> <input.pdf>`; pembangun argumen sebagai fungsi murni; `resolve_range` dipakai sebelum ekstraksi
    - `classify_stderr` (`Encrypted` bila pola sandi, selain itu `Corrupt`) dan `map_proc` (`NotFound|SpawnFailed` -> `Unavailable`); stderr tidak pernah dilog
    - _Requirements: 7.7, 8.1, 8.2, 8.3, 8.7, 8.8_

  - [x] 4.4 Implementasikan `pdf2md/convert.rs`
    - `ConvertInput`, `Conversion`, `convert(rt, input)` dengan urutan: 413 -> 415 -> 400 -> `make_job_dir(root)` (TempDirGuard di `workspace_tmp_root()/pdf2md-job-<uuid>`) -> tulis `input.pdf` lalu `drop(file)` -> `Deadline` -> `extract` -> 422 tanpa teks -> peringatan (file tambahan, `page_to` disesuaikan) -> `structure::build` di `spawn_blocking` (tangkap `JoinError` sebagai `Internal`) -> 422 `no_text_layer` bila tanpa kata -> batas `max_response_bytes` (`output_too_large`)
    - `shape_mcp_result` (ambang 100.000 karakter Unicode, pratinjau 2.000 karakter)
    - _Requirements: 1.1, 1.5, 1.8, 1.9, 2.1, 2.2, 2.3, 2.4, 2.5, 2.6, 2.7, 2.8, 2.12, 5.4, 9.10, 11.3, 11.4, 12.1, 12.2, 12.6_

  - [x] 4.5 Tulis `MockEngine`, helper tes, dan makro skip
    - `mock_engine.rs`: `MockEngine` (`MockOutcome::{Doc, Err, Sleep}`, hitungan `calls`)
    - `helpers.rs`: `skip_unless_tool!` (cek `PATH`, pesan `SKIP: pdftohtml tidak ditemukan di PATH; tes ini membutuhkan poppler-utils`, panic bila `PDF_MD_REQUIRE_TOOLS=1`), `authed_pdf_app`, `pdf_bytes(len)`, `tmp_listing()`, penangkap log `tracing`
    - `gen_pdf.rs`: pembangun PDF minimal dengan `lopdf` dan fixture `encrypted.pdf`, `corrupt.pdf`, `scanned-like.pdf` di `pdf2md/tests/fixtures/`
    - _Requirements: 7.6, 16.1, 16.12_

  - [x] 4.6 Tulis property test klasifikasi stderr
    - **Property 23: Klasifikasi stderr**
    - Berkas: `pdf2md/tests/prop_23.rs`
    - **Validates: Requirements 8.7**

  - [x] 4.7 Tulis property test argumen dan lingkungan subprocess
    - **Property 24: Argumen dan lingkungan subprocess**
    - Berkas: `pdf2md/tests/prop_24.rs`
    - **Validates: Requirements 8.1, 8.2, 8.3, 8.4**

  - [x] 4.8 Tulis property test pemilihan mesin
    - **Property 26: Pemilihan mesin**
    - Berkas: `pdf2md/tests/prop_26.rs`
    - **Validates: Requirements 7.2, 7.3, 7.4**

  - [x] 4.9 Tulis unit test Engine_Registry
    - Tiga kasus: tidak diatur/kosong memilih `poppler`; nilai tak terdaftar ditolak dengan pesan memuat `PDF_MD_ENGINE` dan daftar valid; `MockEngine` terdaftar dipilih dan `convert` menghasilkan hasil tanpa memanggil poppler
    - Berkas: `pdf2md/tests/unit_registry.rs`
    - _Requirements: 7.2, 7.3, 7.4, 7.6, 16.11_

  - [x] 4.10 Tulis unit test `run_bounded` dan pemetaan galat `convert`
    - `run_bounded` (Unix): `sleep` melewati anggaran -> `Timeout` dan grup proses mati < 1 detik (`kill(-pgid, 0)` -> `ESRCH`); `yes` melewati `max_stdout` -> `OutputTooLarge`; pembatalan future mematikan grup
    - `convert` dengan `MockEngine`: satu tes per `EngineError` (status, kode, tanpa `markdown`), 300 halaman diterima dan 301 `too_many_pages`
    - Berkas: `pdf2md/tests/unit_proc.rs` dan `pdf2md/tests/unit_convert.rs`
    - _Requirements: 2.4, 2.5, 2.6, 2.7, 2.8, 8.5, 8.6, 16.1_

  - [x] 4.11 Tulis unit test Poppler_Engine
    - Selalu berjalan: `PopplerEngine::with_binaries("tidak-ada-xyz", "tidak-ada-xyz")` -> `engine_unavailable`, direktori sementara bersih
    - Dengan `skip_unless_tool!("pdftohtml")`: ekstraksi PDF nyata sukses, PDF bersandi -> `encrypted`, PDF terpotong -> `corrupt`, PDF tanpa teks -> `no_text_layer`, rentang halaman, `page_from` di luar rentang
    - Berkas: `pdf2md/tests/unit_poppler.rs`
    - _Requirements: 7.7, 8.1, 8.7, 16.1, 16.12_

  - [x] 4.12 Tulis property test bentuk Conversion_Result
    - **Property 32: Bentuk Conversion_Result**
    - Berkas: `pdf2md/tests/prop_32.rs`
    - **Validates: Requirements 1.1**

  - [x] 4.13 Tulis property test round trip Markdown -> PDF -> Markdown
    - **Property 33: Round trip Markdown ke PDF ke Markdown**
    - Berkas: `pdf2md/tests/prop_33.rs`; pakai `skip_unless_tool!("pdftohtml")` dan `skip_unless_tool!("pandoc")`, uji pendahuluan `generate_pandoc_pdf_bytes` (skip bila mesin PDF tidak tersedia), normalisasi ligatur U+FB00 sampai U+FB06, perbandingan deret kata (bukan batas paragraf)
    - **Validates: Requirements 6.7, 16.2**

- [x] 5. Lingkungan uji berbasis Docker
  - [x] 5.1 Buat `deploy/Dockerfile.test`
    - Basis `rust:1.95-slim-bookworm` (tag sama dengan `deploy/Dockerfile.lambda`); pasang `poppler-utils` (`pdftohtml`, `pdfinfo`), `libssl-dev`, `pkg-config`, `ca-certificates`, `curl`, `xz-utils`
    - Pasang Pandoc 3.6.3 dan Typst 0.12.0 dengan cara yang sama seperti `deploy/Dockerfile.lambda` (unduh biner statis, pilih arsitektur lewat `TARGETARCH` untuk `arm64` dan `amd64`); tanpa texlive/LaTeX
    - `ENV DOC_ENGINE=typst TYPST_BIN=/usr/local/bin/typst PANDOC_BIN=/usr/local/bin/pandoc PDF_MD_REQUIRE_TOOLS=1 HOME=/tmp XDG_DATA_HOME=/tmp XDG_CACHE_HOME=/tmp`
    - Kode aplikasi tidak disalin ke image (di-mount saat run); `WORKDIR /app`; `CMD ["cargo", "test", "--workspace"]`
    - _Requirements: 16.2, 16.12_

  - [x] 5.2 Buat `scripts/test-docker.sh`
    - `set -euo pipefail`; bangun image (`docker build -f deploy/Dockerfile.test -t disposable-test .`, konteks build dibatasi lewat `.dockerignore` agar `target/`, `node_modules/`, dan `.git/` tidak ikut), dengan `--platform "$DOCKER_PLATFORM"` hanya bila variabel itu diatur (bawaan: arsitektur asli)
    - Jalankan kontainer dengan repo di-bind-mount baca-tulis ke `/app`, volume bernama untuk registry cargo (`disposable-cargo-registry`) dan untuk `target/` (`disposable-cargo-target`, `CARGO_TARGET_DIR` mengarah ke volume itu karena `target/` hasil bind-mount sangat lambat di macOS)
    - Langkah pertama di kontainer: verifikasi `pdftohtml -v`, `pdfinfo -v`, `pandoc --version`, `typst --version` (`pdftohtml -v` dan `pdfinfo -v` menulis ke stderr dan boleh berkode keluar 0 atau 99); bila ada yang gagal, cetak pesan jelas yang menyebut alat yang hilang dan keluar non-nol
    - Jalankan `cargo test --workspace "$@"` sehingga argumen (mis. filter nama tes `pdf2md`) diteruskan; opsi `--shell` membuka `bash` interaktif (`-it`) di kontainer yang sama; kode keluar `cargo test` diteruskan apa adanya
    - Dokumentasikan penggunaan pada komentar kepala berkas dan `--help`; `chmod +x scripts/test-docker.sh`
    - _Requirements: 16.1, 16.12_

  - [x] 5.3 Dokumentasikan penggunaan di `README.md` dan `AGENTS.md`
    - Tambah bagian "Menjalankan tes yang membutuhkan poppler di Docker" pada keduanya: `scripts/test-docker.sh`, contoh dengan filter (`scripts/test-docker.sh pdf2md`), `--shell`, variabel `DOCKER_PLATFORM`, catatan bahwa build pertama memakan beberapa menit
    - Jelaskan bahwa `cargo test` lokal biasa tetap menjalankan semua tes yang tidak membutuhkan poppler dan melewati sisanya dengan pesan `SKIP`, dan bahwa kontainer mengatur `PDF_MD_REQUIRE_TOOLS=1` sehingga tes yang dilewati menjadi gagal
    - _Requirements: 15.5, 16.12_

  - [x] 5.4 Verifikasi tes bergantung poppler di Docker
    - Jalankan `scripts/test-docker.sh pdf2md` (build pertama memakan beberapa menit); konfirmasi dari keluaran bahwa tes yang membutuhkan poppler (unit Poppler_Engine task 4.11) dan round trip Markdown -> PDF -> Markdown (Property 33, task 4.13) benar-benar dieksekusi dan lulus, tidak dilewati (tidak ada baris `SKIP` dan `PDF_MD_REQUIRE_TOOLS=1` aktif)
    - Bila keluaran `pdftohtml` pada PDF hasil Typst menuntut penyesuaian heuristik (ukuran heading, deteksi tebal/monospace, jarak paragraf, indentasi kode, ligatur), terapkan penyesuaian pada tugas mesin/struktur (2.2, 2.5, 4.3) dan catat setiap penyesuaian beserta alasannya pada Notes; jalankan ulang hingga hijau
    - _Requirements: 6.7, 16.2, 16.12_

- [x] 6. Adaptor route HTTP dan integrasi `AppState`
  - [x] 6.1 Ubah `crates/server/src/state.rs`
    - Tambah `pdf: Arc<PdfRuntime>` (bawaan `PdfRuntime::defaults()` di `AppState::new()`), `with_pdf(rt)`, `take_ephemeral_download_if(token, pred)` (entri tidak dihapus bila `pred` false; kedaluwarsa dihapus dengan penimpaan nol), `purge_expired_downloads()` memakai `privacy::burn::wipe_buffer`; `store_ephemeral_download` memanggilnya sebagai ganti `retain`
    - Pastikan `wipe_downloads` pada Burn Workspace tetap mengosongkan entri Markdown
    - _Requirements: 10.5, 10.8, 10.9, 12.5_

  - [x] 6.2 Implementasikan `routes/pdf_to_markdown.rs`
    - `convert_pdf` (`AuthenticatedSession` sebagai argumen pertama; baca multipart dengan penghentian di `max_bytes + 1`, part `options` maks 4.096 byte, part `file` tambahan dikuras dan dihitung, 400 `file_required`, `download=true` hanya bila persis `true`), `download_markdown` (UUID tidak valid -> 404 seragam; header `text/markdown; charset=utf-8`, `Content-Disposition` `document.md`, `no-store`), `get_limits`
    - `finalize_pdf_response`: `Cache-Control: no-store` pada semua respons path `/api/apps/pdf-to-markdown*`, dan penyeragaman badan 401/503/413 ke `{success:false, code, message}`
    - `router(&AppState)` dengan `DefaultBodyLimit::max(max_bytes + 65_536)` hanya pada POST
    - _Requirements: 1.1, 1.7, 1.8, 2.1, 2.9, 2.11, 9.5, 9.6, 9.8, 10.1, 10.2, 10.3, 10.4, 10.5, 10.10, 10.11, 10.12, 12.7, 15.3_

  - [x] 6.3 Daftarkan rute dan sambungkan start-up
    - `routes/mod.rs`: `pub mod pdf_to_markdown;`, merge router di `create_api_router`, pasang `finalize_pdf_response` sebagai layer terluar; `routes/burn.rs`: tambah field `"code": "burn_in_progress"` pada JSON 503 `burn_gate`
    - `main.rs`: panggil `PdfRuntime::from_env()` sebelum `TcpListener::bind` (pada `Err` tulis ke stderr dan `exit(2)`), catat peringatan konfigurasi satu kali, jalankan sweeper `purge_expired_downloads` tiap 15 detik
    - _Requirements: 2.9, 2.10, 7.4, 9.3, 9.4, 15.3, 15.4_

  - [x] 6.4 Tulis property test penolakan non-PDF
    - **Property 4: Masukan non-PDF ditolak tanpa menjalankan mesin**
    - Berkas: `pdf2md/tests/prop_04.rs`
    - **Validates: Requirements 2.3, 16.5**

  - [x] 6.5 Tulis property test urutan prioritas galat
    - **Property 5: Urutan prioritas galat**
    - Berkas: `pdf2md/tests/prop_05.rs`
    - **Validates: Requirements 2.1, 2.2, 2.12, 9.6**

  - [x] 6.6 Tulis property test bentuk badan galat
    - **Property 6: Bentuk badan galat**
    - Berkas: `pdf2md/tests/prop_06.rs`
    - **Validates: Requirements 2.11, 7.7**

  - [x] 6.7 Tulis property test zero-footprint
    - **Property 7: Zero-footprint untuk setiap hasil**
    - Berkas: `pdf2md/tests/prop_07.rs` (suntik kegagalan I/O lewat `job_root` tak dapat ditulis, `MockOutcome::Sleep` untuk pembatalan future)
    - **Validates: Requirements 11.9, 12.1, 12.2, 12.3, 12.6, 16.7**

  - [x] 6.8 Tulis property test tidak ada data pengguna di log
    - **Property 8: Tidak ada data pengguna di log**
    - Berkas: `pdf2md/tests/prop_08.rs` (tambahkan `prop_08` ke `tests/mod.rs` bila belum ada)
    - **Validates: Requirements 8.8, 12.4, 16.8**

  - [x] 6.9 Tulis property test round trip Ephemeral_Download
    - **Property 27: Round trip Ephemeral_Download**
    - Berkas: `pdf2md/tests/prop_27.rs`
    - **Validates: Requirements 10.1, 10.4, 10.10, 10.11**

  - [x] 6.10 Tulis property test token tidak valid
    - **Property 28: Token tidak valid dijawab seragam**
    - Berkas: `pdf2md/tests/prop_28.rs`
    - **Validates: Requirements 10.5**

  - [x] 6.11 Tulis unit test route
    - Per rute (`/api/apps/pdf-to-markdown`, `/download/:token`, `/limits`): 401 tanpa sesi dan 503 selama burning, `MockEngine.calls == 0`, tanpa direktori sementara, entri unduhan tidak dikonsumsi
    - Batas persis dengan `max_bytes = 64` (64 diterima, 65 -> 413; body `64 + 65.536` diterima, lebih satu byte -> 413); direktori sementara bersih dalam 1 detik pada jalur 400, 401, 413, 415, 422, 503, 504; log tidak memuat penanda unik, nama file, atau teks hasil pada konversi sukses dan gagal; 405 untuk metode lain; part `file` hilang -> 400 `file_required`; beberapa part `file` -> satu peringatan; `/limits` -> 200 dengan `max_bytes` dan `max_pages = 300`; `download` bernilai `true` vs `false`/`1`/`TRUE`; `no-store` pada 200/401/404/503
    - Berkas: `pdf2md/tests/unit_routes.rs`
    - _Requirements: 1.7, 1.8, 2.9, 2.10, 9.5, 9.6, 9.8, 9.9, 10.2, 10.12, 12.4, 12.7, 15.3, 15.4, 16.1, 16.6, 16.7, 16.8, 16.9_

  - [x] 6.12 Tulis unit test Ephemeral_Download
    - Pemakaian kedua -> 404; kedaluwarsa (`expires_at` lampau) -> 404 dan buffer ditimpa nol oleh `purge_expired_downloads`; Burn Workspace (`run_or_join`) membuat token 404/503; token milik fitur lain tidak dihapus; tidak ada unduhan dibuat saat konversi gagal dengan `download=true`; 401 dan 503 mempertahankan entri
    - Berkas: `pdf2md/tests/unit_download.rs`
    - _Requirements: 10.3, 10.5, 10.6, 10.7, 10.8, 10.9, 16.1, 16.10_

  - [x] 6.13 Tulis tes integrasi proses anak start-up
    - `PDF_MD_ENGINE=tidak-ada` -> keluar non-nol, stderr memuat `PDF_MD_ENGINE` dan `poppler`, port tidak menerima koneksi; `PDF_MD_MAX_BYTES` bernilai `abc`, `0`, `-1`, `1.5` -> keluar non-nol dengan pesan memuat `PDF_MD_MAX_BYTES`; `99999999999` -> start sukses dan `GET /limits` memberi 26.214.400
    - Berkas: `crates/server/tests/pdf_to_markdown_startup.rs` (pola `burn_shutdown.rs`, `CARGO_BIN_EXE_*`)
    - _Requirements: 7.4, 9.3, 9.4_

- [x] 7. Integrasi MCP
  - [x] 7.1 Tambah tool `pdf_to_markdown` dan varian `RpcErrorData` di `routes/mcp.rs`
    - Tambahkan tool ke `tools/list` setelah `burn_workspace_now` dengan `inputSchema` (`file_base64` wajib; `page_from`, `page_to`, `page_breaks`, `strip_headers_footers` opsional); tool lain tidak diubah
    - Varian `ToolOutcome::RpcErrorData { code, message, data }` dan cabang respons yang menyertakan `error.data`; helper `opt_u32_min1` dan `opt_bool`
    - _Requirements: 11.1, 11.6_

  - [x] 7.2 Implementasikan `call_pdf_to_markdown`
    - Validasi argumen (nama tak dikenal, tipe, `page_from > page_to`, batas panjang base64 `ceil(max_bytes/3)*4`, hasil dekode `0 < len <= max_bytes`) -> `-32602` menyebut nama argumen tanpa efek samping; opsi diserialisasi lalu diurai ulang oleh `parse_options`; panggil `convert`
    - Galat dokumen/mesin -> `-32000` dengan `data.code` (termasuk `not_a_pdf`, `output_too_large`, `page_from_out_of_range`); `Internal` -> `-32603`; sukses lewat `shape_mcp_result` dengan Ephemeral_Download (TTL 300 detik, URL `/api/apps/pdf-to-markdown/download/<token>`) hanya bila terpotong; tanpa logging argumen
    - _Requirements: 11.2, 11.3, 11.4, 11.5, 11.6, 11.7, 11.8, 11.9_

  - [x] 7.3 Tulis property test paritas MCP dan REST
    - **Property 29: Paritas MCP dan REST**
    - Berkas: `pdf2md/tests/prop_29.rs`
    - **Validates: Requirements 11.2**

  - [x] 7.4 Tulis property test ambang pemotongan
    - **Property 30: Ambang pemotongan hasil MCP**
    - Berkas: `pdf2md/tests/prop_30.rs`
    - **Validates: Requirements 11.3, 11.4**

  - [x] 7.5 Tulis property test argumen MCP tidak valid
    - **Property 31: Argumen MCP tidak valid ditolak tanpa efek samping**
    - Berkas: `pdf2md/tests/prop_31.rs`
    - **Validates: Requirements 11.5**

  - [x] 7.6 Tulis unit test MCP
    - `tools/list` memuat tepat satu `pdf_to_markdown` dan enam tool lama tidak berubah; sukses (tanpa pemotongan, dan pemotongan dengan URL unduhan dapat diunduh); setiap galat dokumen -> `-32000` dengan `data.code`; 401 tanpa sesi; burning -> galat "sedang dihancurkan"; direktori sementara bersih
    - Berkas: `pdf2md/tests/unit_mcp.rs`
    - _Requirements: 11.1, 11.3, 11.4, 11.6, 11.7, 11.8, 11.9, 16.1_

- [x] 8. Checkpoint - Backend lengkap
  - Ensure all tests pass, ask the user if questions arise.

- [x] 9. Frontend
  - [x] 9.1 Implementasikan `lib/validatePdfFile.ts` dan `lib/parsePdfOptions.ts`
    - `validatePdfFile({name,size,type}, maxBytes)` -> `null | 'too_large' | 'not_pdf'` (`not_pdf` bila ekstensi bukan `.pdf` tanpa memandang huruf besar/kecil dan MIME bukan `application/pdf`)
    - `parsePdfOptions({from,to,pageBreaks,strip})` -> `{ok:true, options} | {ok:false, field, message}` (`/^[1-9][0-9]*$/`, kosong = bawaan, `from > to` -> galat pada `page_from`)
    - _Requirements: 13.4, 13.5, 13.6_

  - [x] 9.2 Implementasikan `lib/pdfErrorMessage.ts`, `lib/markdownPreview.ts`, dan `lib/downloadText.ts`
    - `pdfErrorMessage(status | 'network', code?)` dengan pesan berbeda untuk 401, 413, 415, 422 (per `code`, `no_text_layer` menyebut hasil pindai dan OCR belum didukung), 503, 504, dan jaringan
    - `parseMarkdownPreview(md): PreviewBlock[]` (heading, paragraf, daftar, kode, `hr`, inline tebal/miring, backslash-escape dikembalikan), tanpa HTML
    - `markdownToBlob(md)` dan `saveAs(blob, 'document.md')`
    - _Requirements: 13.11, 13.12, 13.13, 13.18_

  - [x] 9.3 Implementasikan `components/PdfToMarkdown.tsx`
    - Ambil `GET /api/apps/pdf-to-markdown/limits` saat mount (gagal -> pesan galat tanpa angka tebakan); `PageShell currentPath="/pdf-to-markdown"`; `pickFirstFile` dengan pemberitahuan bila > 1 file; nama dan ukuran file via `formatFileSize`; validasi file dan opsi tanpa permintaan bila gagal; `FormData` (`file` + `options`), `AbortController`, tombol nonaktif dan indikator saat berjalan
    - Hasil: `pages`, `headings`, daftar `warnings` (satu `<li>` per entri), mode pratinjau/mentah (awal pratinjau), salin (konfirmasi 2 detik, gagal -> pesan), unduh `document.md`; galat memakai `pdfErrorMessage`, mempertahankan file dan menghapus hasil lama, 401 -> `/login` dalam 1 detik; cleanup saat unmount; tanpa `localStorage`/`sessionStorage`/cookie; hanya token Tailwind dan `PRIMARY_BUTTON`, tanpa heksadesimal
    - Pratinjau dirender sebagai node teks React, tanpa `dangerouslySetInnerHTML`
    - _Requirements: 9.7, 13.1, 13.2, 13.3, 13.4, 13.5, 13.6, 13.7, 13.8, 13.9, 13.10, 13.11, 13.12, 13.13, 13.14, 13.15, 13.16, 13.17, 13.18_

  - [x] 9.4 Tambah halaman dan routing
    - Buat `frontend/src/pages/pdf-to-markdown.astro` (`<AppRouter client:only="react" page="pdf-to-markdown" />`); ubah `AppRouter.tsx` (`'pdf-to-markdown'` pada union `page`, `PAGE_PATH`, render `<PdfToMarkdown />`)
    - _Requirements: 13.1, 15.3_

  - [x] 9.5 Ubah `Navbar.tsx` dan `DashboardView.tsx`
    - `NAV_LINKS` menjadi `{href,label,short}` dengan enam tautan (termasuk `/pdf-to-markdown` berlabel "PDF to Markdown", short "PDF → MD"); `<span class="xl:hidden">{short}</span><span class="hidden xl:inline">{label}</span>`, `aria-label={label}`, teks merek disembunyikan pada `lg` sampai `xl`, jarak `gap-4` pada `lg` dan `xl:gap-6`; menu mobile memakai label penuh dan menutup saat tautan diklik; `aria-current="page"` hanya pada tautan aktif
    - `DashboardView`: tepat satu kartu "PDF to Markdown" dengan `href="/pdf-to-markdown"`
    - _Requirements: 14.1, 14.2, 14.3, 14.4, 14.5, 14.6, 14.7, 14.8_

  - [x] 9.6 Tulis property test validasi file di klien
    - **Property 34: Validasi file di klien**
    - Berkas: `frontend/src/lib/validatePdfFile.test.ts`
    - **Validates: Requirements 13.4**

  - [x] 9.7 Tulis property test validasi opsi di klien
    - **Property 35: Validasi opsi di klien**
    - Berkas: `frontend/src/lib/parsePdfOptions.test.ts`
    - **Validates: Requirements 13.5, 13.6**

  - [x] 9.8 Tulis property test keamanan pratinjau Markdown
    - **Property 36: Pratinjau Markdown tidak pernah menyuntikkan HTML**
    - Berkas: `frontend/src/lib/markdownPreview.test.ts` (string acak diperkaya fragmen HTML, render komponen pratinjau, periksa himpunan elemen, atribut `on*`, dan `textContent`)
    - **Validates: Requirements 13.18**

  - [x] 9.9 Tulis property test berkas unduhan
    - **Property 37: Berkas unduhan identik dengan hasil**
    - Berkas: `frontend/src/lib/downloadText.test.ts`
    - **Validates: Requirements 13.11**

  - [x] 9.10 Tulis property test tampilan warnings
    - **Property 38: Warnings ditampilkan sesuai urutan**
    - Berkas: `frontend/src/components/PdfToMarkdown.warnings.test.tsx`
    - **Validates: Requirements 13.9**

  - [x] 9.11 Tulis tes komponen PdfToMarkdown
    - Pengujian terpisah: hanya file pertama diterima, penolakan bukan PDF, penolakan melebihi batas tanpa `fetch`, batas dari `/limits` tampil dan galatnya tanpa angka tebakan, tombol nonaktif saat berjalan, tampilan hasil (`pages`, `headings`, `warnings`, dua mode), salin sukses/gagal, unduh `document.md`, pesan untuk 401/413/415/422/503/504/jaringan dan `no_text_layer`, arahan login pada 401, pengosongan state saat unmount, spy `localStorage`/`sessionStorage`/`document.cookie` tidak dipanggil
    - Berkas: `frontend/src/components/PdfToMarkdown.test.tsx`
    - _Requirements: 9.7, 13.3, 13.4, 13.7, 13.8, 13.9, 13.10, 13.11, 13.12, 13.13, 13.14, 13.15, 13.16, 16.13_

  - [x] 9.12 Tulis tes komponen Navbar
    - `BurnButton`, `ThemeToggle`, dan `ActiveSessionsModal` di-mock; tiga tes terpisah: tautan ke `/pdf-to-markdown` ada pada tampilan desktop dengan nama yang dapat diakses memuat "PDF to Markdown", `aria-current="page"` hanya saat `/pdf-to-markdown` aktif, tautan ada pada menu "Menu" mobile dan menutup menu saat diklik; `DashboardView` memuat tepat satu tautan
    - Berkas: `frontend/src/components/Navbar.test.tsx`
    - _Requirements: 14.1, 14.3, 14.5, 14.6, 14.7, 14.8, 16.14_

- [x] 10. Deployment, dokumentasi, dan smoke test kepatuhan
  - [x] 10.1 Tambah `poppler-utils` pada Dockerfile
    - `deploy/Dockerfile` dan `deploy/Dockerfile.lambda`: tambahkan `poppler-utils` pada `apt-get install` tahap runtime, `ENV PDF_MD_ENGINE=poppler`, dan `RUN pdftohtml -v || [ $? -eq 99 ]`; `Dockerfile.lambda` juga `ENV PDF_MD_MAX_BYTES=5242880` dan `ENV PDF_MD_MAX_RESPONSE_BYTES=5767168`
    - _Requirements: 15.1, 15.2, 9.2, 9.10_

  - [x] 10.2 Perbarui `deploy/docker-compose.yml`
    - Tambah `PDF_MD_ENGINE=${PDF_MD_ENGINE:-poppler}`, `PDF_MD_MAX_BYTES=${PDF_MD_MAX_BYTES:-}`, `PDF_MD_MAX_RESPONSE_BYTES=${PDF_MD_MAX_RESPONSE_BYTES:-}` pada `environment`; jangan mengubah perilaku `teardown` CLI
    - _Requirements: 9.1, 9.2, 15.6, 15.7_

  - [x] 10.3 Perbarui `README.md` dan `AGENTS.md`
    - Pada keduanya: deskripsi mini app, tiga rute API, tool MCP `pdf_to_markdown`, tiga variabel lingkungan pada tabel runtime (nilai bawaan dan deskripsi terisi), batas 25 MB / 5 MB (Lambda) / 300 halaman / 30 detik, cakupan Fase 1 (tanpa tabel, OCR, multi-kolom, ekstraksi gambar), dan catatan lisensi GPL poppler sebagai subprocess
    - _Requirements: 15.5, 15.6_

  - [x] 10.4 Tulis smoke test kepatuhan
    - Tiga rute baru tidak menghasilkan 404 pada `create_api_router` dan metode lain -> 405; `Cargo.toml` tidak memuat dependensi poppler maupun database; `structure/` tidak mengimpor `poppler`, `xml`, atau `proc`; kedua Dockerfile memuat `poppler-utils`; compose (`include_str!`) memuat tiga variabel baru; `README.md` dan `AGENTS.md` memuat semua butir 15.5 dan 15.6; tidak ada literal warna heksadesimal pada `PdfToMarkdown.tsx`; `crates/cli` (`teardown`) tidak mengimpor atau memanggil PDF_Converter maupun Burn_Service
    - Berkas: `pdf2md/tests/smoke.rs`
    - _Requirements: 7.5, 8.9, 12.5, 13.17, 15.1, 15.3, 15.4, 15.5, 15.6, 15.7_

- [x] 11. CI GitHub Actions
  - [x] 11.1 Perluas `scripts/test-docker.sh` dengan `TEST_CACHE_DIR`
    - Perubahan minimal: bila `TEST_CACHE_DIR` diatur (jalur absolut), buat `$TEST_CACHE_DIR/cargo-registry` dan `$TEST_CACHE_DIR/cargo-target` lalu mount keduanya sebagai bind mount ke `/usr/local/cargo/registry` dan ke lokasi `CARGO_TARGET_DIR` di kontainer, menggantikan volume bernama `disposable-cargo-registry` dan `disposable-cargo-target`; bila tidak diatur, perilaku lokal (volume bernama) tidak berubah
    - Alasan: `actions/cache` hanya dapat menyimpan dan memulihkan direktori host, bukan volume bernama Docker; dengan bind mount ke direktori host di runner, cache dapat dipulihkan sebelum dan disimpan sesudah `scripts/test-docker.sh`
    - Tambahkan `TEST_CACHE_DIR` ke komentar kepala berkas dan `--help`; pastikan kontainer dapat menulis ke direktori itu (jalankan sebagai UID host atau `chmod` direktori); verifikasi dengan `bash -n scripts/test-docker.sh` dan `scripts/test-docker.sh --help`
    - _Requirements: 16.1, 16.12_

  - [x] 11.2 Tulis `.github/workflows/ci.yml`
    - Nama `CI`; `on`: `workflow_dispatch` dan `workflow_call`. Trigger `pull_request` dan `push` (branch `main`) tidak dipasang di sini karena `publish-image.yml` sudah memicu pada kedua event itu (dan tag `v*.*.*`) lalu memanggil `ci.yml`; memasangnya di keduanya membuat CI berjalan dua kali pada event yang sama, dan `concurrency` tidak dapat membatalkan salah satunya karena `github.workflow` di workflow terpanggil bernilai nama workflow pemanggil, sehingga grup kedua run berbeda. Keputusan ini dicatat pada Notes
    - `permissions: contents: read`; `concurrency`: grup `${{ github.workflow }}-${{ github.ref }}`, `cancel-in-progress: ${{ github.ref != 'refs/heads/main' }}`
    - Job `backend` (`ubuntu-latest`, amd64 saja, `timeout-minutes` diatur, mis. 60): `actions/checkout@v4`; `docker/setup-buildx-action@v3`; `actions/cache@v4` untuk `${{ runner.temp }}/test-cache` (dipakai sebagai `TEST_CACHE_DIR`) dengan key `cargo-${{ runner.os }}-${{ hashFiles('Cargo.lock') }}` dan `restore-keys` `cargo-${{ runner.os }}-`; `docker/build-push-action@v6` untuk `deploy/Dockerfile.test` (`context: .`, `tags: disposable-test`, `load: true`, `push: false`, `cache-from: type=gha`, `cache-to: type=gha,mode=max`); langkah `run: scripts/test-docker.sh` dengan `env: TEST_CACHE_DIR`. Karena `PDF_MD_REQUIRE_TOOLS=1` diatur di image, tes poppler yang dilewati menjadi kegagalan. Pastikan skrip memakai image `disposable-test` yang sudah dimuat tanpa membangun ulang secara berlebihan (atau tambahkan opsi `SKIP_BUILD=1` pada skrip bila diperlukan)
    - Job `frontend` (`ubuntu-latest`, paralel dengan `backend`, `timeout-minutes` diatur): `actions/checkout@v4`; `actions/setup-node@v4` (Node 22, `cache: npm`, `cache-dependency-path: frontend/package-lock.json`); `defaults.run.working-directory: frontend`; langkah `npm ci`, `npx tsc --noEmit`, `npm test`, `npm run build`
    - Tanpa clippy dan tanpa `cargo fmt --check`
    - Ketahanan unduhan: ubah `deploy/Dockerfile.test` (berkas dari task 5.1; perubahan ini dikerjakan di grup 11 dan berada di wave setelah 5.1, sehingga tidak ada konflik penulisan berkas) agar setiap unduhan jaringan tahan gangguan sesaat: `curl --retry 5 --retry-delay 2 --retry-all-errors --fail --location` untuk biner Pandoc dan Typst, dan `-o Acquire::Retries=5` pada `apt-get update`/`apt-get install`
    - _Requirements: 16.1, 16.12, 16.13, 16.15_

  - [x] 11.3 Restrukturisasi `.github/workflows/publish-image.yml` (opsi B: gating CI)
    - Trigger tetap: push ke `main`, tag `v*.*.*`, `pull_request` ke `main`, dan `workflow_dispatch` dengan input `tag` yang sudah ada; tambahkan input boolean `skip_ci` (`type: boolean`, `default: false`, deskripsi menyebut bahwa ini hanya untuk keadaan darurat) pada `workflow_dispatch`
    - Job `ci`: `uses: ./.github/workflows/ci.yml` dengan `if: ${{ !(github.event_name == 'workflow_dispatch' && inputs.skip_ci) }}`
    - Job `build-check` (hanya `pull_request`: `if: github.event_name == 'pull_request'`), tanpa `needs` sehingga berjalan paralel dengan `ci`: checkout, QEMU, Buildx, lalu `docker/build-push-action@v6` dengan `context: .`, `file: ./deploy/Dockerfile`, `platforms: linux/amd64,linux/arm64`, `push: false`, `cache-from: type=gha`, `cache-to: type=gha,mode=max`; tanpa login GHCR, metadata, provenance, atau sbom. Duplikasi YAML kecil ini disengaja agar tidak memakai struktur bersama yang rumit
    - Job `build-and-push`: `needs: ci` dan `if: ${{ always() && github.event_name != 'pull_request' && (needs.ci.result == 'success' || (needs.ci.result == 'skipped' && github.event_name == 'workflow_dispatch' && inputs.skip_ci)) }}`; seluruh langkah yang ada (QEMU, Buildx, login GHCR, metadata, build-push `linux/amd64,linux/arm64` dengan provenance dan sbom, ringkasan digest) tidak diubah; tambahkan langkah (hanya bila `inputs.skip_ci` benar) yang menulis peringatan ke `$GITHUB_STEP_SUMMARY`, mis. `echo "⚠️ CI skipped via skip_ci" >> $GITHUB_STEP_SUMMARY`. Kondisi `if: github.event_name != 'pull_request'` pada langkah login GHCR dan ringkasan digest menjadi kode mati di job ini (job tidak pernah berjalan pada PR) sehingga dihapus; `push:` pada build-push menjadi `true`
    - Izin: `publish-image.yml` mendeklarasikan `contents: read` dan `packages: write`; `ci.yml` yang dipanggil hanya membutuhkan `contents: read` (subset izin pemanggil, sehingga panggilan valid) dan tidak membutuhkan `secrets`; catat hal ini sebagai komentar di berkas workflow
    - Hasil: pada `pull_request` hanya job `ci` dan `build-check` yang berjalan; pada push ke `main` atau tag, `ci` lalu `build-and-push`; pada `workflow_dispatch` dengan `skip_ci=true`, `ci` dilewati dan `build-and-push` tetap berjalan; pada `workflow_dispatch` biasa, `ci` lalu `build-and-push`. Pendekatan berbasis `workflow_run` tidak dipakai
    - _Requirements: 16.12, 16.15_

  - [x] 11.4 Validasi workflow secara lokal
    - Jalankan `python3 -c 'import yaml,sys; [yaml.safe_load(open(f)) for f in sys.argv[1:]]' .github/workflows/ci.yml .github/workflows/publish-image.yml`; bila Docker tersedia, jalankan juga `docker run --rm -v "$PWD":/repo -w /repo rhysd/actionlint:latest` dan perbaiki seluruh temuan; bila Docker atau `actionlint` tidak tersedia, lewati langkah itu dan catat di Notes
    - Jalankan `bash -n scripts/test-docker.sh`, dan bila Docker tersedia, `scripts/test-docker.sh pdf2md` tanpa dan dengan `TEST_CACHE_DIR=/tmp/disposable-test-cache` untuk memastikan kedua mode berjalan
    - Karena GitHub Actions tidak dapat dijalankan lokal, verifikasi dengan membaca YAML terhadap daftar periksa berikut:
      - `pull_request`: job yang aktif hanya `ci` dan `build-check` (`build-and-push` dilewati oleh `github.event_name != 'pull_request'`), tidak ada login GHCR atau push
      - push ke `main` atau tag `v*.*.*`: `ci` berjalan, `build-check` dilewati, lalu `build-and-push` berjalan setelah `ci` sukses
      - `workflow_dispatch` dengan `skip_ci=true`: `ci` dilewati, `build-and-push` tetap berjalan (hasil `skipped` diterima oleh kondisi `if`) dan menulis peringatan ke ringkasan
      - `workflow_dispatch` dengan `skip_ci=false`: `ci` berjalan dan `build-and-push` hanya berjalan bila `ci` sukses; bila `ci` gagal, `build-and-push` dilewati
      - `ci.yml` hanya memiliki trigger `workflow_dispatch` dan `workflow_call`, sehingga tidak ada run ganda
    - Tugas ini selesai tanpa menjalankan GitHub Actions
    - _Requirements: 16.12_

  - [x] 11.5 Dokumentasikan CI di `README.md` dan `AGENTS.md`
    - Bagian singkat "CI (GitHub Actions)" pada keduanya: dua job paralel di `ci.yml` (`backend` lewat `scripts/test-docker.sh`, `frontend` lewat `npm ci`, `npx tsc --noEmit`, `npm test`, `npm run build`), variabel `TEST_CACHE_DIR`, hanya amd64, tanpa clippy/`cargo fmt --check`, dan cara menjalankan padanan lokal
    - Jelaskan gating: `publish-image.yml` memanggil `ci.yml`; `build-and-push` (push `main`/tag/dispatch) menunggu `ci` sukses; pada PR berjalan `ci` dan `build-check` (build image tanpa push) secara paralel
    - Jelaskan input darurat `skip_ci` pada `workflow_dispatch`: hanya untuk maintainer dengan akses tulis, melewati CI, dan menghasilkan peringatan "CI skipped via skip_ci" pada ringkasan run
    - Rekomendasikan branch protection pada `main` dengan required status checks `ci / backend`, `ci / frontend`, dan `build-check`; ini pengaturan repositori manual yang tidak dapat dilakukan dari kode, tulis sebagai langkah manual untuk pemilik repositori (nama check sebaiknya dikonfirmasi pada run pertama)
    - Jelaskan cara menjalankan ulang hanya job yang gagal (`gh run rerun --failed` atau tombol "Re-run failed jobs" di UI), dan bahwa tes sensitif waktu atau tidak stabil dapat dilonggarkan lewat variabel lingkungan bila menunjukkan ketidakstabilan
    - Catat bahwa deploy Lambda (ECR) tidak digating oleh workflow ini
    - Catat bahwa eksekusi CI pertama baru terjadi setelah branch dipush ke GitHub, dan hal itu tidak dapat diverifikasi secara lokal selain lint YAML dan menjalankan `scripts/test-docker.sh`
    - _Requirements: 15.5, 16.12_

- [x] 12. Final checkpoint - Ensure all tests pass
  - Jalankan `cargo test --workspace`, lalu di `frontend/`: `npm test`, `npm run build`, dan `npx tsc --noEmit`; Ensure all tests pass, ask the user if questions arise.
  - _Requirements: 16.15_

## Notes

- Tugas bertanda `*` bersifat opsional (pengujian properti dan tes tambahan) dan dapat dilewati untuk MVP lebih cepat. Tes unit dan smoke yang diwajibkan Requirement 16 sengaja tidak diberi tanda `*`.
- Setiap properti 1 sampai 38 dari design memiliki tepat satu sub-tugas; Property 34 sampai 38 berada di sisi frontend.
- Pertanyaan Terbuka di design (definisi zona header/footer, penggabungan blok kode lintas halaman, jeda sweeper 15 detik, pengurai `PDF_MD_MAX_RESPONSE_BYTES`, batas konversi serentak) mengikuti keputusan yang tertulis di design kecuali pengguna mengubahnya.
- Tes yang membutuhkan poppler dijalankan di Docker lewat `scripts/test-docker.sh` (task 5); image hanya memakai Typst (tanpa texlive/LaTeX), dan `PDF_MD_REQUIRE_TOOLS=1` di kontainer mengubah skip menjadi kegagalan. Pipeline CI GitHub Actions ada pada task 11.
- CI (task 11, opsi B): `ci.yml` hanya memakai `workflow_dispatch` dan `workflow_call`; `publish-image.yml` memicu pada push/PR ke `main`, tag, dan dispatch, lalu memanggil `ci.yml` lewat job `ci`. Pada PR, job `build-check` (build tanpa push) berjalan paralel dengan `ci`; pada event non-PR, `build-and-push` memakai `needs: ci` dan tetap berjalan bila `ci` dilewati lewat input `skip_ci` pada dispatch. Trigger `pull_request`/`push` sengaja tidak diduplikasi di `ci.yml` karena `concurrency` tidak dapat membatalkan run ganda lintas workflow, dan pendekatan `workflow_run` tidak dipakai. Ketahanan unduhan (`curl --retry`, `Acquire::Retries`) pada `deploy/Dockerfile.test` dikerjakan di task 11.2 sebagai perubahan atas berkas task 5.1. Branch protection (required checks `ci / backend`, `ci / frontend`, `build-check`) adalah langkah manual pemilik repositori. Eksekusi pertama di GitHub baru terjadi setelah push dan tidak dapat diverifikasi lokal selain lint YAML dan `scripts/test-docker.sh`; tidak ada tugas yang mensyaratkan run GitHub.
- Lebar Navbar pada tepat 1024 px tidak dapat diverifikasi di jsdom; lakukan pemeriksaan visual manual di peramban.
- Tes Rust dijalankan dengan `cargo test --workspace` (tanpa poppler tes terkait dilewati; set `PDF_MD_REQUIRE_TOOLS=1`, otomatis di kontainer Docker); tes frontend dengan `npm test` di `frontend/`.

## Task Dependency Graph

```json
{
  "waves": [
    { "id": 0, "tasks": ["1.1"] },
    { "id": 1, "tasks": ["1.2", "1.3", "2.1", "9.1", "10.1", "10.2"] },
    { "id": 2, "tasks": ["1.4", "2.2", "2.3", "2.4", "2.5", "2.6", "2.8", "4.1", "4.2", "9.2", "9.6", "9.7", "5.1"] },
    { "id": 3, "tasks": ["2.7", "2.9", "2.10", "2.11", "2.12", "2.13", "4.3", "4.5", "4.10", "9.3", "9.8", "9.9", "5.2"] },
    { "id": 4, "tasks": ["2.14", "2.15", "2.16", "2.17", "2.18", "2.19", "2.20", "2.21", "2.22", "2.23", "2.24", "2.25", "2.26", "4.4", "4.6", "4.7", "4.8", "4.9", "4.11", "9.4", "9.5", "9.10", "9.11", "5.3", "11.1"] },
    { "id": 5, "tasks": ["4.12", "4.13", "6.1", "9.12", "10.3"] },
    { "id": 6, "tasks": ["6.2", "7.1", "5.4", "11.2"] },
    { "id": 7, "tasks": ["6.3", "7.2", "11.3"] },
    { "id": 8, "tasks": ["6.4", "6.5", "6.6", "6.7", "6.8", "6.9", "6.10", "6.11", "6.12", "6.13", "7.3", "7.4", "7.5", "7.6", "10.4", "11.4", "11.5"] }
  ]
}
```
