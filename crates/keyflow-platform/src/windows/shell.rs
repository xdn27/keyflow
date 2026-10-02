//! Modul pembacaan konteks Explorer via COM Shell Windows.
//!
//! Mengimplementasikan T0.2:
//! - Berjalan di thread STA (Single-Threaded Apartment) khusus.
//! - Menemukan jendela Explorer aktif melalui `IShellWindows` -> HWND -> `IShellBrowser` / `IFolderView2`.
//! - Membaca path folder aktif dan item terseleksi (`SVGIO_SELECTION`).
//! - Menangani kasus multi-jendela/tab, folder virtual khusus (This PC, Search), dan tanpa seleksi.

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::{Builder, JoinHandle};
use std::time::Instant;

use ::windows::core::{Interface, GUID};
use ::windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, IDispatch, IServiceProvider,
    CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED,
};
use ::windows::Win32::System::Variant::VARIANT;
use ::windows::Win32::UI::Shell::{
    IFolderView2, IShellBrowser, IShellItem, IShellItemArray, IShellView, IShellWindows,
    ShellWindows, SIGDN_FILESYSPATH, SVGIO_SELECTION,
};
use ::windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW, IsChild};

use crate::PlatformError;

pub const SID_S_TOP_LEVEL_BROWSER: GUID = GUID::from_u128(0x4C96BE40_915C_11CF_99D3_00AA004AE837);

/// Konteks Explorer yang dibaca via COM.
#[derive(Debug, Clone, Default)]
pub struct ExplorerShellContext {
    /// Path folder aktif saat ini (None jika folder virtual seperti "This PC" / Search).
    pub active_folder: Option<PathBuf>,
    /// Daftar path file/folder yang sedang dipilih.
    pub selected_items: Vec<PathBuf>,
    /// Judul jendela Explorer.
    pub window_title: String,
    /// Durasi eksekusi query COM (mikrodetik).
    pub query_duration_us: u64,
}

enum ShellThreadRequest {
    QueryActiveExplorer(Sender<Result<ExplorerShellContext, PlatformError>>),
    Shutdown,
}

/// Pengelola thread COM Shell Windows (STA).
pub struct WindowsShellClient {
    tx: Sender<ShellThreadRequest>,
    join_handle: Option<JoinHandle<()>>,
}

impl WindowsShellClient {
    /// Memulai thread STA khusus untuk COM Shell Windows.
    pub fn new() -> Result<Self, PlatformError> {
        let (tx, rx) = channel();
        let (init_tx, init_rx) = channel();

        let handle = Builder::new()
            .name("keyflow-shell-sta".into())
            .spawn(move || {
                // Inisialisasi COM apartemen STA di thread ini
                // SAFETY: CoInitializeEx dipanggil di awal siklus hidup thread STA.
                let co_init = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
                if let Err(e) = co_init.ok() {
                    let _ = init_tx.send(Err(PlatformError::Os(format!(
                        "Gagal inisialisasi COM STA: {e}"
                    ))));
                    return;
                }

                let _ = init_tx.send(Ok(()));
                run_shell_event_loop(rx);

                // SAFETY: CoUninitialize dipanggil sebelum thread keluar.
                unsafe {
                    CoUninitialize();
                }
            })
            .map_err(|e| PlatformError::Os(format!("Gagal membuat thread shell STA: {e}")))?;

        init_rx.recv().map_err(|e| {
            PlatformError::Os(format!("Gagal sinkronisasi inisialisasi STA: {e}"))
        })??;

        Ok(Self {
            tx,
            join_handle: Some(handle),
        })
    }

    /// Mengambil konteks folder aktif dan file terpilih dari Explorer yang sedang fokus.
    pub fn get_active_context(&self) -> Result<ExplorerShellContext, PlatformError> {
        let (resp_tx, resp_rx) = channel();
        self.tx
            .send(ShellThreadRequest::QueryActiveExplorer(resp_tx))
            .map_err(|e| PlatformError::Os(format!("Gagal mengirim request ke thread COM: {e}")))?;

        resp_rx
            .recv()
            .map_err(|e| PlatformError::Os(format!("Gagal menerima respon dari thread COM: {e}")))?
    }
}

impl Drop for WindowsShellClient {
    fn drop(&mut self) {
        let _ = self.tx.send(ShellThreadRequest::Shutdown);
        if let Some(h) = self.join_handle.take() {
            let _ = h.join();
        }
    }
}

/// Loop penerima perintah pada thread STA.
fn run_shell_event_loop(rx: Receiver<ShellThreadRequest>) {
    while let Ok(req) = rx.recv() {
        match req {
            ShellThreadRequest::QueryActiveExplorer(resp_tx) => {
                let start = Instant::now();
                let res = query_active_explorer_internal();
                let duration = start.elapsed().as_micros() as u64;

                let adjusted_res = res.map(|mut ctx| {
                    ctx.query_duration_us = duration;
                    ctx
                });

                let _ = resp_tx.send(adjusted_res);
            }
            ShellThreadRequest::Shutdown => break,
        }
    }
}

