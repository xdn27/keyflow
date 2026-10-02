//! State form pengaturan: logika murni tanpa egui agar mudah diuji.

use keyflow_core::config::{OnConflict, Settings};
use keyflow_core::config_edit::UNDO_HISTORY_LIMIT_RANGE;

/// Nilai form yang sedang diedit beserta nilai yang terakhir tersimpan.
pub struct FormState {
    saved: Settings,
    pub dry_run: bool,
    pub notifications: bool,
    pub create_missing_dirs: bool,
    /// Teks input angka; bisa berisi nilai tidak valid selama pengguna mengetik.
    pub undo_limit_text: String,
    on_conflict: OnConflict,
    pending_overwrite: bool,
}

impl FormState {
    pub fn new(saved: Settings) -> Self {
        Self {
            dry_run: saved.dry_run,
            notifications: saved.notifications,
            create_missing_dirs: saved.create_missing_dirs,
            undo_limit_text: saved.undo_history_limit.to_string(),
            on_conflict: saved.on_conflict,
            pending_overwrite: false,
            saved,
        }
    }

    pub fn on_conflict(&self) -> OnConflict {
        self.on_conflict
    }

    /// Memilih mode konflik. Mengembalikan `true` bila perlu konfirmasi eksplisit
    /// (memilih `overwrite`); pilihan baru belum diterapkan sampai dikonfirmasi.
    pub fn select_on_conflict(&mut self, choice: OnConflict) -> bool {
        if choice == OnConflict::Overwrite && self.on_conflict != OnConflict::Overwrite {
            self.pending_overwrite = true;
            return true;
        }
        self.on_conflict = choice;
        self.pending_overwrite = false;
        false
    }

    pub fn confirm_overwrite(&mut self) {
        if self.pending_overwrite {
            self.on_conflict = OnConflict::Overwrite;
            self.pending_overwrite = false;
        }
    }

    pub fn cancel_overwrite(&mut self) {
        self.pending_overwrite = false;
    }

    /// Pesan galat untuk input batas undo, atau `None` bila valid.
    pub fn undo_limit_error(&self) -> Option<String> {
        match self.undo_limit_text.trim().parse::<usize>() {
            Err(_) => Some("Masukkan angka bulat positif.".to_string()),
            Ok(n) if !UNDO_HISTORY_LIMIT_RANGE.contains(&n) => Some(format!(
                "Harus antara {} dan {}.",
                UNDO_HISTORY_LIMIT_RANGE.start(),
                UNDO_HISTORY_LIMIT_RANGE.end()
            )),
            Ok(_) => None,
        }
    }

    /// `Settings` dari isi form, atau `None` bila ada input yang belum valid.
    pub fn draft(&self) -> Option<Settings> {
        let undo_history_limit = self.undo_limit_text.trim().parse::<usize>().ok()?;
        if !UNDO_HISTORY_LIMIT_RANGE.contains(&undo_history_limit) {
            return None;
        }
        Some(Settings {
            dry_run: self.dry_run,
            notifications: self.notifications,
            on_conflict: self.on_conflict,
            create_missing_dirs: self.create_missing_dirs,
            undo_history_limit,
        })
    }

    /// Ada perbedaan dari yang tersimpan (input tidak valid dihitung berbeda).
    pub fn is_dirty(&self) -> bool {
        self.draft().as_ref() != Some(&self.saved)
    }

    pub fn can_save(&self) -> bool {
        self.draft().is_some() && self.is_dirty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_baru_tidak_dirty_dan_tidak_bisa_disimpan() {
        let form = FormState::new(Settings::default());
        assert!(!form.is_dirty());
        assert!(!form.can_save());
        assert_eq!(form.draft(), Some(Settings::default()));
    }

    #[test]
    fn mengubah_field_membuat_dirty_dan_bisa_disimpan() {
        let mut form = FormState::new(Settings::default());
        form.dry_run = true;
        assert!(form.is_dirty());
        assert!(form.can_save());
        assert_eq!(form.draft().map(|s| s.dry_run), Some(true));
    }

    #[test]
    fn mengembalikan_nilai_awal_menghilangkan_dirty() {
        let mut form = FormState::new(Settings::default());
        form.notifications = false;
        form.notifications = true;
        assert!(!form.is_dirty());
    }

    #[test]
    fn batas_undo_tidak_valid_menolak_simpan() {
        let mut form = FormState::new(Settings::default());
        for bad in ["", "abc", "-5", "0", "10001", "2.5"] {
            form.undo_limit_text = bad.to_string();
            assert!(
                form.undo_limit_error().is_some(),
                "{bad:?} seharusnya galat"
            );
            assert_eq!(form.draft(), None, "{bad:?}");
            assert!(!form.can_save(), "{bad:?}");
        }
    }

    #[test]
    fn batas_undo_di_tepi_rentang_valid() {
        let mut form = FormState::new(Settings::default());
        for ok in ["1", " 50 ", "10000"] {
            form.undo_limit_text = ok.to_string();
            assert_eq!(form.undo_limit_error(), None, "{ok:?}");
            assert!(form.draft().is_some(), "{ok:?}");
        }
    }

    #[test]
    fn memilih_overwrite_butuh_konfirmasi_dan_belum_diterapkan() {
        let mut form = FormState::new(Settings::default());
        assert!(form.select_on_conflict(OnConflict::Overwrite));
        assert_eq!(form.on_conflict(), OnConflict::Rename);
        assert!(!form.is_dirty());
    }

    #[test]
    fn konfirmasi_overwrite_menerapkan_pilihan() {
        let mut form = FormState::new(Settings::default());
        form.select_on_conflict(OnConflict::Overwrite);
        form.confirm_overwrite();
        assert_eq!(form.on_conflict(), OnConflict::Overwrite);
        assert!(form.is_dirty());
    }

    #[test]
    fn batal_overwrite_mempertahankan_nilai_sebelumnya() {
        let mut form = FormState::new(Settings::default());
        form.select_on_conflict(OnConflict::Overwrite);
        form.cancel_overwrite();
        form.confirm_overwrite(); // tanpa pending, tidak boleh menerapkan apa pun
        assert_eq!(form.on_conflict(), OnConflict::Rename);
    }

    #[test]
    fn pilihan_lain_diterapkan_langsung_tanpa_konfirmasi() {
        let mut form = FormState::new(Settings::default());
        assert!(!form.select_on_conflict(OnConflict::Skip));
        assert_eq!(form.on_conflict(), OnConflict::Skip);
        assert!(!form.select_on_conflict(OnConflict::Ask));
        assert_eq!(form.on_conflict(), OnConflict::Ask);
    }

    #[test]
    fn dari_overwrite_ke_overwrite_tidak_meminta_konfirmasi_lagi() {
        let saved = Settings {
            on_conflict: OnConflict::Overwrite,
            ..Settings::default()
        };
        let mut form = FormState::new(saved);
        assert!(!form.select_on_conflict(OnConflict::Overwrite));
    }
}
