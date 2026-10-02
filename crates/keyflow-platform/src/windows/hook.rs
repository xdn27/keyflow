//! Hook keyboard low-level (`WH_KEYBOARD_LL`) Windows.
//!
//! Mengimplementasikan T0.1: menelan tombol '1' hanya saat Windows Explorer
//! (`explorer.exe`, class `CabinetWClass`) sedang aktif/fokus.
//!
//! Memenuhi aturan `AGENTS.md` & `docs/agents/architecture.md`:
//! - Cepat dan sinkron (milidetik).
//! - Tidak ada COM atau I/O berat di dalam hook callback.
//! - Fail-open: jika ragu atau terjadi error/panic, selalu teruskan tombol.
//! - Setiap blok `unsafe` diberi komentar `// SAFETY:`.

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use ::windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use ::windows::Win32::System::ProcessStatus::GetModuleBaseNameW;
use ::windows::Win32::System::Threading::{
    GetCurrentThreadId, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};
use ::windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetClassNameW, GetForegroundWindow, GetMessageW,
    GetWindowThreadProcessId, PostThreadMessageW, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_QUIT,
    WM_SYSKEYDOWN,
};

/// Statistik latensi callback hook (dalam mikrodetik).
#[derive(Debug, Default, Clone)]
pub struct HookLatencyStats {
    pub count: u64,
    pub min_us: u64,
    pub max_us: u64,
    pub total_us: u64,
    pub last_us: u64,
}

impl HookLatencyStats {
    pub fn average_us(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.total_us as f64 / self.count as f64
        }
    }
}

// State statis untuk callback hook Win32
static HOOK_ACTIVE: AtomicBool = AtomicBool::new(false);
static HOOK_THREAD_ID: AtomicU32 = AtomicU32::new(0);

// Metrik latensi thread-safe
static LATENCY_COUNT: AtomicU64 = AtomicU64::new(0);
static LATENCY_MIN_US: AtomicU64 = AtomicU64::new(u64::MAX);
static LATENCY_MAX_US: AtomicU64 = AtomicU64::new(0);
static LATENCY_TOTAL_US: AtomicU64 = AtomicU64::new(0);
static LATENCY_LAST_US: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static HOOK_HANDLE: Cell<Option<HHOOK>> = const { Cell::new(None) };
}

/// Memeriksa apakah jendela target adalah Windows Explorer (`CabinetWClass` dan `explorer.exe`).
pub fn is_explorer_window(hwnd: HWND) -> bool {
    if hwnd.0.is_null() {
        return false;
    }

    // 1. Cek class name jendela
    let mut class_buf = [0u16; 256];
    // SAFETY: GetClassNameW dipanggil dengan buffer valid berukuran 256 elemen.
    let len = unsafe { GetClassNameW(hwnd, &mut class_buf) };
    if len <= 0 {
        return false;
    }
    let class_name = String::from_utf16_lossy(&class_buf[..len as usize]);
    // Windows Explorer folder view menggunakan 'CabinetWClass' (atau 'ExploreWClass' pada varian lama)
    if class_name != "CabinetWClass" && class_name != "ExploreWClass" {
        return false;
    }

    // 2. Cek nama executable proses
    let mut pid = 0u32;
    // SAFETY: GetWindowThreadProcessId dipanggil dengan pointer valid ke `pid`.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        return false;
    }

    // SAFETY: OpenProcess dengan hak akses minimal `PROCESS_QUERY_LIMITED_INFORMATION`.
    let process_handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) };
    let Ok(handle) = process_handle else {
        return false;
    };

    let mut name_buf = [0u16; 256];
    // SAFETY: GetModuleBaseNameW dipanggil dengan handle proses yang valid dan buffer 256 elemen.
    let name_len = unsafe { GetModuleBaseNameW(handle, None, &mut name_buf) };

    // SAFETY: Handle proses ditutup setelah selesai dicek.
    unsafe {
        let _ = ::windows::Win32::Foundation::CloseHandle(handle);
    }

    if name_len == 0 {
        return false;
    }

    let proc_name = String::from_utf16_lossy(&name_buf[..name_len as usize]).to_lowercase();
    proc_name == "explorer.exe"
}

