//! Rendering egui jendela pengaturan. Logika bisnis ada di `state` dan `save`.

use std::path::PathBuf;

use eframe::egui;
use keyflow_core::config::{OnConflict, Settings};

use super::save::{self, LoadOutcome, SaveError, SaveMode, SaveOutcome};
use super::state::FormState;

/// Membuka jendela pengaturan dan memblokir sampai ditutup.
pub fn run(config_path: PathBuf) -> anyhow::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Pengaturan KeyFlow")
            .with_inner_size([500.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Pengaturan KeyFlow",
        options,
        Box::new(move |_cc| Ok(Box::new(SettingsApp::new(config_path)))),
    )
    .map_err(|e| anyhow::anyhow!("Gagal membuka jendela pengaturan: {e}"))
}

#[derive(Clone)]
enum Dialog {
    ConfirmOverwrite,
    ConfirmFullRewrite(String),
    ChangedOnDisk,
}

enum Status {
    Saved,
    Error(String),
}

struct SettingsApp {
    path: PathBuf,
    /// Isi berkas saat dimuat; `None` bila berkas belum ada.
    loaded_text: Option<String>,
    form: FormState,
    /// Terisi bila config di disk tidak valid; form dinonaktifkan.
    blocked: Option<String>,
    dialog: Option<Dialog>,
    status: Option<Status>,
}

impl SettingsApp {
    fn new(path: PathBuf) -> Self {
        let mut app = Self {
            path,
            loaded_text: None,
            form: FormState::new(Settings::default()),
            blocked: None,
            dialog: None,
            status: None,
        };
        app.reload();
        app
    }

    /// Membuang isi form dan membaca ulang dari disk.
    fn reload(&mut self) {
        self.status = None;
        self.dialog = None;
        match save::load(&self.path) {
            LoadOutcome::Missing => {
                self.loaded_text = None;
                self.blocked = None;
                self.form = FormState::new(Settings::default());
            }
            LoadOutcome::Loaded { text, settings } => {
                self.loaded_text = Some(text);
                self.blocked = None;
                self.form = FormState::new(settings);
            }
            LoadOutcome::Invalid { message } => {
                self.loaded_text = None;
                self.blocked = Some(message);
                self.form = FormState::new(Settings::default());
            }
        }
    }

    fn try_save(&mut self, mode: SaveMode) {
        let Some(settings) = self.form.draft() else {
            return;
        };
        match save::save(&self.path, self.loaded_text.as_deref(), &settings, mode) {
            Ok(SaveOutcome::Saved) => {
                tracing::info!(path = %self.path.display(), "Pengaturan disimpan dari GUI");
                self.reload();
                self.status = Some(Status::Saved);
            }
            Ok(SaveOutcome::NeedsFullRewrite(reason)) => {
                self.dialog = Some(Dialog::ConfirmFullRewrite(reason.to_string()));
            }
            Err(SaveError::ChangedOnDisk) => self.dialog = Some(Dialog::ChangedOnDisk),
            Err(error) => {
                tracing::warn!(%error, "Gagal menyimpan pengaturan dari GUI");
                self.status = Some(Status::Error(error.to_string()));
            }
        }
    }

