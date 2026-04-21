//! Colour palette definitions for overlay rendering and CPU batch export.
//!
//! [`sample`] maps a normalised `t ∈ [0, 1]` value to an RGBA byte quad
//! using the chosen [`PaletteId`]. [`sample_f32`] returns the same mapping
//! as `[f32; 4]` for shader / GPU pathways.

// ─── Private helpers ─────────────────────────────────────────────────────────

pub const fn hex_rgba(rgb: u32) -> [f32; 4] {
    let r = ((rgb >> 16) & 0xFF) as f32 / 255.0;
    let g = ((rgb >> 8) & 0xFF) as f32 / 255.0;
    let b = (rgb & 0xFF) as f32 / 255.0;
    [r, g, b, 1.0]
}

fn lerp_f32(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}

// ─── Canonical 8-color constants (§3.2 A1) ──────────────────────────────────
//
// Values are locked against `assets/visual/palette_reference.jpg` — each hex
// came from pixel-sampling the centre of its swatch with ΔE < 6 tolerance in
// sRGB. Changing any of these requires re-sampling the reference first.

pub const DEEP_WATER: [f32; 4] = hex_rgba(0x1C416B);
pub const SHALLOW_WATER: [f32; 4] = hex_rgba(0x599394);
pub const LOWLAND: [f32; 4] = hex_rgba(0x2E5A37);
pub const MIDLAND: [f32; 4] = hex_rgba(0x6C754A);
pub const HIGHLAND: [f32; 4] = hex_rgba(0x9E9C8F);
pub const RIVER: [f32; 4] = hex_rgba(0x80BADF);
pub const BASIN_ACCENT: [f32; 4] = hex_rgba(0x596595);
pub const OVERLAY_NEUTRAL: [f32; 4] = hex_rgba(0x88888A);

// ─── §3.2 A3 Sky gradient helper constants (non-canonical) ─────────────────
//
// NOT part of the canonical 8 and NOT pixel-locked — `palette_reference.jpg`
// has no sky panel. Hand-tuned to stay distinct from the water family.

/// Bottom-of-screen horizon colour for the procedural sky gradient.
pub const SKY_HORIZON: [f32; 4] = hex_rgba(0xB8C8D4);

/// Top-of-screen zenith colour for the procedural sky gradient.
pub const SKY_ZENITH: [f32; 4] = hex_rgba(0x1C2C44);

// ─── PaletteId ───────────────────────────────────────────────────────────────

// ─── Sprint 2 / Sprint 3 CoastType palette ───────────────────────────────────
//
// NOT pixel-sampled from `palette_reference.jpg` — these are Sprint 2/3
// colours with no reference panel. Marked "not pixel-sampled".
// Index mapping: Cliff=0, Beach=1, Estuary=2, RockyHeadland=3, LavaDelta=4.
// Out-of-range indices (including 0xFF for Unknown) → transparent [0.0; 4].

