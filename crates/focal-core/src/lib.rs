//! `FocalPlane`'s GUI-independent CPU reference pipeline.
//!
//! Decoders hand Focal Core an [`Image`] with an explicit [`ImageContract`].
//! Encoding is likewise an application boundary. The default pipeline keeps
//! all operations between those boundaries ordered and inspectable.

mod image;
mod module;
mod pipeline;

pub use image::{
    ChannelMeaning, ColourEncoding, Image, ImageContract, ImageError, Primaries, WhitePoint,
};
pub use module::{Module, ModuleKind, ModuleParameters};
pub use pipeline::{Pipeline, PipelineError, PipelineSnapshot, RenderReport, WorkingSpace};
