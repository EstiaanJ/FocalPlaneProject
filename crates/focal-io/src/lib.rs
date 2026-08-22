//! Shared decoding, metadata, orientation, transparency, and encoding boundary.
//!
//! X-T5 support begins here so camera-file concerns do not leak into
//! `FocalCore` or the editor. The Camera-Neutral rendering itself remains an
//! explicit processing contract rather than an unnamed decoder side effect.

mod raw;

pub use raw::{
    CameraNeutralImage, RawDecodeError, RawSensorImage, RawThumbnail, decode_xt5_camera_neutral,
    decode_xt5_raf, decode_xt5_thumbnail,
};
