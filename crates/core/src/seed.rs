//! Deterministic seed management for reproducible generation.
//!
//! [`Seed`] wraps a `u64` and provides:
//!
//! * [`Seed::to_rng`] — constructs a `Pcg64Mcg` seeded deterministically so
//!   the same `Seed` always yields the same sequence.
//! * [`Seed::fork`] — derives an independent child `Seed` by mixing the
//!   parent value with a stream index via splitmix64, so different streams
//!   and different parent seeds never collide.
//!
//! **Roadmap §验证层**: same `Seed` + same preset must reproduce CPU-path
//! output bit-exact; this module is the stable primitive that guarantees it.

use rand::SeedableRng;
use rand_pcg::Pcg64Mcg;

// ─── Seed newtype ─────────────────────────────────────────────────────────────

/// A deterministic 64-bit seed.
///
/// Cheap to copy everywhere — it's just a `u64`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Seed(pub u64);

impl Seed {
    /// Construct a `Pcg64Mcg` RNG seeded from this value.
    ///
    /// Calling this twice on the **same** `Seed` produces two independent
    /// RNGs that emit identical sequences.
    pub fn to_rng(&self) -> Pcg64Mcg {
        Pcg64Mcg::seed_from_u64(self.0)
    }

    /// Derive a child `Seed` for the given `stream` index.
    ///
    /// Uses the splitmix64 hash mixer so:
    /// * `Seed(A).fork(s)` ≠ `Seed(B).fork(s)` when A ≠ B
    /// * `Seed(A).fork(s1)` ≠ `Seed(A).fork(s2)` when s1 ≠ s2
    ///
    /// Deterministic: same inputs always produce the same output.
    pub fn fork(&self, stream: u64) -> Seed {
        Seed(splitmix64(self.0 ^ splitmix64(stream)))
    }
}

// ─── splitmix64 ──────────────────────────────────────────────────────────────

/// Bijective 64-bit hash / mixer (Sebastiano Vigna's splitmix64).
///
/// Used as a two-argument mixer inside [`Seed::fork`]: feeding different
/// `(parent, stream)` pairs always yields different child seeds.
fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E3779B97F4A7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rand::RngCore;

    // 1. same seed → identical sequence
    #[test]
    fn same_seed_same_sequence() {
        let seed = Seed(42);
        let mut rng_a = seed.to_rng();
        let mut rng_b = seed.to_rng();
        let seq_a: Vec<u64> = (0..10).map(|_| rng_a.next_u64()).collect();
        let seq_b: Vec<u64> = (0..10).map(|_| rng_b.next_u64()).collect();
        assert_eq!(seq_a, seq_b, "same seed must produce identical sequences");
    }

    // 2. different fork streams produce different output
    #[test]
    fn fork_streams_diverge() {
        let seed = Seed(42);
        let mut rng1 = seed.fork(1).to_rng();
        let mut rng2 = seed.fork(2).to_rng();
        let vals1: Vec<u64> = (0..100).map(|_| rng1.next_u64()).collect();
        let vals2: Vec<u64> = (0..100).map(|_| rng2.next_u64()).collect();
        // weak statistical check: means should differ
        let mean1 = vals1.iter().map(|&v| v as f64).sum::<f64>() / 100.0;
        let mean2 = vals2.iter().map(|&v| v as f64).sum::<f64>() / 100.0;
        assert!(
            (mean1 - mean2).abs() > 100.0,
            "fork streams should diverge: mean1={mean1}, mean2={mean2}"
        );
    }

    // 3. fork is deterministic
    #[test]
    fn fork_determinism() {
        let seed = Seed(42);
        let child_a = seed.fork(1);
        let child_b = seed.fork(1);
        assert_eq!(child_a, child_b, "fork must be deterministic");
    }
}