/// Query internal COM `IShellWindows` untuk jendela Explorer yang sedang fokus.
fn query_active_explorer_internal() -> Result<ExplorerShellContext, PlatformError> {
    // SAFETY: GetForegroundWindow untuk membaca jendela aktif saat ini.
    let fg_hwnd = unsafe { GetForegroundWindow() };
    if fg_hwnd.0.is_null() {
        return Ok(ExplorerShellContext::default());
    }

    // SAFETY: Menginstansiasi COM IShellWindows dari ShellWindows CLSID.
    let shell_windows: IShellWindows = unsafe {
        CoCreateInstance(&ShellWindows, None, CLSCTX_LOCAL_SERVER)
            .map_err(|e| PlatformError::Os(format!("Gagal CoCreateInstance ShellWindows: {e}")))?
    };

    // SAFETY: Count mengambil jumlah jendela shell yang terdaftar.
    let count = unsafe {
        shell_windows
            .Count()
            .map_err(|e| PlatformError::Os(format!("Gagal mengambil ShellWindows Count: {e}")))?
    };

    let mut matched_context: Option<ExplorerShellContext> = None;

    for i in 0..count {
        let index = VARIANT::from(i);
        // SAFETY: Memanggil Item pada IShellWindows dengan indeks VARIANT.
        let dispatch: Result<IDispatch, _> = unsafe { shell_windows.Item(&index) };
        let Ok(disp) = dispatch else {
            continue;
        };

        // Query IServiceProvider
        let Ok(sp) = disp.cast::<IServiceProvider>() else {
            continue;
        };

        // Query IShellBrowser melalui SID_STopLevelBrowser
        // SAFETY: QueryService COM dengan GUID SID_S_TOP_LEVEL_BROWSER.
        let browser: Result<IShellBrowser, _> =
            unsafe { sp.QueryService(&SID_S_TOP_LEVEL_BROWSER) };
        let Ok(browser) = browser else {
            continue;
        };

        // Ambil HWND jendela browser ini
        // SAFETY: GetWindow membaca handle jendela IShellBrowser.
        let browser_hwnd = match unsafe { browser.GetWindow() } {
            Ok(h) => h,
            Err(_) => continue,
        };

        // Periksa apakah jendela ini cocok dengan fg_hwnd
        // Di Windows 11 Explorer dengan tabs, fg_hwnd bisa berupa child tab dari browser_hwnd
        // SAFETY: IsChild memeriksa hierarki HWND jika bukan perbandingan langsung.
        let is_match =
            (browser_hwnd == fg_hwnd) || unsafe { IsChild(browser_hwnd, fg_hwnd) }.as_bool();
        if !is_match {
            continue;
        }

        // Baca judul jendela
        let mut title_buf = [0u16; 512];
        // SAFETY: GetWindowTextW dipanggil dengan buffer valid berukuran 512 elemen.
        let title_len = unsafe { GetWindowTextW(browser_hwnd, &mut title_buf) };
        let title = if title_len > 0 {
            String::from_utf16_lossy(&title_buf[..title_len as usize])
        } else {
            String::new()
        };

        // Query IShellView yang sedang aktif di tab/jendela ini
        // SAFETY: QueryActiveShellView mengambil tampilan aktif dari browser.
        let shell_view: Result<IShellView, _> = unsafe { browser.QueryActiveShellView() };
        let Ok(view) = shell_view else {
            continue;
        };

        // Cast ke IFolderView2 (atau IFolderView)
        let Ok(folder_view) = view.cast::<IFolderView2>() else {
            continue;
        };

        // 1. Baca folder aktif
        // SAFETY: GetFolder::<IShellItem>() mengambil Shell Item representasi folder aktif.
        let active_folder = match unsafe { folder_view.GetFolder::<IShellItem>() } {
            Ok(shell_item) => {
                // SAFETY: GetDisplayName dengan SIGDN_FILESYSPATH.
                // Jika folder adalah folder virtual (Search, This PC, Recycle Bin), ini akan mengembalikan error.
                match unsafe { shell_item.GetDisplayName(SIGDN_FILESYSPATH) } {
                    Ok(pwstr) => {
                        // SAFETY: pwstr merupakan pointer string null-terminated UTF-16 yang valid dari IShellItem.
                        let path_str = unsafe { pwstr.to_string().ok() };
                        // SAFETY: CoTaskMemFree wajib dipanggil untuk membebaskan alokasi PWSTR dari Shell.
                        unsafe {
                            CoTaskMemFree(Some(pwstr.as_ptr().cast()));
                        }
                        path_str.map(PathBuf::from)
                    }
                    Err(_) => {
                        // Folder virtual (bukan filesystem path biasa, mis. PC Ini / Search)
                        None
                    }
                }
            }
            Err(_) => None,
        };

        // 2. Baca file terpilih (selection)
        let mut selected_items = Vec::new();
        // SAFETY: Items dengan SVGIO_SELECTION mengambil daftar item yang dipilih.
        if let Ok(item_array) = unsafe { folder_view.Items::<IShellItemArray>(SVGIO_SELECTION) } {
            // SAFETY: GetCount mengambil jumlah item terpilih.
            if let Ok(item_count) = unsafe { item_array.GetCount() } {
                for item_idx in 0..item_count {
                    // SAFETY: GetItemAt mengambil item ke-item_idx.
                    if let Ok(item) = unsafe { item_array.GetItemAt(item_idx) } {
                        // SAFETY: GetDisplayName SIGDN_FILESYSPATH mengambil path file.
                        if let Ok(pwstr) = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) } {
                            // SAFETY: pwstr merupakan pointer string null-terminated UTF-16 yang valid dari IShellItem.
                            if let Ok(path_str) = unsafe { pwstr.to_string() } {
                                selected_items.push(PathBuf::from(path_str));
                            }
                            // SAFETY: Membebaskan PWSTR dari Shell.
                            unsafe {
                                CoTaskMemFree(Some(pwstr.as_ptr().cast()));
                            }
                        }
                    }
                }
            }
        }

        matched_context = Some(ExplorerShellContext {
            active_folder,
            selected_items,
            window_title: title,
            query_duration_us: 0,
        });

        break;
    }

    Ok(matched_context.unwrap_or_default())
}
