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
//!   4       4    format_version (little-endian u32); current = 2, legacy = 1
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
//! The SeedReplay payload format did not change between v1 and v2.
//!
//! # Minimal payload — v2 (Sprint 3+, current writer always emits this)
//!
//! ```text
//! offset           bytes  meaning
//!   0                4    resolution.sim_width (u32, LE)
//!   4                4    resolution.sim_height (u32, LE)
//!   8                8    seed.0 (u64, LE)
//!  16                4    preset_ron_len = N (u32, LE)
//!  20                N    preset RON bytes (UTF-8, ron::to_string(&preset))
//!  20+N              1    height_present_flag (0 / 1)
//!  21+N              4    height_bytes_len = M (u32, LE)   — only if flag = 1
//!  25+N              M    ScalarField2D<f32>::to_bytes()   — only if flag = 1
//!  25+N+M            1    sediment_present_flag (0 / 1)
//!  26+N+M            4    sediment_bytes_len = K (u32, LE) — only if flag = 1
//!  30+N+M            K    ScalarField2D<f32>::to_bytes()   — only if flag = 1
//! ```
//!
//! The writer always emits `height_present_flag = 1` for Minimal mode (Minimal
//! by definition requires height), and sets `sediment_present_flag = 1` when
//! `authoritative.sediment` is `Some`, `0` otherwise. Sprint 3 populates
//! sediment in `CoastMaskStage`, so practically both flags are `1` for any
//! post-pipeline world.
//!
//! # Minimal payload — v1 (legacy; read-only migration path)
//!
//! v1 wrote height and sediment back-to-back without any present flags:
//!
//! ```text
//! offset           bytes  meaning
//!   0                4    resolution.sim_width (u32, LE)
//!   4                4    resolution.sim_height (u32, LE)
//!   8                8    seed.0 (u64, LE)
//!  16                4    preset_ron_len = N
//!  20                N    preset RON
//!  20+N              4    height_bytes_len = M
//!  24+N              M    height bytes
//!  24+N+M            4    sediment_bytes_len = K
//!  28+N+M            K    sediment bytes
//! ```
//!
//! # v1 → v2 migration (DD7)
//!
//! `read_world` inspects the header's `format_version` and dispatches to the
//! matching Minimal parser. The loaded `WorldState` is identical in shape no
//! matter which payload format produced it; the bytes are preserved verbatim.
//! A Sprint-0-era save with all-zero sediment will be overwritten on the next
//! pipeline run (Sprint 3's `CoastMaskStage` writes the `0.1 · is_land`
//! initialization), so no explicit re-initialization happens in the codec.
//!
//! v2 → v1 downgrade is **not supported**. `write_world` always emits v2.

use std::io::{Read, Write};

use crate::field::{FieldDecodeError, ScalarField2D};
use crate::preset::IslandArchetypePreset;
use crate::seed::Seed;
use crate::world::{Resolution, WorldState};

// ─── constants ───────────────────────────────────────────────────────────────

const MAGIC: [u8; 4] = *b"IPGS";

/// Sprint 3 DD7: bumped from 1 → 2. The writer always emits this version;
/// [`read_world`] still accepts version 1 via the v1 → v2 migration path.
pub const SAVE_FORMAT_VERSION: u32 = 2;

/// Legacy format version, retained for the Sprint 3 migration test and for
/// `read_world`'s version-sniffing dispatch.
const LEGACY_FORMAT_VERSION_V1: u32 = 1;

// ─── SaveMode ────────────────────────────────────────────────────────────────

