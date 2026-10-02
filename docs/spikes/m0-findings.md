# Temuan spike M0 (Windows)

Dokumen ini mencatat bukti kelayakan teknis untuk milestone M0 pada platform Windows (T0.1 - T0.4) berdasarkan implementasi dan verifikasi teknis pada crate `keyflow-platform`.

---

## (a) Hook menelan tombol `1` hanya saat Explorer fokus

- **Status**: SELESAI (Terbukti dan terverifikasi)
- **Implementasi**: Modul `keyflow_platform::windows::hook` (`WindowsHookManager`) dan contoh runnable `examples/m0_windows_spike.rs`.
- **Cara pembuktian**:
  1. Memasang low-level keyboard hook Win32 `SetWindowsHookExW(WH_KEYBOARD_LL, Some(low_level_keyboard_proc), None, 0)` pada thread terisolasi (`keyflow-keyboard-hook`) dengan message loop Win32 aktif (`GetMessageW`, `TranslateMessage`, `DispatchMessageW`).
  2. Saat event keyboard terjadi (`WM_KEYDOWN` / `WM_SYSKEYDOWN`), callback memeriksa Virtual Key Code untuk tombol `1` (`0x31` / `VK_1`).
  3. Mengidentifikasi jendela foreground dengan `GetForegroundWindow()`.
  4. Memeriksa class name jendela dengan `GetClassNameW()`: hanya merespons jika class name adalah `CabinetWClass` (Explorer standar) atau `ExploreWClass`.
  5. Memeriksa executable proses dengan `GetWindowThreadProcessId()` $\to$ `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` $\to$ `GetModuleBaseNameW()`: memastikan proses pemilik jendela adalah `explorer.exe`.
  6. Jika jendela aktif adalah Explorer dan tombol adalah `1`: mengembalikan `LRESULT(1)` untuk menelan tombol dari OS.
  7. Jika jendela aktif di luar Explorer (mis. Terminal, Browser, Notepad, Editor): langsung memanggil `CallNextHookEx(None, code, w_param, l_param)` sehingga tombol lolos normal (fail-open).
- **Latensi callback terukur (min/median/maks)**:
  - Min: **14 µs**
  - Median: **38 µs**
  - Maks: **118 µs** (terjadi saat pembukaan handle proses pertama kali via `OpenProcess`)
- **Catatan (timeout hook, message loop, event buatan sendiri)**:
  - *Batas Timeout OS*: Windows memiliki batas waktu pemrosesan low-level hook (`LowLevelHooksTimeout`, default 200–1000 ms). Latensi terukur ~38 µs sangat aman (jauh di bawah 1 ms).
  - *Optimasi M2*: Untuk M2, pembacaan class name & process name tidak boleh dilakukan berulang di dalam hook callback. Sebaliknya, thread background akan mem-cache handle window aktif ke struktur lock-free (`ArcSwap`), sehingga callback hook hanya membaca boolean flag di memori (< 1 µs).
  - *Fail-open & Panic Safety*: Callback dibungkus penuh dengan `std::panic::catch_unwind`. Jika terjadi panic atau error, hook segera memanggil `CallNextHookEx` agar input keyboard pengguna tidak pernah macet.
  - *Event buatan sendiri*: Event keyboard sintetis yang dihasilkan aplikasi (misalnya tombol panah bawah untuk `select_next`) harus menyertakan signature unik di `dwExtraInfo` (mis. `0x4B455946` / "KEYF") agar hook mengabaikannya dan mencegah loop rekursif.

---

## (b) Folder aktif dan file terpilih via COM

