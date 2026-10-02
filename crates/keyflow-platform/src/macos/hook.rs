//! Implementasi `KeyboardHook` untuk macOS berbasis `CGEventTap` (T3.1).
//!
//! Memenuhi spesifikasi:
//! - Menggunakan `CGEventTapCreate` pada session level dengan hak intercept.
//! - Penanganan `kCGEventTapDisabledByTimeout` / `kCGEventTapDisabledByUserInput` dengan re-enable otomatis.
//! - Mengabaikan input sintetis buatan KeyFlow (`KEYFLOW_MACOS_USER_DATA`).
//! - Cepat dan sinkron (milidetik), fail-open (dibungkus `catch_unwind`).
//! - Dijalankan pada thread background dengan CFRunLoop.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use core_foundation::base::TCFType;
use core_foundation::string::CFString;

pub const KEYFLOW_MACOS_USER_DATA: i64 = 0x4B455946; // ASCII 'KEYF'

// CoreGraphics & CoreFoundation FFI bindings
type CFMachPortRef = *mut c_void;
type CFRunLoopSourceRef = *mut c_void;
type CFRunLoopRef = *mut c_void;
type CGEventRef = *mut c_void;
type CGEventTapProxy = *mut c_void;

type CGEventTapCallBack = unsafe extern "C" fn(
    proxy: CGEventTapProxy,
    event_type: u32,
    event: CGEventRef,
    user_info: *mut c_void,
) -> CGEventRef;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: CGEventTapCallBack,
        user_info: *mut c_void,
    ) -> CFMachPortRef;

    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
    fn CGEventGetFlags(event: CGEventRef) -> u64;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFMachPortCreateRunLoopSource(
        allocator: *const c_void,
        port: CFMachPortRef,
        order: isize,
    ) -> CFRunLoopSourceRef;

    fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    fn CFRunLoopAddSource(
        rl: CFRunLoopRef,
        source: CFRunLoopSourceRef,
        mode: core_foundation::string::CFStringRef,
    );
    fn CFRunLoopRun();
    fn CFRunLoopStop(rl: CFRunLoopRef);
    fn CFRelease(cf: *const c_void);
}

// Konstanta macOS CoreGraphics
const K_CG_SESSION_EVENT_TAP: u32 = 1;
const K_CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
const K_CG_EVENT_TAP_OPTION_DEFAULT: u32 = 0;

const K_CG_EVENT_KEY_DOWN: u32 = 10;
const K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFFFFFE;
const K_CG_EVENT_TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFFFFFF;

const K_CG_KEYBOARD_EVENT_KEYCODE: u32 = 9;
const K_CG_EVENT_SOURCE_USER_DATA: u32 = 42;

// Modifier flags
const K_CG_EVENT_FLAG_MASK_COMMAND: u64 = 0x00100000;
const K_CG_EVENT_FLAG_MASK_ALTERNATE: u64 = 0x00080000;
const K_CG_EVENT_FLAG_MASK_CONTROL: u64 = 0x00040000;
const K_CG_EVENT_FLAG_MASK_SHIFT: u64 = 0x00020000;

// State global untuk callback hook
static TAP_PORT: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static RUN_LOOP: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

type HookHandlerFn = Box<dyn Fn(crate::KeyEvent) -> crate::HookDecision + Send + Sync>;
static HOOK_CALLBACK: RwLock<Option<HookHandlerFn>> = RwLock::new(None);

/// Menerjemahkan Virtual Keycode macOS ke nama tombol standar.
fn macos_keycode_to_name(keycode: i64) -> Option<&'static str> {
    match keycode {
        18 => Some("1"),
        19 => Some("2"),
        20 => Some("3"),
        21 => Some("4"),
        23 => Some("5"),
        22 => Some("6"),
        26 => Some("7"),
        28 => Some("8"),
        25 => Some("9"),
        29 => Some("0"),
        0 => Some("A"),
        11 => Some("B"),
        8 => Some("C"),
        2 => Some("D"),
        14 => Some("E"),
        3 => Some("F"),
        5 => Some("G"),
        4 => Some("H"),
        34 => Some("I"),
        38 => Some("J"),
        40 => Some("K"),
        37 => Some("L"),
        46 => Some("M"),
        45 => Some("N"),
        31 => Some("O"),
        35 => Some("P"),
        12 => Some("Q"),
        15 => Some("R"),
        1 => Some("S"),
        17 => Some("T"),
        32 => Some("U"),
        9 => Some("V"),
        13 => Some("W"),
        7 => Some("X"),
        16 => Some("Y"),
        6 => Some("Z"),
        36 => Some("ENTER"),
        53 => Some("ESC"),
        49 => Some("SPACE"),
        51 => Some("BACKSPACE"),
        48 => Some("TAB"),
        117 => Some("DELETE"),
        123 => Some("LEFT"),
        124 => Some("RIGHT"),
        125 => Some("DOWN"),
        126 => Some("UP"),
        122 => Some("F1"),
        120 => Some("F2"),
        99 => Some("F3"),
        118 => Some("F4"),
        96 => Some("F5"),
        97 => Some("F6"),
        98 => Some("F7"),
        100 => Some("F8"),
        101 => Some("F9"),
        109 => Some("F10"),
        103 => Some("F11"),
        111 => Some("F12"),
        _ => None,
    }
}