/// The save density tier. Sprint 0 implements `SeedReplay` and `Minimal`;
/// `Full` and `DebugCapture` are reserved for Sprint 1B+ and Sprint 4+.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveMode {
    /// ~100 bytes: seed + preset name + resolution. Sufficient for
    /// deterministic replay but not for exact field restoration without
    /// re-running the pipeline.
    SeedReplay,
    /// seed + preset (inline RON) + `authoritative.height` + optional
    /// `authoritative.sediment`. Height is always required; sediment became
    /// optional in the v2 layout (Sprint 3 DD7) — a `None` sediment encodes
    /// to a `0` present-flag byte with no further sediment bytes.
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
    pub format_version: u32, // current writer emits 2; reader accepts 1 via migration
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
    /// `Minimal` file: reconstructed [`WorldState`] with
    /// `authoritative.height` always populated and
    /// `authoritative.sediment` populated only when the save blob declared
    /// it present (v2 present-flag = 1, or any v1 payload since v1 always
    /// wrote sediment).
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

    #[error("invalid present-flag byte: expected 0 or 1, got {0}")]
    InvalidPresentFlag(u8),

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
/// `Minimal` returns [`SaveError::MissingAuthoritativeField`] when
/// `authoritative.height` is `None`. `authoritative.sediment` may be `None`
/// under v2 (Sprint 3) — the writer emits a `0` present flag and no
/// sediment bytes in that case.
///
/// The outer header always declares [`SAVE_FORMAT_VERSION`] (= 2). There is
/// no writer for legacy v1 — `read_world` handles the v1 → v2 migration
/// direction, not the reverse.
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
    w.write_all(&SAVE_FORMAT_VERSION.to_le_bytes())?;
    w.write_all(&[mode.discriminant()])?;

    // Mode-specific payload
    match mode {
        SaveMode::SeedReplay => write_seed_replay(world, w),
        SaveMode::Minimal => write_minimal_v2(world, w),
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

    // 2. Format version: accept either the current v2 or the legacy v1. The
    //    payload parser is dispatched below based on this value.
    let version = read_u32_le(r)?;
    if version != SAVE_FORMAT_VERSION && version != LEGACY_FORMAT_VERSION_V1 {
        return Err(SaveError::UnsupportedVersion(version));
    }

    // 3. Mode discriminant
    let mut mode_buf = [0u8; 1];
    r.read_exact(&mut mode_buf)?;
    let mode = SaveMode::from_discriminant(mode_buf[0])?;

    // 4. Mode-specific payload — SeedReplay is version-invariant; Minimal
    //    splits between legacy v1 (no present flags) and v2 (present flags).
    match mode {
        SaveMode::SeedReplay => read_seed_replay(r),
        SaveMode::Minimal => {
            if version == LEGACY_FORMAT_VERSION_V1 {
                read_minimal_v1(r)
            } else {
                read_minimal_v2(r)
            }
        }
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

/// Write a v2 Minimal payload: seed + preset RON + per-field present flags.
///
/// Height is required (Minimal mode has no meaning without it). Sediment is
/// optional under v2 — `None` emits `sediment_present_flag = 0` and no
/// sediment bytes. Sprint 3+ pipelines always populate sediment, so in
/// practice both flags are `1` for any post-pipeline world.
fn write_minimal_v2<W: Write>(world: &WorldState, w: &mut W) -> Result<(), SaveError> {
    let height = world
        .authoritative
        .height
        .as_ref()
        .ok_or(SaveError::MissingAuthoritativeField { field: "height" })?;

    w.write_all(&world.resolution.sim_width.to_le_bytes())?;
    w.write_all(&world.resolution.sim_height.to_le_bytes())?;
    w.write_all(&world.seed.0.to_le_bytes())?;

    let preset_ron =
        ron::to_string(&world.preset).map_err(|e| SaveError::PresetSerialize(e.to_string()))?;
    let preset_bytes = preset_ron.as_bytes();
    w.write_all(&(preset_bytes.len() as u32).to_le_bytes())?;
    w.write_all(preset_bytes)?;

    // Height is always present in Minimal mode.
    w.write_all(&[1u8])?;
    let h_bytes = height.to_bytes();
    w.write_all(&(h_bytes.len() as u32).to_le_bytes())?;
    w.write_all(&h_bytes)?;

    // Sediment is optional under v2; emit a `0` flag and nothing else if absent.
    match world.authoritative.sediment.as_ref() {
        Some(sediment) => {
            w.write_all(&[1u8])?;
            let s_bytes = sediment.to_bytes();
            w.write_all(&(s_bytes.len() as u32).to_le_bytes())?;
            w.write_all(&s_bytes)?;
        }
        None => {
            w.write_all(&[0u8])?;
        }
    }

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

/// Shared preamble for both v1 and v2 Minimal payloads: resolution + seed +
/// inline preset RON.
fn read_minimal_preamble<R: Read>(
    r: &mut R,
) -> Result<(u32, u32, u64, IslandArchetypePreset), SaveError> {
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

    Ok((sim_width, sim_height, seed_raw, preset))
}

/// Legacy v1 Minimal payload: both `height` and `sediment` appear back-to-back
/// with no present flags. All-zero sediment bytes (Sprint 0-era placeholders)
/// are preserved verbatim — the next pipeline run will overwrite them via
/// `CoastMaskStage`'s `0.1 · is_land` initialization.
fn read_minimal_v1<R: Read>(r: &mut R) -> Result<LoadedWorld, SaveError> {
    let (sim_width, sim_height, seed_raw, preset) = read_minimal_preamble(r)?;

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

/// Current v2 Minimal payload: per-field present flags in front of each
/// optional field.
fn read_minimal_v2<R: Read>(r: &mut R) -> Result<LoadedWorld, SaveError> {
    let (sim_width, sim_height, seed_raw, preset) = read_minimal_preamble(r)?;

    let mut world = WorldState::new(
        Seed(seed_raw),
        preset,
        Resolution::new(sim_width, sim_height),
    );

    // Height present flag
    let mut flag = [0u8; 1];
    r.read_exact(&mut flag)?;
    match flag[0] {
        0 => {
            // Minimal without height is structurally invalid — reject rather
            // than silently producing a world with no elevation data.
            return Err(SaveError::MissingAuthoritativeField { field: "height" });
        }
        1 => {
            world.authoritative.height = Some(read_field_f32(r)?);
        }
        other => return Err(SaveError::InvalidPresentFlag(other)),
    }

    // Sediment present flag
    r.read_exact(&mut flag)?;
    match flag[0] {
        0 => {
            // Sediment legitimately absent (Sprint 3+ writer emits this when
            // `authoritative.sediment` was `None` at save time).
            world.authoritative.sediment = None;
        }
        1 => {
            world.authoritative.sediment = Some(read_field_f32(r)?);
        }
        other => return Err(SaveError::InvalidPresentFlag(other)),
    }

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
            climate: Default::default(),
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

    // ── Test 4: Minimal accepts absent sediment under v2 ──────────────────────
    //
    // Sprint 3 DD7 relaxed Sprint 0's "sediment must be Some" rule: under the
    // v2 layout the sediment present-flag byte is `0` and no sediment bytes
    // follow. `read_world` round-trips this cleanly back to
    // `authoritative.sediment == None`.

    #[test]
    fn minimal_accepts_absent_sediment_under_v2() {
        let mut world = WorldState::new(Seed(2), make_test_preset(), Resolution::new(8, 8));
        world.authoritative.height = Some(make_small_field(0.3));
        world.authoritative.sediment = None;

        let mut buf = Vec::new();
        write_world(&world, SaveMode::Minimal, &mut buf).expect("v2 allows missing sediment");

        let mut cursor = Cursor::new(buf);
        let loaded = read_world(&mut cursor).expect("v2 blob must parse back");
        match loaded {
            LoadedWorld::Minimal(w2) => {
                assert!(
                    w2.authoritative.height.is_some(),
                    "height should be populated"
                );
                assert!(
                    w2.authoritative.sediment.is_none(),
                    "sediment should round-trip as None"
                );
            }
            other => panic!("expected Minimal variant, got {other:?}"),
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

    // ─── Sprint 3 DD7: format_version 1 → 2 migration tests ───────────────────

    /// Hand-rolled v1 Minimal payload matching the pre-Sprint-3 layout:
    /// `[MAGIC | version = 1 | mode = Minimal | preamble | height | sediment]`
    /// (no per-field present flags).
    fn build_v1_minimal_blob(world: &WorldState, sediment: &ScalarField2D<f32>) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&MAGIC);
        buf.extend_from_slice(&LEGACY_FORMAT_VERSION_V1.to_le_bytes());
        buf.push(SaveMode::Minimal.discriminant());

        buf.extend_from_slice(&world.resolution.sim_width.to_le_bytes());
        buf.extend_from_slice(&world.resolution.sim_height.to_le_bytes());
        buf.extend_from_slice(&world.seed.0.to_le_bytes());

        let preset_ron = ron::to_string(&world.preset).expect("preset RON serialises");
        let preset_bytes = preset_ron.as_bytes();
        buf.extend_from_slice(&(preset_bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(preset_bytes);

        let h_bytes = world.authoritative.height.as_ref().unwrap().to_bytes();
        buf.extend_from_slice(&(h_bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(&h_bytes);

        let s_bytes = sediment.to_bytes();
        buf.extend_from_slice(&(s_bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(&s_bytes);

        buf
    }

    #[test]
    fn sprint_3_save_format_version_is_2() {
        assert_eq!(
            SAVE_FORMAT_VERSION, 2,
            "Sprint 3 DD7 locks format version at 2"
        );
    }

    #[test]
    fn write_world_always_emits_v2() {
        let world = make_test_world_with_fields();
        let mut buf = Vec::new();
        write_world(&world, SaveMode::Minimal, &mut buf).expect("write failed");

        // Magic at bytes 0..4 = b"IPGS"
        assert_eq!(&buf[0..4], b"IPGS", "magic bytes");
        // format_version at bytes 4..8 must be 2 (little-endian u32)
        let version = u32::from_le_bytes(buf[4..8].try_into().unwrap());
        assert_eq!(
            version, 2,
            "write_world must always emit format_version = 2"
        );
    }

    #[test]
    fn read_world_accepts_v1_legacy_save() {
        // Sprint-0-era blob: all-zero sediment, since DD7 calls that case out
        // explicitly — the next pipeline run will overwrite via CoastMaskStage.
        let world = make_test_world_with_fields();
        let zero_sediment = make_small_field(0.0);
        let v1_blob = build_v1_minimal_blob(&world, &zero_sediment);

        let mut cursor = Cursor::new(v1_blob);
        let loaded = read_world(&mut cursor).expect("v1 blob must parse under Sprint 3 binary");

        match loaded {
            LoadedWorld::Minimal(w2) => {
                assert_eq!(w2.seed, world.seed);
                assert_eq!(w2.resolution, world.resolution);
                assert_eq!(
                    w2.authoritative.height.as_ref().unwrap().to_bytes(),
                    world.authoritative.height.as_ref().unwrap().to_bytes(),
                    "height preserved across v1 → v2 migration"
                );
                // Sediment bytes preserved as-is (all zeros). Pipeline rerun
                // is what overwrites these with the 0.1·is_land pattern.
                let sediment = w2
                    .authoritative
                    .sediment
                    .as_ref()
                    .expect("v1 always wrote sediment, migration preserves it");
                assert!(
                    sediment.data.iter().all(|&v| v == 0.0),
                    "all-zero sediment bytes preserved by the migration path"
                );
            }
            other => panic!("expected Minimal variant, got {other:?}"),
        }
    }

    #[test]
    fn read_world_accepts_v1_legacy_save_with_non_zero_sediment() {
        // DD7: a Sprint 3+ writer would never emit v1, but if one somehow did
        // (user on a mixed-version binary), the non-zero sediment bytes are
        // preserved verbatim — not re-initialised.
        let world = make_test_world_with_fields();
        let live_sediment = make_small_field(0.42);
        let v1_blob = build_v1_minimal_blob(&world, &live_sediment);

        let mut cursor = Cursor::new(v1_blob);
        let loaded = read_world(&mut cursor).expect("v1 blob must parse");
        match loaded {
            LoadedWorld::Minimal(w2) => {
                let sediment = w2
                    .authoritative
                    .sediment
                    .as_ref()
                    .expect("sediment preserved");
                assert!(
                    sediment.data.iter().all(|&v| (v - 0.42).abs() < 1e-6),
                    "non-zero v1 sediment preserved verbatim"
                );
            }
            other => panic!("expected Minimal variant, got {other:?}"),
        }
    }

    #[test]
    fn read_world_accepts_v2_new_save() {
        // Round-trip through the current writer — same assertions as the
        // existing `minimal_roundtrip_with_both_fields` but explicit about the
        // v2 contract and binding SAVE_FORMAT_VERSION.
        let world = make_test_world_with_fields();
        let mut buf = Vec::new();
        write_world(&world, SaveMode::Minimal, &mut buf).expect("v2 write");

        let version = u32::from_le_bytes(buf[4..8].try_into().unwrap());
        assert_eq!(version, SAVE_FORMAT_VERSION);

        let mut cursor = Cursor::new(buf);
        let loaded = read_world(&mut cursor).expect("v2 blob must round-trip");
        match loaded {
            LoadedWorld::Minimal(w2) => {
                assert_eq!(
                    w2.authoritative.height.as_ref().unwrap().to_bytes(),
                    world.authoritative.height.as_ref().unwrap().to_bytes(),
                );
                assert_eq!(
                    w2.authoritative.sediment.as_ref().unwrap().to_bytes(),
                    world.authoritative.sediment.as_ref().unwrap().to_bytes(),
                );
            }
            other => panic!("expected Minimal variant, got {other:?}"),
        }
    }

    #[test]
    fn read_world_rejects_unsupported_version() {
        // v0 doesn't exist as a real format, but any value outside {1, 2} is
        // rejected. This test locks the migration path to the two known
        // versions rather than silently accepting arbitrary blobs.
        let world = make_test_world_with_fields();
        let mut buf = Vec::new();
        write_world(&world, SaveMode::Minimal, &mut buf).unwrap();
        buf[4..8].copy_from_slice(&0u32.to_le_bytes());

        let mut cursor = Cursor::new(buf);
        let err = read_world(&mut cursor).expect_err("version 0 must be rejected");
        assert!(
            matches!(err, SaveError::UnsupportedVersion(0)),
            "expected UnsupportedVersion(0), got {err:?}"
        );
    }

    #[test]
    fn read_v2_rejects_invalid_height_present_flag() {
        // Flip the height-present-flag byte to 99 after a clean write and
        // confirm the codec rejects it rather than silently misaligning the
        // remaining payload.
        let world = make_test_world_with_fields();
        let mut buf = Vec::new();
        write_world(&world, SaveMode::Minimal, &mut buf).unwrap();

        // Locate the height-present-flag: after [magic(4), version(4),
        // mode(1), sim_width(4), sim_height(4), seed(8), preset_len(4),
        // preset_bytes(N)]. Read preset_len from buf.
        let preset_len_offset = 4 + 4 + 1 + 4 + 4 + 8;
        let preset_len = u32::from_le_bytes(
            buf[preset_len_offset..preset_len_offset + 4]
                .try_into()
                .unwrap(),
        ) as usize;
        let flag_offset = preset_len_offset + 4 + preset_len;
        buf[flag_offset] = 99;

        let mut cursor = Cursor::new(buf);
        let err = read_world(&mut cursor).expect_err("invalid flag must be rejected");
        assert!(
            matches!(err, SaveError::InvalidPresentFlag(99)),
            "expected InvalidPresentFlag(99), got {err:?}"
        );
    }
}
