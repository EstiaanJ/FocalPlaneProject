//! `FocalPlane`'s GUI-independent CPU reference pipeline.
//!
//! Decoders hand Focal Core an [`Image`] with an explicit [`ImageContract`].
//! Encoding is likewise an application boundary. The default pipeline keeps
//! all operations between those boundaries ordered and inspectable.

mod curve;
mod execution;
mod image;
mod module;
mod pipeline;
mod processing;
pub mod scope;

pub use curve::{
    ADOBE_RGB_LUMA_COEFFICIENTS, CurveChannel, CurveError, CurveMode, CurvePoint, CurveSet,
    SmoothCurve, adobe_rgb_luma,
};
pub use execution::{
    CancellationToken, ProgressReporter, RenderContext, RenderProgress, RenderQuality,
};
pub use image::{
    ChannelMeaning, ColourEncoding, Image, ImageContract, ImageError, Primaries, WhitePoint,
};
pub use module::{CropSettings, Module, ModuleKind, ModuleParameters};
pub use pipeline::{
    PIPELINE_VERSION, Pipeline, PipelineError, PipelineSnapshot, RenderReport, RenderStageReport,
    RenderStageStatus, WorkingSpace,
};
