//! A deterministic pseudo-random stream: xorshift64\* with no external dependency.
//!
//! The same seed gives the same stream on every machine (integer arithmetic only). It is for
//! generating data and sampling a parameter box, not for anything that must be unpredictable.
//! [`Rng::uniform`] is exactly uniform over any integer range (rejection); [`Rng::int`] takes a
//! remainder and is exactly uniform only when the span is a power of two.

/// A deterministic stream (xorshift64\*)
#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// A stream from `seed` (0 would be a fixed point of xorshift, so the seed is mixed first)
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self {
            state: seed ^ 0x9E37_79B9_7F4A_7C15 | 1,
        }
    }

    /// The next 64 bits
    pub const fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// An integer in `[lo, hi]`, every value with the same probability: draws above the largest
    /// multiple of the span are rejected and drawn again
    ///
    /// # Panics
    ///
    /// When `hi < lo`.
    pub fn uniform(&mut self, lo: i64, hi: i64) -> i64 {
        assert!(hi >= lo, "uniform: an empty range [{lo}, {hi}]");
        let span = hi.abs_diff(lo).wrapping_add(1);
        if span == 0 {
            // the whole of i64: every draw is in range
            return i64::from_ne_bytes(self.next_u64().to_ne_bytes());
        }
        // the draws that map evenly: [0, u64::MAX - (2^64 mod span)]
        let limit = u64::MAX - (u64::MAX - span + 1) % span;
        loop {
            let x = self.next_u64();
            if x <= limit {
                return lo.wrapping_add_unsigned(x % span);
            }
        }
    }

    /// `[0, 1)` (the top 24 bits as an `f32` mantissa)
    #[allow(clippy::cast_precision_loss, reason = "24 bits, exact in f32")]
    pub fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// `[lo, hi)`
    #[allow(
        clippy::suboptimal_flops,
        reason = "kept unfused: the stream's values are pinned by its users"
    )]
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }

    /// An integer in `[lo, hi]` by remainder: exactly uniform only when `hi - lo + 1` is a power of two
    /// (use [`Self::uniform`] otherwise)
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        reason = "the span and the remainder fit the range they were taken from"
    )]
    pub fn int(&mut self, lo: i64, hi: i64) -> i64 {
        debug_assert!(hi >= lo);
        let span = (hi - lo + 1) as u64;
        lo + (self.next_u64() % span) as i64
    }

    /// One of `[lo, hi]` in steps of `step` (dimensions at round values)
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::suboptimal_flops,
        reason = "a few hundred steps; kept unfused as its users pin it"
    )]
    pub fn stepped(&mut self, lo: f32, hi: f32, step: f32) -> f32 {
        let n = ((hi - lo) / step).floor() as i64;
        lo + step * self.int(0, n.max(0)) as f32
    }

    /// `true` with probability `p`
    pub fn chance(&mut self, p: f32) -> bool {
        self.unit() < p
    }

    /// One item of a slice
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the index is below the slice's length"
    )]
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        debug_assert!(!items.is_empty());
        let i = (self.next_u64() % items.len() as u64) as usize;
        &items[i]
    }
}

#[cfg(test)]
#[allow(
    clippy::many_single_char_names,
    reason = "test streams a, b, r and values x, i, u, s"
)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_in_range() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..1000 {
            let x = a.range(-3.0, 7.0);
            assert!((x - b.range(-3.0, 7.0)).abs() < f32::EPSILON);
            assert!((-3.0..7.0).contains(&x));
            let i = a.int(2, 5);
            assert_eq!(i, b.int(2, 5));
            assert!((2..=5).contains(&i));
            let u = a.uniform(-7, 9);
            assert_eq!(u, b.uniform(-7, 9));
            assert!((-7..=9).contains(&u));
        }
        let s = a.stepped(10.0, 100.0, 5.0);
        assert!((s / 5.0).fract().abs() < 1e-6, "{s}");
    }

    #[test]
    fn uniform_covers_the_ends_of_a_range() {
        let mut r = Rng::new(7);
        let (mut lo, mut hi) = (false, false);
        for _ in 0..10_000 {
            match r.uniform(-2, 2) {
                -2 => lo = true,
                2 => hi = true,
                _ => {}
            }
        }
        assert!(lo && hi);
        assert_eq!(
            Rng::new(1).uniform(i64::MIN, i64::MAX),
            Rng::new(1).uniform(i64::MIN, i64::MAX)
        );
        assert_eq!(Rng::new(3).uniform(5, 5), 5);
    }
}
