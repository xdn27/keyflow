//! Inti KeyFlow. Platform-agnostic: tidak boleh ada kode OS di crate ini.
//!
//! Lihat `docs/agents/architecture.md` dan `docs/agents/safety.md`.

#![forbid(unsafe_code)]

pub mod actions;
pub mod config;
pub mod matcher;
pub mod undo;