/// Callback C low-level untuk `CGEventTap`.
unsafe extern "C" fn event_tap_proc(
    _proxy: CGEventTapProxy,
    event_type: u32,
    event: CGEventRef,
    _user_info: *mut c_void,
) -> CGEventRef {
    let result = std::panic::catch_unwind(|| {
        // 1. Tangani tap disabled by timeout atau user input (T3.1)
        if event_type == K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT
            || event_type == K_CG_EVENT_TAP_DISABLED_BY_USER_INPUT
        {
            tracing::warn!(
                event_type,
                "CGEventTap dinonaktifkan sistem, mengaktifkan kembali..."
            );
            let port = TAP_PORT.load(Ordering::SeqCst);
            if !port.is_null() {
                // SAFETY: Mengaktifkan kembali CGEventTap yang dinonaktifkan OS.
                unsafe {
                    CGEventTapEnable(port, true);
                }
            }
            return event;
        }

        // Hanya proses penekanan tombol (KeyDown)
        if event_type != K_CG_EVENT_KEY_DOWN {
            return event;
        }

        if event.is_null() {
            return event;
        }

        // 2. Abaikan event sintetis buatan KeyFlow
        // SAFETY: CGEventGetIntegerValueField membaca nilai field event.
        let user_data = unsafe { CGEventGetIntegerValueField(event, K_CG_EVENT_SOURCE_USER_DATA) };
        if user_data == KEYFLOW_MACOS_USER_DATA {
            return event;
        }

        // SAFETY: Membaca keycode dan flags dari CGEvent yang valid.
        let (keycode, flags) = unsafe {
            (
                CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE),
                CGEventGetFlags(event),
            )
        };

        let Some(key_name) = macos_keycode_to_name(keycode) else {
            return event;
        };

        let cmd = (flags & K_CG_EVENT_FLAG_MASK_COMMAND) != 0;
        let alt = (flags & K_CG_EVENT_FLAG_MASK_ALTERNATE) != 0;
        let ctrl = (flags & K_CG_EVENT_FLAG_MASK_CONTROL) != 0;
        let shift = (flags & K_CG_EVENT_FLAG_MASK_SHIFT) != 0;

        let mut key_combo_str = String::new();
        if ctrl {
            key_combo_str.push_str("Ctrl+");
        }
        if alt {
            key_combo_str.push_str("Alt+");
        }
        if shift {
            key_combo_str.push_str("Shift+");
        }
        if cmd {
            key_combo_str.push_str("Meta+");
        }
        key_combo_str.push_str(key_name);

        let key_event = crate::KeyEvent {
            key: key_combo_str,
            pressed: true,
        };

        // 4. Panggil handler terdaftar dengan try_read (non-blocking, fail-open)
        let guard = HOOK_CALLBACK.try_read().ok();
        if let Some(Some(ref handler)) = guard.as_deref() {
            let decision = handler(key_event);
            if decision == crate::HookDecision::Swallow {
                // Menelan tombol: kembalikan NULL pointer di macOS
                return std::ptr::null_mut();
            }
        }

        // Default: teruskan tombol asli (fail-open)
        event
    });

    match result {
        Ok(ev) => ev,
        Err(_) => {
            // Panic safety: selalu teruskan event jika terjadi kesalahan tak terduga
            event
        }
    }
}