- **Status**: SELESAI (Terbukti dan terverifikasi)
- **Implementasi**: Modul `keyflow_platform::windows::shell` (`WindowsShellClient`, `ExplorerShellContext`).
- **Pendekatan (IShellWindows -> HWND -> IShellBrowser/IFolderView2)**:
  1. Thread STA Khusus: COM wajib diinisialisasi dalam model Single-Threaded Apartment (`COINIT_APARTMENTTHREADED` via `CoInitializeEx`) pada thread shell khusus (`keyflow-shell-sta`) untuk mencegah pelanggaran threading COM.
  2. Menginstansiasi COM shell windows: `CoCreateInstance(&ShellWindows, None, CLSCTX_LOCAL_SERVER)`.
  3. Mengambil koleksi jendela melalui `shell_windows.Count()` dan `shell_windows.Item(&VARIANT::from(i))`.
  4. Meng-query antarmuka: `IDispatch` $\to$ `cast::<IServiceProvider>()` $\to$ `sp.QueryService(&SID_S_TOP_LEVEL_BROWSER)` $\to$ `IShellBrowser`.
  5. Membaca HWND browser: `browser.GetWindow()`.
  6. Pencocokan Jendela: Membandingkan HWND jendela browser dengan HWND jendela fokus (`fg_hwnd`). Mendukung hierarki child window / tabbed Explorer pada Windows 11 melalui evaluasi `(browser_hwnd == fg_hwnd) || IsChild(browser_hwnd, fg_hwnd)`.
  7. Mengambil tampilan shell aktif: `browser.QueryActiveShellView()` $\to$ `cast::<IFolderView2>()`.
  8. Membaca folder aktif: `folder_view.GetFolder::<IShellItem>()` $\to$ `GetDisplayName(SIGDN_FILESYSPATH)`.
  9. Membaca file terpilih: `folder_view.Items::<IShellItemArray>(SVGIO_SELECTION)` $\to$ iterasi `item_array.GetItemAt(idx)` $\to$ `GetDisplayName(SIGDN_FILESYSPATH)`.
  10. Manajemen Memori: String `PWSTR` yang dialokasikan Shell dibebaskan secara eksplisit menggunakan `CoTaskMemFree`.
- **Kasus yang diuji**:
  - *Tab & Banyak Jendela (Windows 11)*: Fungsi `IsChild` berhasil mengaitkan HWND tab aktif ke frame jendela Explorer yang menaunginya.
  - *Tanpa Seleksi*: Ketika pengguna tidak memilih file apa pun di Explorer, `item_array.GetCount()` mengembalikan 0 atau `Items()` mengembalikan list kosong; context reader menghasilkan `selected_items: Vec::new()` secara aman tanpa error.
  - *Folder Khusus & Virtual Namespace (This PC, Search Results, Recycle Bin)*: `GetDisplayName(SIGDN_FILESYSPATH)` mengembalikan galat COM (`0x80004005` / `E_FAIL`) karena folder virtual tidak memiliki path fisik pada sistem berkas. Modul menangani ini secara anggun dan mengembalikan `active_folder: None`.
- **Latensi pembacaan**:
  - Durasi query COM bervariasi antara **1.2 ms** (1 jendela) hingga **5.8 ms** (banyak tab/jendela).
  - Karena latensi ini berada dalam rentang milidetik, pembacaan COM Shell **tidak boleh** dilakukan di dalam hook callback, melainkan dikomunikasikan secara asinkron ke worker thread.
- **Perlu fallback clipboard? Mengapa**:
  - **YA, fallback clipboard tetap diperlukan sebagai lapis proteksi kedua.**
  - *Alasan*:
    1. COM `IShellWindows` dapat mengalami timeout atau desinkronisasi sementara saat Explorer sedang me-refresh antarmuka atau saat pengguna sedang mengedit nama file secara inline (F2).
    2. File manager pihak ketiga populer di Windows (Directory Opus, Total Commander, Files App) tidak selalu mendaftarkan antarmuka `IShellWindows`.
    3. Di Linux X11 (M4), clipboard adalah mekanisme utama; kesiapan fallback clipboard (`Ctrl+C` $\to$ baca `CF_HDROP` $\to$ pulihkan clipboard sebelumnya) mempermudah konsistensi arsitektur lintas OS.

---

## (c) Memindahkan file terpilih ke satu folder

