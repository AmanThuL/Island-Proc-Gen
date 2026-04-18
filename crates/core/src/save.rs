//! Save/load codec for [`WorldState`].
//!
//! # Byte-level API
//!
//! This module operates **only** on `impl Write` / `impl Read` — no
//! `std::fs::File`, no `&Path`. This is intentional: Sprint 5's wasm/web
//! target will plug in IndexedDB blobs without touching this module.
//!
//! The thin `std::fs` wrapper lives in `crates/app/src/save_io.rs`.
//!
//! # Outer frame (all modes)
//!
//! ```text
//! offset  bytes  meaning
//!   0       4    magic = b"IPGS"
//!   4       4    format_version = 1 (little-endian u32)
//!   8       1    mode discriminant: 0=SeedReplay, 1=Minimal, 2=Full, 3=DebugCapture
//!   9       …    mode-specific payload
//! ```
//!
//! # SeedReplay payload (from byte 9)
//!
//! ```text
//! offset  bytes  meaning
//!   0       8    seed.0 (u64, LE)
//!   8       4    resolution.sim_width (u32, LE)
//!  12       4    resolution.sim_height (u32, LE)
//!  16       4    preset_name_len (u32, LE)
//!  20       N    preset_name UTF-8 bytes
//! ```
//!
//! `SeedReplay` stores only the **preset name**, not the full preset. The
//! caller (e.g. `app::save_io` or the wasm host) is responsible for calling
//! `data::presets::load_preset(name)` to recover the full preset. `read_world`
//! returns [`LoadedWorld::SeedReplay`] with `preset_name: String` so the app
//! layer can do this. Sprint 0 tests verify the name round-trips correctly;
//! Sprint 1A+ will wire the full restoration.
//!
//! # Minimal payload (from byte 9)
//!
//! ```text
//! offset  bytes  meaning
//!   0       4    resolution.sim_width (u32, LE)
//!   4       4    resolution.sim_height (u32, LE)
//!   8       8    seed.0 (u64, LE)
//!  16       4    preset_ron_len (u32, LE)
//!  20       N    preset RON bytes (UTF-8, ron::to_string(&preset))
//!  20+N     4    height_bytes_len (u32, LE)
//!  24+N     M    ScalarField2D<f32>::to_bytes() for height
//!  24+N+M   4    sediment_bytes_len (u32, LE)
//!  28+N+M   K    ScalarField2D<f32>::to_bytes() for sediment
//! ```
//!
//! Sprint 0: both `height` and `sediment` must be `Some` — the writer returns
//! [`SaveError::MissingAuthoritativeField`] if either is `None`. Sprint 1A will
//! iterate on this (sediment becomes optional until Sprint 3), bumping
//! `format_version` to 2 and adding a present-flag byte for each field.

use std::io::{Read, Write};

use crate::field::{FieldDecodeError, ScalarField2D};
use crate::preset::IslandArchetypePreset;
use crate::seed::Seed;
use crate::world::{Resolution, WorldState};

// ─── constants ───────────────────────────────────────────────────────────────

const MAGIC: [u8; 4] = *b"IPGS";
const FORMAT_VERSION: u32 = 1;

// ─── SaveMode ────────────────────────────────────────────────────────────────

/// The save density tier. Sprint 0 implements `SeedReplay` and `Minimal`;
/// `Full` and `DebugCapture` are reserved for Sprint 1B+ and Sprint 4+.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveMode {
    /// ~100 bytes: seed + preset name + resolution. Sufficient for
    /// deterministic replay but not for exact field restoration without
    /// re-running the pipeline.
    SeedReplay,
    /// seed + preset (inline RON) + authoritative.height + authoritative.sediment.
    /// Both fields must be `Some`; Sprint 0 rejects saves where either is `None`.
    Minimal,
    /// Sprint 1B+: adds `baked.*` fields. Not yet supported.
    Full,
    /// Sprint 4+: adds `derived.*` fields + overlay PNGs. Not yet supported.
    DebugCapture,
}

impl SaveMode {
    fn discriminant(self) -> u8 {
        match self {
            SaveMode::SeedReplay => 0,
            SaveMode::Minimal => 1,
            SaveMode::Full => 2,
            SaveMode::DebugCapture => 3,
        }
    }

    fn from_discriminant(d: u8) -> Result<Self, SaveError> {
        match d {
            0 => Ok(Self::SeedReplay),
            1 => Ok(Self::Minimal),
            2 => Ok(Self::Full),
            3 => Ok(Self::DebugCapture),
            other => Err(SaveError::InvalidMode(other)),
        }
    }
}

