//! Deteksi dan panduan izin Accessibility dan Input Monitoring di macOS (T3.2).
//!
//! Di macOS 10.15+, aplikasi yang ingin memasang `CGEventTap` memerlukan:
//! 1. Izin Accessibility (Privacy & Security > Accessibility)
//! 2. Izin Input Monitoring (Privacy & Security > Input Monitoring)

use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::CFString;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: core_foundation::dictionary::CFDictionaryRef)
        -> bool;
}

/// Memeriksa apakah aplikasi memiliki izin Accessibility yang valid.
pub fn is_accessibility_trusted() -> bool {
    // SAFETY: AXIsProcessTrusted adalah fungsi FFI resmi Apple tanpa efek samping.
    unsafe { AXIsProcessTrusted() }
}

/// Meminta prompt dialog izin Accessibility sistem jika belum diberikan.
pub fn request_accessibility_permissions() -> bool {
    let key = CFString::new("AXTrustedCheckOptionPrompt");
    let value = CFBoolean::true_value();

    let options = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), value.as_CFType())]);

    // SAFETY: AXIsProcessTrustedWithOptions dipanggil dengan CFDictionaryRef yang valid.
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) }
}

/// Memverifikasi seluruh izin macOS yang dibutuhkan dan mengembalikan pesan panduan jika belum lengkap.
pub fn verify_macos_permissions() -> Result<(), crate::PlatformError> {
    if !is_accessibility_trusted() {
        // Tampilkan dialog izin OS jika memungkinkan
        request_accessibility_permissions();

        return Err(crate::PlatformError::PermissionDenied(
            "KeyFlow memerlukan izin 'Accessibility' dan 'Input Monitoring' untuk memantau shortcut.\n\
             Silakan buka: System Settings > Privacy & Security > Accessibility, lalu aktifkan KeyFlow.\n\
             Lihat petunjuk lengkap di docs/PERMISSIONS.md".to_string(),
        ));
    }

    Ok(())
}
