//! Climate stages: temperature, precipitation, fog likelihood.
//!
//! Ordering (per the canonical pipeline): `Temperature → Precipitation →
//! FogLikelihood`. Precipitation and fog both consume
//! [`common::signed_uplift`] so the wind / grad-z sign convention lives
//! in one place.

pub mod common;
pub mod precipitation;
pub mod temperature;

pub use precipitation::PrecipitationStage;
pub use temperature::TemperatureStage;