// ─── SaveHeader ──────────────────────────────────────────────────────────────

/// Fixed 9-byte prefix on every save blob.
pub struct SaveHeader {
    pub magic: [u8; 4],      // b"IPGS"
    pub format_version: u32, // v1 = 1
    pub mode: SaveMode,
}

// ─── LoadedWorld ─────────────────────────────────────────────────────────────

/// Result of [`read_world`]. Variants reflect what data the file actually
/// contained — for `SeedReplay` the caller must resolve the preset by name.
#[derive(Debug)]
pub enum LoadedWorld {
    /// `SeedReplay` file: contains seed, preset name, and resolution only.
    ///
    /// The caller is responsible for calling `data::presets::load_preset`
    /// with `preset_name` to obtain the full [`IslandArchetypePreset`].
    SeedReplay {
        seed: Seed,
        preset_name: String,
        resolution: Resolution,
    },
    /// `Minimal` file: fully reconstructed [`WorldState`] with
    /// `authoritative.height` and `authoritative.sediment` populated.
    ///
    /// Boxed because `WorldState` is large relative to `SeedReplay`.
    Minimal(Box<WorldState>),
}

impl LoadedWorld {
    /// Return the [`SaveMode`] that produced this loaded world.
    pub fn mode(&self) -> SaveMode {
        match self {
            LoadedWorld::SeedReplay { .. } => SaveMode::SeedReplay,
            LoadedWorld::Minimal(_) => SaveMode::Minimal,
        }
    }
}

// ─── SaveError ───────────────────────────────────────────────────────────────