/// Catat latensi eksekusi callback hook.
fn record_latency(us: u64) {
    LATENCY_COUNT.fetch_add(1, Ordering::Relaxed);
    LATENCY_TOTAL_US.fetch_add(us, Ordering::Relaxed);
    LATENCY_LAST_US.store(us, Ordering::Relaxed);

    // Update min
    let mut current_min = LATENCY_MIN_US.load(Ordering::Relaxed);
    while us < current_min {
        match LATENCY_MIN_US.compare_exchange_weak(
            current_min,
            us,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(actual) => current_min = actual,
        }
    }

    // Update max
    let mut current_max = LATENCY_MAX_US.load(Ordering::Relaxed);
    while us > current_max {
        match LATENCY_MAX_US.compare_exchange_weak(
            current_max,
            us,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(actual) => current_max = actual,
        }
    }
}

/// Ambil snapshot statistik latensi callback hook.
pub fn get_latency_stats() -> HookLatencyStats {
    let count = LATENCY_COUNT.load(Ordering::Relaxed);
    let min = LATENCY_MIN_US.load(Ordering::Relaxed);
    HookLatencyStats {
        count,
        min_us: if count > 0 && min != u64::MAX { min } else { 0 },
        max_us: LATENCY_MAX_US.load(Ordering::Relaxed),
        total_us: LATENCY_TOTAL_US.load(Ordering::Relaxed),
        last_us: LATENCY_LAST_US.load(Ordering::Relaxed),
    }
}

/// Callback low-level keyboard hook Win32 (`WH_KEYBOARD_LL`).
///
/// PERINGATAN: Harus sangat cepat (sub-milidetik), sinkron, tanpa alokasi besar,
/// tanpa COM, tanpa I/O. Selalu dibungkus `catch_unwind` agar tidak pernah panic (fail-open).
unsafe extern "system" fn low_level_keyboard_proc(
    code: i32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    let start_time = Instant::now();

    let result = std::panic::catch_unwind(|| {
        if code < 0 {
            // SAFETY: Sesuai dokumentasi Win32, jika code < 0, wajib teruskan ke CallNextHookEx.
            return unsafe { CallNextHookEx(None, code, w_param, l_param) };
        }

        let msg = w_param.0 as u32;
        // Hanya cek saat tombol ditekan (down)
        if msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN {
            // SAFETY: l_param pada WH_KEYBOARD_LL menunjuk ke struktur KBDLLHOOKSTRUCT.
            let kbd = unsafe { *(l_param.0 as *const KBDLLHOOKSTRUCT) };

            // Tombol '1' memiliki Virtual Key code 0x31 (VK_1)
            const VK_1: u32 = 0x31;
            if kbd.vkCode == VK_1 {
                // SAFETY: GetForegroundWindow aman dipanggil untuk memeriksa jendela aktif saat ini.
                let fg_hwnd = unsafe { GetForegroundWindow() };
                if is_explorer_window(fg_hwnd) {
                    let elapsed = start_time.elapsed().as_micros() as u64;
                    record_latency(elapsed);
                    tracing::debug!(latency_us = elapsed, "Tombol '1' ditelan di Explorer");
                    // Mengembalikan nilai non-nol (1) untuk menelan tombol
                    return LRESULT(1);
                }
            }
        }

        // Default: teruskan tombol (fail-open)
        // SAFETY: Meneruskan pesan keyboard ke hook berikutnya.
        unsafe { CallNextHookEx(None, code, w_param, l_param) }
    });

    match result {
        Ok(lresult) => lresult,
        Err(_) => {
            // Jika terjadi panic mendadak di dalam callback, fail-open!
            // SAFETY: Selalu teruskan tombol bila terjadi kegagalan tak terduga.
            unsafe { CallNextHookEx(None, code, w_param, l_param) }
        }
    }
}

