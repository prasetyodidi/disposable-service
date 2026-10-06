# Product Requirements Document (PRD): Disposable Service Workspace

**Document Version:** 1.0 (MVP)
**Status:** Approved for Development
**Target Audience:** Engineering Team (Backend & Infrastructure)

---

## 1. Executive Summary

**Disposable Service Workspace** adalah utilitas *on-demand* berfokus pada privasi yang memungkinkan *developer* untuk secara instan mendirikan (provision), menggunakan, dan menghancurkan (teardown) layanan internal yang terisolasi.

Tujuan utama sistem ini adalah menyediakan *ephemeral environment* (lingkungan sementara) untuk memproses tugas-tugas sensitif—dimulai dengan konversi Markdown ke PDF (menggunakan Pandoc)—tanpa meninggalkan jejak data di server pihak ketiga atau mengharuskan *developer* melakukan konfigurasi infrastruktur manual yang memakan waktu.

## 2. Core Value Proposition

* **Zero-Footprint:** Data diproses di lingkungan terisolasi dan tidak disimpan secara permanen.
* **Frictionless Provisioning:** Otomatisasi dari pembuatan instans hingga *deployment* aplikasi hanya dengan satu perintah CLI.
* **High Flexibility:** Mendukung *deployment* ke instans baru (via Cloud API) maupun instans yang sudah ada (via SSH).
* **Security by Default (MVP Level):** Otentikasi *Single Password* dengan sesi kedaluwarsa otomatis.

## 3. User Persona

* **Primary User:** Backend/Infrastructure/DevOps Engineers, atau tim internal teknis yang membutuhkan utilitas *on-the-fly* untuk pemrosesan data (seperti konversi dokumen) tanpa risiko kebocoran data (Data Leakage).

## 4. Minimum Viable Product (MVP) Scope (MUST HAVE)

MVP berfokus pada penciptaan *pipeline* dasar dari CLI hingga akses web, dengan satu aplikasi *use-case* spesifik.

### 4.1 CLI Automation & Provisioning (The Control Plane)

Modul CLI (Command Line Interface) bertindak sebagai titik masuk (*entry point*) utama.

* **Req 4.1.1: Dual-Target Routing.** CLI harus mendukung dua mode eksekusi:
* **New Instance Mode (`--new`):**
* Berinteraksi dengan Cloud API (misal: DigitalOcean Droplet / AWS EC2).
* Membuat instans baru dengan OS Linux (Ubuntu/Debian).
* Membuka *firewall* untuk Port 22 (SSH) dan Port 80 (HTTP).
* Menunggu hingga Public IP tersedia dan *SSH daemon* siap (Wait for SSH).


* **Existing Instance Mode (`--existing`):**
* Menerima argumen berupa IP, Port, Username, dan path ke SSH Private Key.
* Memvalidasi koneksi SSH.




* **Req 4.1.2: Deployment Pipeline.**
* Melakukan transfer (via SCP/SFTP) file `docker-compose.yml` ke target server.
* Mengeksekusi perintah `docker compose up -d` via SSH.
* Menampilkan URL akses (`http://<IP_ADDRESS>:80`) di terminal setelah proses selesai.



### 4.2 Web App & Authentication (The Core Service)

Aplikasi *web* yang berjalan di dalam instans dan diekspos melalui Port 80.

* **Req 4.2.1: First Access Setup (Claim on First Access).**
* Saat URL root (`/`) pertama kali diakses, sistem mendeteksi ketiadaan kredensial aktif (atau *password* belum diatur).
* Menampilkan form pendaftaran untuk membuat "Single Password".
* *Password* ini akan digunakan sebagai satu-satunya *key* untuk masuk ke dalam sistem selama instans tersebut hidup.


* **Req 4.2.2: Session Management.**
* Menggunakan JSON Web Token (JWT).
* *Expiration time:* 1 jam (Hardcoded untuk MVP).
* Jika *token* kedaluwarsa, API harus merespons dengan `401 Unauthorized` dan UI mengarahkan pengguna ke halaman *Login*.