/// All error cases from [`write_world`] and [`read_world`].
#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("bad magic: expected b\"IPGS\", got {0:?}")]
    BadMagic([u8; 4]),

    #[error("unsupported save format version {0}")]
    UnsupportedVersion(u32),

    #[error("invalid save mode discriminant {0}")]
    InvalidMode(u8),

    #[error("save mode {0:?} not yet supported")]
    NotYetSupported(SaveMode),

    #[error("missing authoritative field '{field}' — cannot serialize in Minimal mode")]
    MissingAuthoritativeField { field: &'static str },

    #[error("truncated save: expected {expected} bytes, got {actual}")]
    Truncated { expected: usize, actual: usize },

    #[error("preset RON parse error: {0}")]
    PresetParse(String),

    #[error("preset RON serialize error: {0}")]
    PresetSerialize(String),

    #[error("field decode error: {0}")]
    FieldDecode(#[from] FieldDecodeError),

    #[error("field length mismatch: header dims {dims:?}, body {actual} bytes")]
    FieldLengthMismatch { dims: (u32, u32), actual: usize },
}

// ─── public API ──────────────────────────────────────────────────────────────

/// Serialize `world` into `w` using the given `mode`.
///
/// `Full` and `DebugCapture` return [`SaveError::NotYetSupported`] immediately
/// without writing any bytes.
/// `Minimal` returns [`SaveError::MissingAuthoritativeField`] if either
/// `authoritative.height` or `authoritative.sediment` is `None`.
pub fn write_world<W: Write>(
    world: &WorldState,
    mode: SaveMode,
    w: &mut W,
) -> Result<(), SaveError> {
    // Reject unsupported modes before writing anything
    if matches!(mode, SaveMode::Full | SaveMode::DebugCapture) {
        return Err(SaveError::NotYetSupported(mode));
    }

    // Outer header
    w.write_all(&MAGIC)?;
    w.write_all(&FORMAT_VERSION.to_le_bytes())?;
    w.write_all(&[mode.discriminant()])?;

    // Mode-specific payload
    match mode {
        SaveMode::SeedReplay => write_seed_replay(world, w),
        SaveMode::Minimal => write_minimal(world, w),
        // Already handled above
        SaveMode::Full | SaveMode::DebugCapture => unreachable!(),
    }
}

/// Deserialize a [`WorldState`] (or partial replay info) from `r`.
///
/// Returns `Err` — never panics — on bad magic, unsupported version,
/// unknown mode discriminant, or truncated input (via `read_exact`'s
/// `UnexpectedEof` → [`SaveError::Io`]).
pub fn read_world<R: Read>(r: &mut R) -> Result<LoadedWorld, SaveError> {
    // 1. Magic
    let mut magic = [0u8; 4];
    r.read_exact(&mut magic)?;
    if magic != MAGIC {
        return Err(SaveError::BadMagic(magic));
    }

    // 2. Format version
    let version = read_u32_le(r)?;
    if version != FORMAT_VERSION {
        return Err(SaveError::UnsupportedVersion(version));
    }

    // 3. Mode discriminant
    let mut mode_buf = [0u8; 1];
    r.read_exact(&mut mode_buf)?;
    let mode = SaveMode::from_discriminant(mode_buf[0])?;

    // 4. Mode-specific payload
    match mode {
        SaveMode::SeedReplay => read_seed_replay(r),
        SaveMode::Minimal => read_minimal(r),
        SaveMode::Full | SaveMode::DebugCapture => Err(SaveError::NotYetSupported(mode)),
    }
}

// ─── write helpers ───────────────────────────────────────────────────────────

fn write_seed_replay<W: Write>(world: &WorldState, w: &mut W) -> Result<(), SaveError> {
    w.write_all(&world.seed.0.to_le_bytes())?;
    w.write_all(&world.resolution.sim_width.to_le_bytes())?;
    w.write_all(&world.resolution.sim_height.to_le_bytes())?;
    let name_bytes = world.preset.name.as_bytes();
    w.write_all(&(name_bytes.len() as u32).to_le_bytes())?;
    w.write_all(name_bytes)?;
    Ok(())
}

fn write_minimal<W: Write>(world: &WorldState, w: &mut W) -> Result<(), SaveError> {
    // Validate both fields are present before writing anything
    let height = world
        .authoritative
        .height
        .as_ref()
        .ok_or(SaveError::MissingAuthoritativeField { field: "height" })?;
    let sediment = world
        .authoritative
        .sediment
        .as_ref()
        .ok_or(SaveError::MissingAuthoritativeField { field: "sediment" })?;

    w.write_all(&world.resolution.sim_width.to_le_bytes())?;
    w.write_all(&world.resolution.sim_height.to_le_bytes())?;
    w.write_all(&world.seed.0.to_le_bytes())?;

    let preset_ron =
        ron::to_string(&world.preset).map_err(|e| SaveError::PresetSerialize(e.to_string()))?;
    let preset_bytes = preset_ron.as_bytes();
    w.write_all(&(preset_bytes.len() as u32).to_le_bytes())?;
    w.write_all(preset_bytes)?;

    let h_bytes = height.to_bytes();
    w.write_all(&(h_bytes.len() as u32).to_le_bytes())?;
    w.write_all(&h_bytes)?;

    let s_bytes = sediment.to_bytes();
    w.write_all(&(s_bytes.len() as u32).to_le_bytes())?;
    w.write_all(&s_bytes)?;

    Ok(())
}

// ─── read helpers ────────────────────────────────────────────────────────────

fn read_seed_replay<R: Read>(r: &mut R) -> Result<LoadedWorld, SaveError> {
    let seed_raw = read_u64_le(r)?;
    let sim_width = read_u32_le(r)?;
    let sim_height = read_u32_le(r)?;
    let name_len = read_u32_le(r)? as usize;
    let mut name_bytes = vec![0u8; name_len];
    r.read_exact(&mut name_bytes)?;
    let preset_name =
        String::from_utf8(name_bytes).map_err(|e| SaveError::PresetParse(e.to_string()))?;
    Ok(LoadedWorld::SeedReplay {
        seed: Seed(seed_raw),
        preset_name,
        resolution: Resolution::new(sim_width, sim_height),
    })
}

fn read_minimal<R: Read>(r: &mut R) -> Result<LoadedWorld, SaveError> {
    let sim_width = read_u32_le(r)?;
    let sim_height = read_u32_le(r)?;
    let seed_raw = read_u64_le(r)?;

    let preset_ron_len = read_u32_le(r)? as usize;
    let mut preset_bytes = vec![0u8; preset_ron_len];
    r.read_exact(&mut preset_bytes)?;
    let preset_ron =
        String::from_utf8(preset_bytes).map_err(|e| SaveError::PresetParse(e.to_string()))?;
    let preset: IslandArchetypePreset =
        ron::from_str(&preset_ron).map_err(|e| SaveError::PresetParse(e.to_string()))?;

    let height = read_field_f32(r)?;
    let sediment = read_field_f32(r)?;

    // Use WorldState::new so Sprint 1A+ additions to BakedSnapshot /
    // DerivedCaches with non-trivial defaults don't require an edit here.
    let mut world = WorldState::new(
        Seed(seed_raw),
        preset,
        Resolution::new(sim_width, sim_height),
    );
    world.authoritative.height = Some(height);
    world.authoritative.sediment = Some(sediment);
    Ok(LoadedWorld::Minimal(Box::new(world)))
}

/// Read a length-prefixed `ScalarField2D<f32>` blob: 4-byte LE length, then
/// that many bytes consumed by `ScalarField2D<f32>::from_bytes`.
fn read_field_f32<R: Read>(r: &mut R) -> Result<ScalarField2D<f32>, SaveError> {
    let len = read_u32_le(r)? as usize;
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    let field = ScalarField2D::<f32>::from_bytes(&buf)?;
    Ok(field)
}

// ─── low-level read primitives ────────────────────────────────────────────────

fn read_u32_le<R: Read>(r: &mut R) -> Result<u32, SaveError> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

fn read_u64_le<R: Read>(r: &mut R) -> Result<u64, SaveError> {
    let mut buf = [0u8; 8];
    r.read_exact(&mut buf)?;
    Ok(u64::from_le_bytes(buf))
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::preset::IslandAge;

    // ── shared helpers ────────────────────────────────────────────────────────

    fn make_test_preset() -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "vol_single".into(),
            island_radius: 0.5,
            max_relief: 0.6,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: 0.0,
            marine_moisture_strength: 0.5,
            sea_level: 0.3,
            erosion: Default::default(),
        }
    }

    fn make_small_field(v: f32) -> ScalarField2D<f32> {
        let mut f = ScalarField2D::<f32>::new(4, 4);
        f.data.fill(v);
        f
    }

    fn make_test_world_with_fields() -> WorldState {
        let mut world = WorldState::new(Seed(42), make_test_preset(), Resolution::new(16, 16));
        world.authoritative.height = Some(make_small_field(0.5));
        world.authoritative.sediment = Some(make_small_field(0.1));
        world
    }

    // ── Test 1: SeedReplay round-trip ─────────────────────────────────────────

    #[test]
    fn seed_replay_roundtrip() {
        let world = WorldState::new(Seed(42), make_test_preset(), Resolution::new(256, 256));

        let mut buf = Vec::new();
        write_world(&world, SaveMode::SeedReplay, &mut buf).expect("write failed");

        let mut cursor = Cursor::new(buf);
        let loaded = read_world(&mut cursor).expect("read failed");

        match loaded {
            LoadedWorld::SeedReplay {
                seed,
                preset_name,
                resolution,
            } => {
                assert_eq!(seed, Seed(42));
                assert_eq!(preset_name, "vol_single");
                assert_eq!(resolution, Resolution::new(256, 256));
            }
            _ => panic!("expected SeedReplay variant"),
        }
    }

    // ── Test 2: Minimal round-trip with both fields ───────────────────────────

    #[test]
    fn minimal_roundtrip_with_both_fields() {
        let world = make_test_world_with_fields();

        let mut buf = Vec::new();
        write_world(&world, SaveMode::Minimal, &mut buf).expect("write failed");

        let mut cursor = Cursor::new(buf);
        let loaded = read_world(&mut cursor).expect("read failed");

        match loaded {
            LoadedWorld::Minimal(w2) => {
                assert_eq!(w2.seed, world.seed);
                assert_eq!(w2.preset, world.preset);
                assert_eq!(w2.resolution, world.resolution);
                // ScalarField2D doesn't derive PartialEq; compare via to_bytes()
                assert_eq!(
                    w2.authoritative.height.as_ref().unwrap().to_bytes(),
                    world.authoritative.height.as_ref().unwrap().to_bytes(),
                    "height mismatch"
                );
                assert_eq!(
                    w2.authoritative.sediment.as_ref().unwrap().to_bytes(),
                    world.authoritative.sediment.as_ref().unwrap().to_bytes(),
                    "sediment mismatch"
                );
            }
            _ => panic!("expected Minimal variant"),
        }
    }

    // ── Test 3: Minimal rejects missing height ────────────────────────────────

    #[test]
    fn minimal_rejects_missing_height() {
        let mut world = WorldState::new(Seed(1), make_test_preset(), Resolution::new(8, 8));
        world.authoritative.height = None;
        world.authoritative.sediment = Some(make_small_field(0.2));

        let mut buf = Vec::new();
        let err = write_world(&world, SaveMode::Minimal, &mut buf)
            .expect_err("should fail with missing height");

        match err {
            SaveError::MissingAuthoritativeField { field: "height" } => {}
            other => panic!("expected MissingAuthoritativeField(height), got {other:?}"),
        }
    }

    // ── Test 4: Minimal rejects missing sediment ──────────────────────────────

    #[test]
    fn minimal_rejects_missing_sediment() {
        let mut world = WorldState::new(Seed(2), make_test_preset(), Resolution::new(8, 8));
        world.authoritative.height = Some(make_small_field(0.3));
        world.authoritative.sediment = None;

        let mut buf = Vec::new();
        let err = write_world(&world, SaveMode::Minimal, &mut buf)
            .expect_err("should fail with missing sediment");

        match err {
            SaveError::MissingAuthoritativeField { field: "sediment" } => {}
            other => panic!("expected MissingAuthoritativeField(sediment), got {other:?}"),
        }
    }

    // ── Test 5: read rejects bad magic ────────────────────────────────────────

    #[test]
    fn read_rejects_bad_magic() {
        let world = WorldState::new(Seed(7), make_test_preset(), Resolution::new(8, 8));
        let mut buf = Vec::new();
        write_world(&world, SaveMode::SeedReplay, &mut buf).unwrap();
        buf[0] = b'X'; // corrupt first magic byte

        let mut cursor = Cursor::new(buf);
        let err = read_world(&mut cursor).expect_err("should fail with bad magic");

        assert!(
            matches!(err, SaveError::BadMagic(_)),
            "expected BadMagic, got {err:?}"
        );
    }

    // ── Test 6: read rejects bad format version ───────────────────────────────

    #[test]
    fn read_rejects_bad_version() {
        let world = WorldState::new(Seed(7), make_test_preset(), Resolution::new(8, 8));
        let mut buf = Vec::new();
        write_world(&world, SaveMode::SeedReplay, &mut buf).unwrap();
        // format_version is at bytes 4..8
        buf[4..8].copy_from_slice(&999u32.to_le_bytes());

        let mut cursor = Cursor::new(buf);
        let err = read_world(&mut cursor).expect_err("should fail with bad version");

        assert!(
            matches!(err, SaveError::UnsupportedVersion(999)),
            "expected UnsupportedVersion(999), got {err:?}"
        );
    }

    // ── Test 7: Full returns NotYetSupported ──────────────────────────────────

    #[test]
    fn full_returns_not_yet_supported() {
        let world = WorldState::new(Seed(3), make_test_preset(), Resolution::new(8, 8));
        let mut buf = Vec::new();
        let err = write_world(&world, SaveMode::Full, &mut buf)
            .expect_err("should return NotYetSupported");

        assert!(
            matches!(err, SaveError::NotYetSupported(SaveMode::Full)),
            "expected NotYetSupported(Full), got {err:?}"
        );
    }

    // ── Test 8: DebugCapture returns NotYetSupported ──────────────────────────

    #[test]
    fn debug_capture_returns_not_yet_supported() {
        let world = WorldState::new(Seed(4), make_test_preset(), Resolution::new(8, 8));
        let mut buf = Vec::new();
        let err = write_world(&world, SaveMode::DebugCapture, &mut buf)
            .expect_err("should return NotYetSupported");

        assert!(
            matches!(err, SaveError::NotYetSupported(SaveMode::DebugCapture)),
            "expected NotYetSupported(DebugCapture), got {err:?}"
        );
    }

    // ── Test 9 (bonus): truncated Minimal returns Io(UnexpectedEof) ──────────

    #[test]
    fn read_rejects_truncated_minimal() {
        let world = make_test_world_with_fields();
        let mut buf = Vec::new();
        write_world(&world, SaveMode::Minimal, &mut buf).unwrap();

        // Truncate last 10 bytes
        let new_len = buf.len() - 10;
        buf.truncate(new_len);

        let mut cursor = Cursor::new(buf);
        let err = read_world(&mut cursor).expect_err("should fail on truncated input");

        // read_exact returns io::ErrorKind::UnexpectedEof, which maps to SaveError::Io
        assert!(
            matches!(err, SaveError::Io(_) | SaveError::FieldDecode(_)),
            "expected Io or FieldDecode error, got {err:?}"
        );
    }

    // ── Test 10: read rejects invalid mode discriminant ───────────────────────

    #[test]
    fn read_rejects_invalid_mode_discriminant() {
        let world = WorldState::new(Seed(5), make_test_preset(), Resolution::new(8, 8));
        let mut buf = Vec::new();
        write_world(&world, SaveMode::SeedReplay, &mut buf).unwrap();
        // mode discriminant is at byte 8
        buf[8] = 99;

        let mut cursor = Cursor::new(buf);
        let err = read_world(&mut cursor).expect_err("should fail with invalid discriminant");

        assert!(
            matches!(err, SaveError::InvalidMode(99)),
            "expected InvalidMode(99), got {err:?}"
        );
    }
}