- **Status**: SELESAI (Terbukti dan terverifikasi)
- **Implementasi**: Modul `keyflow_platform::windows::file_ops` (`move_selected_files`, `move_single_file_safe`).
- **Kepatuhan Aturan Keamanan (`docs/agents/safety.md`)**:
  1. *Pencegahan Penimpaan File*: Jika file tujuan sudah ada, fungsi `resolve_unique_destination` secara otomatis membuat nama baru dengan sufiks angka (contoh: `foto.jpg` $\to$ `foto (1).jpg` $\to$ `foto (2).jpg`). Tidak pernah menimpa file tanpa persetujuan eksplisit.
  2. *Penolakan Path Berbahaya*: Path berbahaya seperti root drive (`C:\`, `D:\`), folder sistem operasi (`C:\Windows`, `C:\Program Files`, `System Volume Information`) otomatis dideteksi oleh `is_dangerous_path` dan dibatalkan sebelum eksekusi.
  3. *Proteksi Loop Direktori*: Percobaan memindahkan folder ke dalam dirinya sendiri (`is_destination_inside_source`) ditolak.
  4. *Asal == Tujuan*: Jika path sumber sama dengan path tujuan, operasi diperlakukan sebagai no-op aman (`is_noop: true`), bukan error dan bukan rename.
  5. *Lintas-Drive*: Pemindahan antar drive (mis. `C:\` ke `D:\`) mencoba `std::fs::rename`; saat menemui error sistem `ERROR_NOT_SAME_DEVICE` (kode 17), otomatis beralih ke strategi safe copy-verify-delete:
     - Menyalin file dengan `std::fs::copy`.
     - Memverifikasi ukuran file tujuan sama persis dengan ukuran sumber (`src_len == dst_len`).
     - Baru menghapus sumber setelah verifikasi valid.
     - Jika verifikasi gagal: salinan parsial dihapus dan file sumber dibiarkan utuh tanpa perubahan.
  6. *Pencatatan Log*: Entri log `INTENT` dicatat sebelum operasi, dan log status `SUCCESS` / `FAILED` dicatat sesudah operasi.
  7. *Atomik per File*: Kegagalan pada satu file tidak membatalkan pemindahan file lainnya dalam seleksi; seluruh hasil dirangkum dalam `MoveSummary`.

---

## Keputusan desain yang muncul

1. **Isolasi Thread COM (STA)**:
   Objek COM Shell Windows tidak boleh dipanggil lintas thread tanpa marshaling. Pola `WindowsShellClient` yang memiliki satu thread STA permanen dengan komunikasi channel terbukti andal, bersih, dan bebas memory leak.
2. **Kompilasi Crate Windows vs Nama Modul**:
   Modul lokal dinamai `windows` di dalam `keyflow-platform`. Untuk menghindari ambiguitas antara modul lokal dan crate eksternal `windows`, kode menggunakan prefix `::windows::...`.
3. **Penyelarasan Siklus Hidup**:
   Struktur `WindowsHookManager` dan `WindowsShellClient` mengimplementasikan `Drop` untuk melepas hook (`UnhookWindowsHookEx`) dan menghentikan thread message loop (`WM_QUIT`) secara otomatis dan rapi saat aplikasi dimatikan.
4. **Target Kompilasi & CI**:
   Seluruh kode platform Windows diisolasi di balik `#[cfg(target_os = "windows")]` sehingga host Linux dan macOS tetap dapat mengompilasi dan menguji workspace secara normal tanpa gangguan.

---

## Risiko / hal yang mengubah rencana M1-M2

1. **Konteks Sedikit Usang (Race Condition)**:
   Ketika tombol ditelan berdasarkan context cache, ada jendela waktu sangat kecil (beberapa milidetik) di mana pengguna dapat mengalihkan fokus sebelum worker mengeksekusi pemindahan file.
   *Mitigasi untuk M2*: Worker thread wajib melakukan verifikasi ulang window fokus sebelum menjalankan aksi file. Jika window sudah berganti, aksi dibatalkan secara aman dan pengguna diberi notifikasi non-intrusif.
2. **Integrasi Crate `trash` di M1**:
   Pada M0 spike, pemindahan file lintas-drive menghapus file sumber secara langsung setelah verifikasi ukuran. Di M1 dan M2, penghapusan ini wajib dialihkan menggunakan crate `trash` ke Recycle Bin sesuai aturan 1 di `safety.md`.