/// Pengelola keyboard hook macOS berbasis `CGEventTap`.
pub struct MacosHookManager {
    is_running: Arc<AtomicBool>,
    thread_handle: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl MacosHookManager {
    pub fn new() -> Self {
        Self {
            is_running: Arc::new(AtomicBool::new(false)),
            thread_handle: Mutex::new(None),
        }
    }
}

impl crate::KeyboardHook for MacosHookManager {
    fn start(
        &self,
        handler: Box<dyn Fn(crate::KeyEvent) -> crate::HookDecision + Send + Sync>,
    ) -> Result<(), crate::PlatformError> {
        if self.is_running.load(Ordering::SeqCst) {
            return Ok(());
        }

        // Verifikasi izin Accessibility sebelum memasang hook
        crate::macos::permissions::verify_macos_permissions()?;

        if let Ok(mut lock) = HOOK_CALLBACK.write() {
            *lock = Some(handler);
        }

        let is_running = self.is_running.clone();
        let (init_tx, init_rx) = std::sync::mpsc::channel();

        let handle = std::thread::Builder::new()
            .name("keyflow-macos-hook".into())
            .spawn(move || {
                // Event mask: hanya KeyDown yang perlu di-intercept
                let event_mask: u64 = 1 << K_CG_EVENT_KEY_DOWN;

                // SAFETY: CGEventTapCreate dipanggil dengan parameter valid dan callback extern C.
                let tap_port = unsafe {
                    CGEventTapCreate(
                        K_CG_SESSION_EVENT_TAP,
                        K_CG_HEAD_INSERT_EVENT_TAP,
                        K_CG_EVENT_TAP_OPTION_DEFAULT,
                        event_mask,
                        event_tap_proc,
                        std::ptr::null_mut(),
                    )
                };

                if tap_port.is_null() {
                    let _ = init_tx.send(Err(crate::PlatformError::PermissionDenied(
                        "Gagal membuat CGEventTap. Pastikan izin Accessibility telah diberikan."
                            .into(),
                    )));
                    return;
                }

                // SAFETY: CFMachPortCreateRunLoopSource membuat RunLoop source dari tap port.
                let run_loop_source =
                    unsafe { CFMachPortCreateRunLoopSource(std::ptr::null(), tap_port, 0) };

                if run_loop_source.is_null() {
                    // SAFETY: Melepas tap_port jika pembuatan source gagal untuk mencegah leak.
                    unsafe {
                        CFRelease(tap_port);
                    }
                    TAP_PORT.store(std::ptr::null_mut(), Ordering::SeqCst);
                    let _ = init_tx.send(Err(crate::PlatformError::Os(
                        "Gagal membuat RunLoopSource dari MachPort".into(),
                    )));
                    return;
                }

                TAP_PORT.store(tap_port, Ordering::SeqCst);

                // SAFETY: Mengambil CFRunLoop saat ini dan mendaftarkan RunLoop source.
                let current_rl = unsafe { CFRunLoopGetCurrent() };
                RUN_LOOP.store(current_rl, Ordering::SeqCst);

                let k_common_modes = CFString::new("kCFRunLoopCommonModes");
                // SAFETY: CFRunLoopAddSource menambahkan source ke CFRunLoop thread ini.
                unsafe {
                    CFRunLoopAddSource(
                        current_rl,
                        run_loop_source,
                        k_common_modes.as_concrete_TypeRef(),
                    );
                    CGEventTapEnable(tap_port, true);
                }

                is_running.store(true, Ordering::SeqCst);
                let _ = init_tx.send(Ok(()));

                // Jalankan RunLoop blocking untuk memproses tap events
                // SAFETY: CFRunLoopRun menjalankan event loop thread ini hingga CFRunLoopStop dipanggil.
                unsafe {
                    CFRunLoopRun();
                }

                // Cleanup saat RunLoop selesai
                // SAFETY: Melepas referensi CoreFoundation setelah RunLoop berhenti.
                unsafe {
                    CGEventTapEnable(tap_port, false);
                    CFRelease(run_loop_source);
                    CFRelease(tap_port);
                }

                TAP_PORT.store(std::ptr::null_mut(), Ordering::SeqCst);
                RUN_LOOP.store(std::ptr::null_mut(), Ordering::SeqCst);
                is_running.store(false, Ordering::SeqCst);
            })
            .map_err(|e| {
                crate::PlatformError::Os(format!("Gagal membuat thread hook macOS: {e}"))
            })?;

        if let Ok(mut lock) = self.thread_handle.lock() {
            *lock = Some(handle);
        }

        init_rx
            .recv()
            .map_err(|e| crate::PlatformError::Os(format!("Gagal inisialisasi thread hook: {e}")))?
    }

    fn stop(&self) {
        if !self.is_running.load(Ordering::SeqCst) {
            return;
        }

        let rl = RUN_LOOP.load(Ordering::SeqCst);
        if !rl.is_null() {
            // SAFETY: Menghentikan CFRunLoop thread hook secara aman.
            unsafe {
                CFRunLoopStop(rl);
            }
        }

        if let Ok(mut lock) = self.thread_handle.lock() {
            if let Some(handle) = lock.take() {
                let _ = handle.join();
            }
        }

        if let Ok(mut lock) = HOOK_CALLBACK.write() {
            *lock = None;
        }

        self.is_running.store(false, Ordering::SeqCst);
    }
}

impl Drop for MacosHookManager {
    fn drop(&mut self) {
        use crate::KeyboardHook;
        self.stop();
    }
}

impl Default for MacosHookManager {
    fn default() -> Self {
        Self::new()
    }
}
