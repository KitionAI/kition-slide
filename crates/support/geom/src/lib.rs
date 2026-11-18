//! Kition Slide geometry: units, frames and the preset shape outlines come from
//! `kition-office-core`, shared by Kition's kits (moved there from this crate unchanged); the morph
//! transition's contour matching stays here.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod morph;
pub mod stored;

pub use kition_office_core::geom::*;
pub use kition_office_core::ink::centre_line as ink_centre_line;
pub use kition_office_core::preset;
pub use kition_office_core::stroke;
