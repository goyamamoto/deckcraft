//! DeckCraft equations.
//!
//! A TeX subset ([`tex::parse`]) and Office Math ([`omml::read`]) both give the same equation
//! tree ([`ast::Node`]); the tree is written as OMML ([`omml::write`], what a PPTX carries), as
//! TeX again ([`tex::to_tex`], to edit an imported equation), as one line of plain text
//! ([`linear::linear`], the fallback text) and laid out TeX-fashion over any font's metrics
//! ([`layout::layout`]). Pure, platform-free, no fonts of its own: wasm-safe.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod ast;
pub mod layout;
pub mod linear;
pub mod omml;
pub mod symbols;
pub mod tex;

pub use ast::{Math, Node, Style};
pub use layout::{Glyph, Layout, MathFont, PlacedGlyph};
pub use tex::TexError;

/// Parse TeX to the equation tree.
pub fn parse_tex(s: &str) -> Result<Math, TexError> {
    tex::parse(s)
}

/// An equation from TeX as what a presentation stores: its OMML and its linear text.
pub fn tex_to_omml(s: &str) -> Result<(String, String), TexError> {
    tex_to_omml_runs(s, "")
}

/// [`tex_to_omml`] with `run_props` XML in every OMML run ([`omml::write_runs`]).
pub fn tex_to_omml_runs(s: &str, run_props: &str) -> Result<(String, String), TexError> {
    let m = tex::parse(s)?;
    Ok((omml::write_runs(&m, run_props), linear::linear(&m)))
}

/// The TeX of a stored equation, for editing it; `None` when the OMML is not XML.
pub fn omml_to_tex(omml: &str) -> Option<String> {
    omml::read(omml).map(|m| tex::to_tex(&m))
}

#[cfg(test)]
mod tests;