/// Sprint 2 v1 + Sprint 3 DD6 coastal geomorphology palette — not pixel-sampled.
///
/// RGBA sRGB colours for the five [`CoastType`] variants (index = discriminant):
/// * `0` Cliff: dark warm grey
/// * `1` Beach: sand
/// * `2` Estuary: teal
/// * `3` RockyHeadland: medium grey
/// * `4` LavaDelta: deep reddish-black (fresh volcanic rock)
///
/// LavaDelta is a Sprint 3 addition and has no entry in
/// `assets/visual/palette_reference.jpg`; the colour was chosen to
/// read as "fresh basalt" against the Beach / RockyHeadland neighbours.
/// Future Sprint 4.5 may add a palette-reference swatch and re-lock the
/// value via a `canonical_constants_match_palette_reference`-style test.
///
/// [`CoastType`]: island_core::world::CoastType
pub const COAST_TYPE_TABLE: [[f32; 4]; 5] = [
    [0.35, 0.32, 0.28, 1.0], // Cliff: dark warm grey
    [0.90, 0.85, 0.70, 1.0], // Beach: sand
    [0.30, 0.65, 0.70, 1.0], // Estuary: teal
    [0.55, 0.50, 0.45, 1.0], // RockyHeadland: medium grey
    [0.25, 0.05, 0.03, 1.0], // LavaDelta: deep reddish-black (fresh volcanic rock)
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteId {
    Grayscale,
    Viridis,
    Turbo,
    Categorical,
    /// Sprint 1A height ramp: Lowland → Midland → Highland over `t ∈ [0, 1]`.
    TerrainHeight,
    /// Sprint 1A binary-blue mask: transparent for `t < 0.5`, RIVER colour for `t ≥ 0.5`.
    BinaryBlue,
    /// Sprint 2 coastal geomorphology categorical: index `0..=3` → COAST_TYPE_TABLE;
    /// index `0xFF` (Unknown sentinel) and any other out-of-range value → transparent.
    CoastType,
    /// Sprint 3 fog / cloud-forest water input palette: a single-hue blue ramp
    /// from near-white (low fog contribution) to deep blue (high fog contribution).
    /// `t = 0.0` → pale blue-white; `t = 1.0` → deep saturated blue.
    Blues,
    /// Sprint 3 Task 3.7 LavaDelta mask palette.
    ///
    /// Used with `ValueRange::Fixed(0.0, 5.0)` on the `coast_type` field.
    /// Only discriminant 4 (LavaDelta) is rendered — all other discriminants
    /// produce transparent pixels, effectively masking out every coast type
    /// except LavaDelta so users can inspect its spatial distribution in
    /// isolation.
    ///
    /// * `t = 4.0 / 5.0` (discriminant 4, LavaDelta) → `COAST_TYPE_TABLE[4]`, opaque.
    /// * All other `t` values → `[0.0, 0.0, 0.0, 0.0]`, transparent.
    LavaDeltaMask,
}

// ─── Convenience converters ──────────────────────────────────────────────────

pub fn rgba_to_u8(c: [f32; 4]) -> [u8; 4] {
    [
        (c[0].clamp(0.0, 1.0) * 255.0) as u8,
        (c[1].clamp(0.0, 1.0) * 255.0) as u8,
        (c[2].clamp(0.0, 1.0) * 255.0) as u8,
        (c[3].clamp(0.0, 1.0) * 255.0) as u8,
    ]
}

// ─── Lookup tables ───────────────────────────────────────────────────────────

// Matplotlib Viridis 256-entry LUT (BSD licensed).
// Source: matplotlib/_cm_listed.py _viridis_data, converted to u8 via round(x*255).
// Endpoints: [0]=[68,1,84], [255]=[253,242,0].
#[rustfmt::skip]
const VIRIDIS_LUT: [[u8; 3]; 256] = [
    [68,1,84],[68,2,86],[69,4,87],[69,5,89],[70,7,90],[70,8,92],[70,10,93],
    [70,11,94],[71,13,96],[71,14,97],[71,16,99],[71,17,100],[71,19,101],[72,20,103],
    [72,22,104],[72,23,105],[72,24,106],[72,26,108],[72,27,109],[72,28,110],[72,29,111],
    [72,31,112],[72,32,113],[72,33,115],[72,35,116],[72,36,117],[72,37,118],[72,38,119],
    [72,40,120],[72,41,121],[71,42,122],[71,44,122],[71,45,123],[71,46,124],[71,47,125],
    [70,48,126],[70,50,126],[70,51,127],[70,52,128],[69,53,129],[69,55,129],[69,56,130],
    [68,57,131],[68,58,131],[68,59,132],[67,61,132],[67,62,133],[66,63,133],[66,64,134],
    [66,65,134],[65,66,135],[65,68,135],[64,69,136],[64,70,136],[63,71,136],[63,72,137],
    [62,73,137],[62,74,137],[62,76,138],[61,77,138],[61,78,138],[60,79,138],[60,80,139],
    [59,81,139],[59,82,139],[58,83,139],[58,84,140],[57,85,140],[57,86,140],[56,88,140],
    [56,89,140],[55,90,140],[55,91,141],[54,92,141],[54,93,141],[53,94,141],[53,95,141],
    [52,96,141],[52,97,141],[51,98,141],[51,99,141],[50,100,142],[50,101,142],[49,102,142],
    [49,103,142],[49,104,142],[48,105,142],[48,106,142],[47,107,142],[47,108,142],[46,109,142],
    [46,110,142],[46,111,142],[45,112,142],[45,113,142],[44,113,142],[44,114,142],[44,115,142],
    [43,116,142],[43,117,142],[42,118,142],[42,119,142],[42,120,142],[41,121,142],[41,122,142],
    [41,123,142],[40,124,142],[40,125,142],[39,126,142],[39,127,142],[39,128,142],[38,129,142],
    [38,130,142],[38,130,142],[37,131,142],[37,132,142],[37,133,142],[36,134,142],[36,135,142],
    [35,136,142],[35,137,142],[35,138,141],[34,139,141],[34,140,141],[34,141,141],[33,142,141],
    [33,143,141],[33,144,141],[33,145,140],[32,146,140],[32,146,140],[32,147,140],[31,148,140],
    [31,149,139],[31,150,139],[31,151,139],[31,152,139],[31,153,138],[31,154,138],[30,155,138],
    [30,156,137],[30,157,137],[31,158,137],[31,159,136],[31,160,136],[31,161,136],[31,161,135],
    [31,162,135],[32,163,134],[32,164,134],[33,165,133],[33,166,133],[34,167,133],[34,168,132],
    [35,169,131],[36,170,131],[37,171,130],[37,172,130],[38,173,129],[39,173,129],[40,174,128],
    [41,175,127],[42,176,127],[44,177,126],[45,178,125],[46,179,124],[47,180,124],[49,181,123],
    [50,182,122],[52,182,121],[53,183,121],[55,184,120],[56,185,119],[58,186,118],[59,187,117],
    [61,188,116],[63,189,115],[64,189,114],[66,190,113],[68,191,112],[70,192,111],[72,193,110],
    [74,194,109],[76,195,108],[78,195,107],[80,196,106],[82,197,105],[84,198,104],[86,199,103],
    [88,200,101],[90,200,100],[92,201,99],[94,202,98],[96,203,96],[99,204,95],[101,204,94],
    [103,205,92],[105,206,91],[108,207,90],[110,208,88],[112,208,87],[115,209,86],[117,210,84],
    [119,211,83],[122,211,81],[124,212,80],[127,213,78],[129,214,77],[132,214,75],[134,215,73],
    [137,216,72],[139,217,70],[142,217,69],[144,218,67],[147,219,65],[149,219,64],[152,220,62],
    [155,221,60],[157,221,58],[160,222,57],[162,223,55],[165,223,53],[168,224,51],[170,225,50],
    [173,225,48],[176,226,46],[178,227,44],[181,227,42],[184,228,40],[186,229,38],[189,229,36],
    [192,230,34],[194,230,32],[197,231,30],[200,231,28],[202,232,26],[205,233,24],[208,233,22],
    [210,234,20],[213,234,18],[216,235,16],[218,235,14],[221,236,12],[223,236,10],[226,237,8],
    [229,237,5],[231,238,3],[234,238,1],[236,239,0],[239,239,0],[241,240,0],[244,240,0],
    [246,240,0],[248,241,0],[251,241,0],[253,242,0],
];

// Google Turbo 256-entry LUT (Apache-2 license, Google LLC).
// Generated via 6th-order polynomial from:
// https://gist.github.com/mikhailov-work/0d177465a8151eb6ede1768d51d476c7
// Endpoints: [0]=[35,23,255], [255]=[138,0,0].
#[rustfmt::skip]
const TURBO_LUT: [[u8; 3]; 256] = [
    [35,23,255],[39,26,255],[43,28,254],[47,30,249],[50,32,244],[54,35,240],[57,37,237],
    [59,39,234],[62,42,231],[64,44,229],[66,47,227],[68,49,226],[69,52,225],[70,54,224],
    [71,57,223],[72,60,223],[73,62,223],[74,65,223],[74,68,224],[74,70,224],[74,73,225],
    [74,76,225],[74,78,226],[74,81,227],[74,84,228],[73,86,230],[73,89,231],[72,92,232],
    [71,95,233],[71,97,235],[70,100,236],[69,103,237],[68,106,238],[67,108,240],[66,111,241],
    [65,114,242],[63,117,243],[62,119,244],[61,122,245],[60,125,246],[59,127,247],[57,130,247],
    [56,133,248],[55,136,248],[54,138,249],[53,141,249],[51,143,249],[50,146,249],[49,149,249],
    [48,151,248],[47,154,248],[46,156,247],[45,159,247],[44,161,246],[43,164,245],[42,166,244],
    [41,169,242],[40,171,241],[40,174,239],[39,176,237],[38,178,235],[38,181,233],[37,183,231],
    [37,185,229],[37,187,226],[36,190,224],[36,192,221],[36,194,218],[36,196,215],[36,198,211],
    [36,200,208],[36,202,205],[36,204,201],[36,206,197],[37,208,193],[37,210,189],[37,211,185],
    [38,213,181],[39,215,176],[39,217,172],[40,218,167],[41,220,162],[42,221,157],[43,223,152],
    [44,224,147],[45,226,142],[46,227,137],[48,228,131],[49,230,126],[51,231,120],[52,232,115],
    [54,233,109],[55,234,103],[57,235,98],[59,236,92],[61,237,86],[63,238,80],[65,239,74],
    [67,240,68],[69,241,62],[71,241,55],[73,242,49],[76,242,43],[78,243,37],[80,243,31],
    [83,244,24],[85,244,18],[88,244,12],[91,245,5],[93,245,0],[96,245,0],[99,245,0],
    [101,245,0],[104,245,0],[107,245,0],[110,244,0],[113,244,0],[116,244,0],[119,243,0],
    [122,243,0],[125,242,0],[128,241,0],[131,241,0],[134,240,0],[137,239,0],[140,238,0],
    [143,237,0],[146,236,0],[149,235,0],[152,234,0],[155,232,0],[158,231,0],[161,229,0],
    [164,228,0],[168,226,0],[171,224,0],[174,222,0],[177,220,0],[180,218,0],[183,216,0],
    [186,214,0],[188,212,0],[191,209,0],[194,207,0],[197,204,0],[200,201,0],[202,198,0],
    [205,195,0],[208,192,0],[210,189,0],[213,186,0],[215,182,0],[218,179,0],[220,175,0],
    [223,171,0],[225,167,0],[227,163,0],[229,159,0],[231,155,0],[234,150,0],[236,146,0],
    [237,141,0],[239,136,0],[241,131,0],[243,126,0],[244,120,0],[246,115,0],[247,109,0],
    [249,103,0],[250,97,0],[251,91,0],[252,85,0],[253,78,0],[254,72,0],[255,65,0],
    [255,58,0],[255,50,0],[255,43,0],[255,35,0],[255,27,0],[255,19,0],[255,11,0],
    [255,2,0],[255,0,0],[255,0,0],[255,0,0],[255,0,0],[255,0,0],[255,0,0],
    [255,0,0],[255,0,0],[255,0,0],[255,0,0],[255,0,0],[254,0,0],[253,0,0],
    [252,0,0],[250,0,0],[249,0,0],[248,0,0],[246,0,0],[245,0,0],[243,0,0],
    [242,0,0],[240,0,0],[238,0,0],[236,0,0],[234,0,0],[232,0,0],[230,0,0],
    [228,0,0],[226,0,0],[223,0,0],[221,0,0],[219,0,0],[216,0,0],[214,0,0],
    [211,0,0],[209,0,0],[206,0,0],[204,0,0],[201,0,0],[198,0,0],[196,0,0],
    [193,0,0],[190,0,0],[188,0,0],[185,0,0],[182,0,0],[180,0,0],[177,0,0],
    [175,0,0],[172,0,0],[169,0,0],[167,0,0],[165,0,0],[162,0,0],[160,0,0],
    [158,0,0],[155,0,0],[153,0,0],[151,0,0],[150,0,0],[148,0,0],[146,0,0],
    [145,0,0],[143,0,0],[142,0,0],[141,0,0],[140,0,0],[139,0,0],[138,0,0],
    [138,0,0],[138,0,0],[138,0,0],[138,0,0],
];

// Fixed 16-entry muted blue-purple table for Categorical (hue offsets around BASIN_ACCENT).
// Range: #4A5080–#7A85AA with varying lightness and hue.
#[rustfmt::skip]
const CATEGORICAL_LUT: [[u8; 3]; 16] = [
    [89,101,149],  // BASIN_ACCENT base
    [74, 96,128],  // darker slate
    [122,130,170], // lighter lavender
    [60, 85,115],  // deep blue-grey
    [105,120,160], // mid periwinkle
    [80, 75,130],  // violet lean
    [110,145,158], // teal-steel
    [70,110,140],  // muted cyan-blue
    [95, 90,155],  // indigo-violet
    [130,125,175], // pale mauve
    [55, 80,120],  // navy
    [115,140,148], // desaturated teal
    [85,100,165],  // soft cobalt
    [100,80,140],  // dusty purple
    [75,120,145],  // steel blue
    [120,115,145], // lavender-grey
];

// ─── sample_f32 ──────────────────────────────────────────────────────────────

pub fn sample_f32(palette: PaletteId, t: f32) -> [f32; 4] {
    let t = t.clamp(0.0, 1.0);
    match palette {
        PaletteId::Grayscale => [t, t, t, 1.0],

        PaletteId::Viridis => {
            let idx = ((t * 255.0) as usize).min(255);
            let [r, g, b] = VIRIDIS_LUT[idx];
            [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
        }

        PaletteId::Turbo => {
            let idx = ((t * 255.0) as usize).min(255);
            let [r, g, b] = TURBO_LUT[idx];
            [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
        }

        PaletteId::Categorical => {
            let id = ((t * 15.0) as usize).min(15);
            let [r, g, b] = CATEGORICAL_LUT[id];
            [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
        }

        PaletteId::TerrainHeight => {
            // Two-segment lerp LOWLAND → MIDLAND → HIGHLAND with the midpoint
            // at t = 0.5. Reaches HIGHLAND exactly at t = 1.0 (no plateau).
            let t = t.clamp(0.0, 1.0);
            if t < 0.5 {
                lerp_f32(LOWLAND, MIDLAND, t / 0.5)
            } else {
                lerp_f32(MIDLAND, HIGHLAND, (t - 0.5) / 0.5)
            }
        }

        PaletteId::BinaryBlue => {
            if t >= 0.5 {
                RIVER
            } else {
                [0.0, 0.0, 0.0, 0.0]
            }
        }

        // `CoastType` is a 5-entry categorical palette (0=Cliff, 1=Beach,
        // 2=Estuary, 3=RockyHeadland, 4=LavaDelta) plus an out-of-range
        // transparent sentinel for Unknown (0xFF). The paired overlay
        // descriptor uses `ValueRange::Fixed(0.0, 5.0)` so
        // `t = disc / 5` → `idx = disc` exactly for discriminants 0..=4;
        // 0xFF clamps to `t = 1.0` → `idx = 5` → transparent. Sprint 3
        // DD6 widened this from 4 entries to 5.
        PaletteId::CoastType => {
            let idx = (t * 5.0) as usize;
            if idx < 5 {
                COAST_TYPE_TABLE[idx]
            } else {
                [0.0, 0.0, 0.0, 0.0] // transparent for Unknown and out-of-range
            }
        }

        // Single-hue blue ramp: near-white at `t = 0` to deep saturated blue at
        // `t = 1`. Chosen to be perceptually distinct from the Viridis and Turbo
        // palettes while clearly encoding "water / moisture" semantics. Not
        // pixel-locked to `palette_reference.jpg`; the ramp is procedural.
        PaletteId::Blues => {
            // Endpoints: pale blue-white (low) → deep blue (high).
            // lo = [0.87, 0.92, 0.97, 1.0]  (near-white steel-blue tint)
            // hi = [0.03, 0.19, 0.42, 1.0]  (deep navy blue)
            const BLUES_LO: [f32; 4] = [0.87, 0.92, 0.97, 1.0];
            const BLUES_HI: [f32; 4] = [0.03, 0.19, 0.42, 1.0];
            lerp_f32(BLUES_LO, BLUES_HI, t)
        }

        // LavaDelta mask: renders only discriminant 4 (LavaDelta) opaque;
        // all other discriminants (0..=3 and sentinel 5+) are transparent.
        // Paired with `ValueRange::Fixed(0.0, 5.0)` so `t = disc / 5`:
        //   disc 4 → t = 0.8 → idx = (0.8 * 5.0) as usize = 4 → LavaDelta.
        //   disc 0..=3 → idx 0..=3 → transparent.
        //   disc 5+ (0xFF sentinel clamped to 1.0) → idx ≥ 5 → transparent.
        PaletteId::LavaDeltaMask => {
            let idx = (t * 5.0) as usize;
            if idx == 4 {
                COAST_TYPE_TABLE[4] // LavaDelta colour, opaque
            } else {
                [0.0, 0.0, 0.0, 0.0] // transparent for all non-LavaDelta discriminants
            }
        }
    }
}

// ─── sample ──────────────────────────────────────────────────────────────────

pub fn sample(palette: PaletteId, t: f32) -> [u8; 4] {
    rgba_to_u8(sample_f32(palette, t))
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Sprint 0 Grayscale ───────────────────────────────────────────────────

    #[test]
    fn grayscale_black_at_zero() {
        assert_eq!(sample(PaletteId::Grayscale, 0.0), [0, 0, 0, 255]);
    }

    #[test]
    fn grayscale_white_at_one() {
        assert_eq!(sample(PaletteId::Grayscale, 1.0), [255, 255, 255, 255]);
    }

    #[test]
    fn grayscale_clamps_below_zero() {
        assert_eq!(sample(PaletteId::Grayscale, -1.0), [0, 0, 0, 255]);
    }

    #[test]
    fn grayscale_clamps_above_one() {
        assert_eq!(sample(PaletteId::Grayscale, 2.0), [255, 255, 255, 255]);
    }

    // ── Canonical constants ───────────────────────────────────────────────────

    #[test]
    fn canonical_constants_have_opaque_alpha() {
        let colors = [
            DEEP_WATER,
            SHALLOW_WATER,
            LOWLAND,
            MIDLAND,
            HIGHLAND,
            RIVER,
            BASIN_ACCENT,
            OVERLAY_NEUTRAL,
        ];
        for c in &colors {
            assert_eq!(c[3], 1.0, "expected alpha == 1.0, got {:?}", c);
        }
    }

    #[test]
    fn canonical_constants_in_unit_range() {
        let colors = [
            DEEP_WATER,
            SHALLOW_WATER,
            LOWLAND,
            MIDLAND,
            HIGHLAND,
            RIVER,
            BASIN_ACCENT,
            OVERLAY_NEUTRAL,
        ];
        for c in &colors {
            for &ch in c {
                assert!((0.0..=1.0).contains(&ch), "channel out of range: {:?}", c);
            }
        }
    }

    #[test]
    fn hex_rgba_parses_known_value() {
        // Generic hex parser check (not a canonical palette color).
        let c = hex_rgba(0x24466B);
        let eps = 1e-2_f32;
        assert!((c[0] - 0.141).abs() < eps, "R: {}", c[0]);
        assert!((c[1] - 0.275).abs() < eps, "G: {}", c[1]);
        assert!((c[2] - 0.420).abs() < eps, "B: {}", c[2]);
        assert_eq!(c[3], 1.0);
    }

    // Lock the canonical constants against pixel-sampled values from
    // assets/visual/palette_reference.jpg (ΔE < 6 in sRGB).
    #[test]
    fn canonical_constants_match_palette_reference() {
        let checks: [(&str, [f32; 4], [u8; 3]); 8] = [
            ("DEEP_WATER", DEEP_WATER, [0x1C, 0x41, 0x6B]),
            ("SHALLOW_WATER", SHALLOW_WATER, [0x59, 0x93, 0x94]),
            ("LOWLAND", LOWLAND, [0x2E, 0x5A, 0x37]),
            ("MIDLAND", MIDLAND, [0x6C, 0x75, 0x4A]),
            ("HIGHLAND", HIGHLAND, [0x9E, 0x9C, 0x8F]),
            ("RIVER", RIVER, [0x80, 0xBA, 0xDF]),
            ("BASIN_ACCENT", BASIN_ACCENT, [0x59, 0x65, 0x95]),
            ("OVERLAY_NEUTRAL", OVERLAY_NEUTRAL, [0x88, 0x88, 0x8A]),
        ];
        for (name, rgba, expected) in checks {
            let got = rgba_to_u8(rgba);
            assert_eq!(
                [got[0], got[1], got[2]],
                expected,
                "{name} drifted from palette_reference.jpg sample"
            );
        }
    }

    // ── Viridis ───────────────────────────────────────────────────────────────

    #[test]
    fn viridis_endpoints() {
        let lo = sample(PaletteId::Viridis, 0.0);
        let hi = sample(PaletteId::Viridis, 1.0);
        // VIRIDIS_LUT[0]=[68,1,84], VIRIDIS_LUT[255]=[253,242,0]
        assert_eq!(lo, [68, 1, 84, 255]);
        assert_eq!(hi, [253, 242, 0, 255]);
    }

    #[test]
    fn viridis_monotone_lightness() {
        // Green channel rises monotonically across Viridis (blue falls in the upper half as
        // the colormap transitions to yellow-green, so green is the reliable monotone channel).
        let g_lo = sample_f32(PaletteId::Viridis, 0.2)[1];
        let g_hi = sample_f32(PaletteId::Viridis, 0.8)[1];
        assert!(g_lo < g_hi, "expected G({}) < G({})", g_lo, g_hi);
    }

    // ── Turbo ─────────────────────────────────────────────────────────────────

    #[test]
    fn turbo_endpoints() {
        let turbo_lo = sample(PaletteId::Turbo, 0.0);
        let turbo_hi = sample(PaletteId::Turbo, 1.0);
        let viridis_lo = sample(PaletteId::Viridis, 0.0);
        let viridis_hi = sample(PaletteId::Viridis, 1.0);
        // Turbo and Viridis start from different colours.
        assert_ne!(turbo_lo, viridis_lo);
        // Turbo lo and hi are distinct.
        assert_ne!(turbo_lo, turbo_hi);
        // Turbo hi and Viridis hi are distinct.
        assert_ne!(turbo_hi, viridis_hi);
    }

    // ── Categorical ───────────────────────────────────────────────────────────

    #[test]
    fn categorical_distinct_ids() {
        let c0 = sample(PaletteId::Categorical, 0.0);
        let c1 = sample(PaletteId::Categorical, 0.5);
        let c2 = sample(PaletteId::Categorical, 1.0);
        assert_ne!(c0, c1);
        assert_ne!(c1, c2);
        // Determinism: same t → same color.
        assert_eq!(c0, sample(PaletteId::Categorical, 0.0));
        assert_eq!(c1, sample(PaletteId::Categorical, 0.5));
    }

    // ── TerrainHeight ─────────────────────────────────────────────────────────

    #[test]
    fn terrain_height_ramps_through_stops() {
        let lo = sample_f32(PaletteId::TerrainHeight, 0.0);
        let hi = sample_f32(PaletteId::TerrainHeight, 1.0);
        let mid = sample_f32(PaletteId::TerrainHeight, 0.5);
        let eps = 1e-3_f32;
        // t=0 → LOWLAND, t=0.5 → MIDLAND, t=1 → HIGHLAND (exact endpoints)
        for i in 0..4 {
            assert!(
                (lo[i] - LOWLAND[i]).abs() < eps,
                "lo[{i}]: {} vs {}",
                lo[i],
                LOWLAND[i]
            );
            assert!(
                (mid[i] - MIDLAND[i]).abs() < eps,
                "mid[{i}]: {} vs {}",
                mid[i],
                MIDLAND[i]
            );
            assert!(
                (hi[i] - HIGHLAND[i]).abs() < eps,
                "hi[{i}]: {} vs {}",
                hi[i],
                HIGHLAND[i]
            );
        }
    }

    // Guard against the "HIGHLAND plateau" bug: the upper half must lerp
    // continuously toward HIGHLAND and reach it only at t = 1.0.
    #[test]
    fn terrain_height_upper_half_is_continuous() {
        let a = sample_f32(PaletteId::TerrainHeight, 0.70);
        let b = sample_f32(PaletteId::TerrainHeight, 0.90);
        // Both should be strictly between MIDLAND and HIGHLAND (no plateau).
        for i in 0..3 {
            let midland = MIDLAND[i];
            let highland = HIGHLAND[i];
            let (lo, hi) = if midland < highland {
                (midland, highland)
            } else {
                (highland, midland)
            };
            assert!(a[i] >= lo - 1e-4 && a[i] <= hi + 1e-4, "0.70 channel {i}");
            assert!(b[i] >= lo - 1e-4 && b[i] <= hi + 1e-4, "0.90 channel {i}");
        }
        // And b is strictly closer to HIGHLAND than a is.
        let da: f32 = (0..3).map(|i| (a[i] - HIGHLAND[i]).abs()).sum();
        let db: f32 = (0..3).map(|i| (b[i] - HIGHLAND[i]).abs()).sum();
        assert!(
            db < da,
            "t=0.9 must be closer to HIGHLAND than t=0.7 ({db} < {da})"
        );
    }

    // ── BinaryBlue ────────────────────────────────────────────────────────────

    #[test]
    fn binary_blue_thresholds() {
        let below = sample(PaletteId::BinaryBlue, 0.49);
        let above = sample(PaletteId::BinaryBlue, 0.51);
        // Below threshold → transparent
        assert_eq!(below[3], 0, "expected transparent alpha below threshold");
        // Above threshold → RIVER color, opaque
        let river_u8 = rgba_to_u8(RIVER);
        assert_eq!(above, river_u8);
        assert_eq!(above[3], 255);
    }

    // ── LUT palettes return opaque colours ───────────────────────────────────

    #[test]
    fn lut_palettes_return_opaque_alpha() {
        assert_eq!(sample(PaletteId::Viridis, 0.5)[3], 255);
        assert_eq!(sample(PaletteId::Turbo, 0.5)[3], 255);
        assert_eq!(sample(PaletteId::Categorical, 0.5)[3], 255);
    }

    // ── CoastType (Sprint 2 + Sprint 3 DD6) ──────────────────────────────────
    //
    // Paired with `ValueRange::Fixed(0.0, 5.0)` in the `coast_type` overlay
    // descriptor: each discriminant `d ∈ 0..=4` normalises to `t = d / 5`,
    // and `(t * 5.0) as usize = d` exactly. `Fixed(0.0, 4.0)` would map the
    // new Sprint 3 `LavaDelta (4)` to `idx = 5` → transparent; the earlier
    // `Fixed(0.0, 3.0)` bug did the same to `RockyHeadland (3)`. These tests
    // lock the 5-bin pairing.

    #[test]
    fn coast_type_five_discriminants_sample_distinct_table_entries() {
        // Each d ∈ 0..=4 with t = d/5 must sample COAST_TYPE_TABLE[d].
        for d in 0u8..=4u8 {
            let t = d as f32 / 5.0;
            let rgba = sample_f32(PaletteId::CoastType, t);
            assert_eq!(
                rgba, COAST_TYPE_TABLE[d as usize],
                "discriminant {d} (t={t}) should map to COAST_TYPE_TABLE[{d}]"
            );
            assert!(
                rgba[3] > 0.0,
                "discriminant {d} must be opaque (not the transparent sentinel)"
            );
        }
    }

    #[test]
    fn coast_type_unknown_sentinel_clamps_to_transparent() {
        // With descriptor Fixed(0.0, 5.0), the 0xFF sentinel's `t` clamps to
        // 1.0+, producing idx ≥ 5, i.e. transparent.
        let rgba = sample_f32(PaletteId::CoastType, 1.0);
        assert_eq!(rgba, [0.0, 0.0, 0.0, 0.0], "t=1.0 must be transparent");
        let rgba = sample_f32(PaletteId::CoastType, 100.0);
        assert_eq!(rgba, [0.0, 0.0, 0.0, 0.0], "large t must be transparent");
    }

    #[test]
    fn coast_type_regression_guard_against_fixed_0_to_4_bug() {
        // With the buggy Fixed(0.0, 4.0) descriptor (the Sprint 2 version,
        // which paired with a 4-entry table), LavaDelta (d=4) would produce
        // t = 4/4 = 1.0 → idx = 5 → transparent. This test pins the correct
        // Sprint 3 math: with Fixed(0.0, 5.0), t = 4/5 = 0.8 → idx = 4 →
        // COAST_TYPE_TABLE[4] (LavaDelta color).
        let rgba = sample_f32(PaletteId::CoastType, 4.0 / 5.0);
        assert_eq!(
            rgba, COAST_TYPE_TABLE[4],
            "LavaDelta must render as COAST_TYPE_TABLE[4], not transparent"
        );
        assert!(
            rgba[3] > 0.0,
            "LavaDelta must be opaque — the Fixed(0.0, 4.0) bug would make it transparent"
        );
    }

    // ── Sprint 3 DD6: COAST_TYPE_TABLE is locked at 5 entries ────────────────

    /// Compile-time-style guard: `COAST_TYPE_TABLE` must have exactly 5 rows
    /// (Cliff, Beach, Estuary, RockyHeadland, LavaDelta).
    #[test]
    fn coast_type_table_has_five_entries() {
        assert_eq!(
            COAST_TYPE_TABLE.len(),
            5,
            "COAST_TYPE_TABLE must have 5 entries after Sprint 3 DD6 LavaDelta addition"
        );
        // Each entry must be opaque so no non-Unknown coast cell renders
        // accidentally transparent.
        for (i, entry) in COAST_TYPE_TABLE.iter().enumerate() {
            assert_eq!(entry[3], 1.0, "COAST_TYPE_TABLE[{i}] must be opaque");
        }
    }
}