/// Pengelola siklus hidup keyboard hook Windows.
pub struct WindowsHookManager {
    is_running: Arc<AtomicBool>,
    thread_handle: Option<std::thread::JoinHandle<()>>,
}

impl WindowsHookManager {
    pub fn new() -> Self {
        Self {
            is_running: Arc::new(AtomicBool::new(false)),
            thread_handle: None,
        }
    }

    /// Menjalankan keyboard hook pada thread khusus dengan message loop Win32.
    pub fn start(&mut self) -> Result<(), crate::PlatformError> {
        if self.is_running.load(Ordering::SeqCst) {
            return Ok(());
        }

        let is_running = self.is_running.clone();
        let (init_tx, init_rx) = std::sync::mpsc::channel();

        let handle = std::thread::Builder::new()
            .name("keyflow-keyboard-hook".into())
            .spawn(move || {
                // SAFETY: Mengambil thread ID untuk komunikasi pembatalan / penutupan loop.
                let thread_id = unsafe { GetCurrentThreadId() };
                HOOK_THREAD_ID.store(thread_id, Ordering::SeqCst);

                // Pasang hook WH_KEYBOARD_LL
                // SAFETY: SetWindowsHookExW dipanggil dengan fungsi hook proc yang valid.
                let hook = unsafe {
                    SetWindowsHookExW(WH_KEYBOARD_LL, Some(low_level_keyboard_proc), None, 0)
                };

                let hook_res = match hook {
                    Ok(h) => {
                        HOOK_HANDLE.with(|cell| cell.set(Some(h)));
                        HOOK_ACTIVE.store(true, Ordering::SeqCst);
                        is_running.store(true, Ordering::SeqCst);
                        let _ = init_tx.send(Ok(()));
                        h
                    }
                    Err(e) => {
                        let _ = init_tx.send(Err(crate::PlatformError::Os(format!(
                            "Gagal memasang SetWindowsHookExW: {e}"
                        ))));
                        return;
                    }
                };

                // Message loop Win32 wajib untuk WH_KEYBOARD_LL
                let mut msg = MSG::default();
                // SAFETY: GetMessageW memblokir hingga ada pesan window/thread.
                while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
                    if msg.message == WM_QUIT {
                        break;
                    }
                    // SAFETY: Pemrosesan pesan standar Win32.
                    unsafe {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }

                // Bersihkan hook sebelum thread keluar
                // SAFETY: UnhookWindowsHookEx melepas hook saat thread selesai.
                unsafe {
                    let _ = UnhookWindowsHookEx(hook_res);
                }
                HOOK_ACTIVE.store(false, Ordering::SeqCst);
                is_running.store(false, Ordering::SeqCst);
            })
            .map_err(|e| crate::PlatformError::Os(format!("Gagal membuat thread hook: {e}")))?;

        self.thread_handle = Some(handle);

        init_rx
            .recv()
            .map_err(|e| crate::PlatformError::Os(format!("Gagal inisialisasi hook thread: {e}")))?
    }

    /// Menghentikan keyboard hook dan keluar dari message loop.
    pub fn stop(&mut self) {
        if !self.is_running.load(Ordering::SeqCst) {
            return;
        }

        let tid = HOOK_THREAD_ID.load(Ordering::SeqCst);
        if tid != 0 {
            // SAFETY: Mengirim pesan WM_QUIT ke thread hook agar keluar dari GetMessageW.
            unsafe {
                let _ = PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }

        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }

        self.is_running.store(false, Ordering::SeqCst);
    }
}

impl Drop for WindowsHookManager {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Default for WindowsHookManager {
    fn default() -> Self {
        Self::new()
    }
}
