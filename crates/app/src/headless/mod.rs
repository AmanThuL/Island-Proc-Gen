//! Headless (windowless) capture pipeline.
//!
//! The entry point is [`request::CaptureRequest`], which is loaded from a RON
//! file and drives batch offline rendering without opening a GPU window.

pub mod request;