* **Req 4.2.3: Dashboard & Monitoring.**
* **App Directory:** Daftar layanan yang tersedia (MVP: Pandoc Converter).
* **Active Sessions View:** Halaman admin sederhana yang menampilkan daftar sesi aktif berdasarkan IP Address dan detail *Device/User-Agent*. (Tidak ada *username* karena menggunakan pendekatan *single password*).



### 4.3 App 1: Markdown to PDF Converter (Pandoc)

Aplikasi pertama yang dijalankan di dalam lingkungan *disposable*.

* **Req 4.3.1: User Interface.**
* Tampilan form minimalis.
* Mendukung *paste* teks Markdown mentah (via `<textarea>`) atau *upload* file `.md`.


* **Req 4.3.2: Backend Processor.**
* Harus berjalan dalam *container* yang memiliki instalasi Pandoc dan LaTeX (`pdflatex`).
* Mengeksekusi perintah berikut saat menerima *request*:
```bash
pandoc 'input_file_name.md' -o 'output_file_name.pdf' \
  --pdf-engine=pdflatex \
  -V papersize=a4 \
  -V geometry:"top=1.8cm, bottom=1.8cm, left=2cm, right=2cm" \
  -V mainfont="Helvetica Neue" \
  -V fontsize=10.5pt \
  -V lineheight=1.25 \
  -V colorlinks=true \
  -V linkcolor=navy

```




* **Req 4.3.3: Output Delivery.**
* Mengembalikan *stream* file PDF ke *browser* untuk diunduh langsung (`Content-Disposition: attachment`).
* Data *input* dan *output* (file `.md` dan `.pdf`) harus segera dihapus (*cleaned up*) dari sistem file *container* setelah transmisi selesai.



### 4.4 Lifecycle Management (Teardown)

* **Req 4.4.1: Manual Teardown.**
* Untuk MVP, penghancuran instans atau pembersihan kontainer diserahkan kepada *developer* secara manual (misal: menghapus VM via Cloud Dashboard, atau menjalankan `docker compose down` via koneksi SSH).



---

## 5. Technical Architecture (MVP)

* **CLI Language:** Go atau Rust (sesuai keahlian Anda, disarankan Go untuk interaksi API Cloud/SSH yang ekstensif dan distribusi *binary* tunggal).
* **Deployment Configuration:** Docker Compose (`docker-compose.yml`).
* **Backend Application:** Go (Direkomendasikan untuk efisiensi *footprint* memori) atau Rust.
* **Frontend Application:** HTML/CSS/JS (Vanilla atau Lightweight framework seperti Alpine.js/HTMX) disajikan langsung oleh Backend.
* **Security:** HTTP (Port 80) tanpa TLS (Batasan MVP). Auth via JWT di Cookie (HttpOnly).

---

## 6. Future Enhancements (Post-MVP)

Fitur-fitur ini **tidak** termasuk dalam *scope* pengerjaan saat ini (MVP), tetapi didokumentasikan untuk arah arsitektur ke depan:

1. **Automated Teardown (TTL):** Mekanisme *cron* bawaan yang melakukan *shutdown/destroy* server setelah durasi tertentu (misal: 2 jam) atau *idle* selama 30 menit.
2. **HTTPS/TLS Integration:** Menggunakan *reverse proxy* (Caddy/Traefik) di *docker-compose* untuk *auto-provisioning self-signed* atau Let's Encrypt sertifikat guna melindungi proses otentikasi.
3. **Resource Throttling:** Menerapkan limitasi CPU dan Memory (`cpus`, `mem_limit`) di level `docker-compose` untuk melindungi instans dari eksploitasi beban berat (OOM killer).
4. **Auto-Generated Password:** Mengalihkan pembuatan *password* dari web menjadi otomatis di-*generate* oleh CLI dan di-*inject* via `.env` saat *deployment*.

---
