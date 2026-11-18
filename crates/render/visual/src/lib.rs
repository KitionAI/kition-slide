//! Visual fidelity of Kition Slide's renderer against reference renders.
//!
//! - [`compare`]: the comparator (SSIM, exact-equal and over-threshold pixel ratios, diff box,
//!   diff images) with its frozen metric manifest;
//! - [`corpus`]: finding slides and their reference renders under a reference root;
//! - [`gate`]: the non-regression gate against an accepted baseline scorecard;
//! - [`regions`]: which kind of object differing pixels fall on, the features a slide uses, and
//!   fonts the slide requests but cannot find;
//! - [`runner`]: the resumable, time-boxed corpus run and its scorecard.
//!
//! `tests/reference.rs` runs the corpus when `KITION_SLIDE_REFERENCE_ROOT` is set;
//! `tests/comparator.rs` validates the comparator on renders of synthetic slides.

mod cache;
pub mod compare;
pub mod corpus;
pub mod feature_pairs;
pub mod gate;
pub mod regions;
pub mod runner;