    fn draw_form(&mut self, ui: &mut egui::Ui) {
        ui.heading("Pengaturan KeyFlow");
        ui.label(
            egui::RichText::new(self.path.display().to_string())
                .weak()
                .small(),
        );
        ui.add_space(8.0);

        if let Some(message) = &self.blocked {
            ui.colored_label(
                egui::Color32::LIGHT_RED,
                "Config saat ini tidak valid, jadi pengaturan tidak dapat disimpan dari sini. \
                 Perbaiki config.yaml, lalu klik \"Muat ulang\".",
            );
            ui.add_space(4.0);
            egui::ScrollArea::vertical()
                .max_height(240.0)
                .show(ui, |ui| {
                    ui.monospace(message);
                });
            ui.add_space(8.0);
            if ui.button("Muat ulang").clicked() {
                self.reload();
            }
            return;
        }

        ui.checkbox(&mut self.form.dry_run, "Mode simulasi (dry run)");
        ui.label(
            egui::RichText::new(
                "Berkas fisik tidak disentuh; hanya dicatat di log dan notifikasi.",
            )
            .weak(),
        );
        ui.add_space(6.0);
        ui.checkbox(&mut self.form.notifications, "Tampilkan notifikasi desktop");
        ui.add_space(6.0);
        ui.checkbox(
            &mut self.form.create_missing_dirs,
            "Buat folder tujuan otomatis bila belum ada",
        );
        ui.add_space(6.0);

        let mut picked = None;
        ui.horizontal(|ui| {
            ui.label("Jika nama berkas sama:");
            egui::ComboBox::from_id_salt("on_conflict")
                .selected_text(conflict_label(self.form.on_conflict()))
                .show_ui(ui, |ui| {
                    for option in [
                        OnConflict::Rename,
                        OnConflict::Skip,
                        OnConflict::Overwrite,
                        OnConflict::Ask,
                    ] {
                        let selected = self.form.on_conflict() == option;
                        if ui
                            .selectable_label(selected, conflict_label(option))
                            .clicked()
                        {
                            picked = Some(option);
                        }
                    }
                });
        });
        if let Some(option) = picked {
            if self.form.select_on_conflict(option) {
                self.dialog = Some(Dialog::ConfirmOverwrite);
            }
        }
        ui.label(egui::RichText::new(conflict_hint(self.form.on_conflict())).weak());
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.label("Batas riwayat undo:");
            ui.add(egui::TextEdit::singleline(&mut self.form.undo_limit_text).desired_width(80.0));
        });
        if let Some(error) = self.form.undo_limit_error() {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }

        ui.add_space(12.0);
        ui.separator();
        ui.horizontal(|ui| {
            let can_save = self.form.can_save();
            if ui
                .add_enabled(can_save, egui::Button::new("Simpan"))
                .clicked()
            {
                self.try_save(SaveMode::PatchOnly);
            }
            if ui.button("Muat ulang").clicked() {
                self.reload();
            }
            if self.form.is_dirty() {
                ui.label(egui::RichText::new("Ada perubahan yang belum disimpan").weak());
            }
        });
        match &self.status {
            Some(Status::Saved) => {
                ui.colored_label(egui::Color32::LIGHT_GREEN, "Tersimpan.");
            }
            Some(Status::Error(message)) => {
                ui.colored_label(egui::Color32::LIGHT_RED, message);
            }
            None => {}
        }
    }

    fn draw_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.dialog.clone() else {
            return;
        };
        match dialog {
            Dialog::ConfirmOverwrite => {
                dialog_window("Konfirmasi: timpa berkas").show(ctx, |ui| {
                    ui.label(
                        "Dengan \"timpa\", berkas lama di folder tujuan akan diganti oleh berkas \
                         baru bernama sama dan bisa hilang. Pilihan aman adalah \"rename\" (bawaan).",
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Ya, timpa").clicked() {
                            self.form.confirm_overwrite();
                            self.dialog = None;
                        }
                        if ui.button("Batal").clicked() {
                            self.form.cancel_overwrite();
                            self.dialog = None;
                        }
                    });
                });
            }
            Dialog::ConfirmFullRewrite(reason) => {
                dialog_window("Tulis ulang seluruh berkas?").show(ctx, |ui| {
                    ui.label(format!(
                        "Config tidak dapat diubah tanpa mengubah formatnya ({reason}). \
                         Anda dapat menulis ulang seluruh berkas: komentar akan hilang, dan \
                         cadangan config.yaml.bak dibuat lebih dulu."
                    ));
                    ui.horizontal(|ui| {
                        if ui.button("Tulis ulang penuh").clicked() {
                            self.dialog = None;
                            self.try_save(SaveMode::AllowFullRewrite);
                        }
                        if ui.button("Batal").clicked() {
                            self.dialog = None;
                        }
                    });
                });
            }
            Dialog::ChangedOnDisk => {
                dialog_window("Config berubah di tempat lain").show(ctx, |ui| {
                    ui.label(
                        "config.yaml diubah di luar jendela ini sejak dimuat. Muat ulang untuk \
                         melihat versi terbaru; perubahan di form ini akan dibuang.",
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Muat ulang").clicked() {
                            self.reload();
                        }
                        if ui.button("Tutup").clicked() {
                            self.dialog = None;
                        }
                    });
                });
            }
        }
    }
}

impl eframe::App for SettingsApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        egui::CentralPanel::default().show(ui, |ui| self.draw_form(ui));
        self.draw_dialog(&ctx);
    }
}

fn dialog_window(title: &str) -> egui::Window<'_> {
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
}

fn conflict_label(option: OnConflict) -> &'static str {
    match option {
        OnConflict::Rename => "rename (disarankan)",
        OnConflict::Skip => "skip",
        OnConflict::Overwrite => "overwrite (berisiko)",
        OnConflict::Ask => "ask",
    }
}

fn conflict_hint(option: OnConflict) -> &'static str {
    match option {
        OnConflict::Rename => {
            "Nama otomatis diberi nomor, mis. foto (1).jpg. Tidak ada berkas tertimpa."
        }
        OnConflict::Skip => "Berkas dilewati bila nama sudah ada di tujuan.",
        OnConflict::Overwrite => "Berkas lama di tujuan ditimpa. Gunakan dengan hati-hati.",
        OnConflict::Ask => "Meminta konfirmasi setiap kali ada nama yang sama.",
    }
}
