//! xorshift64\* — 外部 dep なしの決定論 RNG (品質は data 生成用途に十分)

/// 決定論 RNG
#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// seed 0 は退化するので固定値と xor
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self {
            state: seed ^ 0x9E37_79B9_7F4A_7C15 | 1,
        }
    }

    /// 次の 64 bit
    pub const fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// `[0, 1)`
    pub fn unit(&mut self) -> f32 {
        // 上位 24 bit → f32 の仮数に収まる
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// `[lo, hi)`
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }

    /// `[lo, hi]` の整数
    pub fn int(&mut self, lo: i64, hi: i64) -> i64 {
        debug_assert!(hi >= lo);
        let span = (hi - lo + 1) as u64;
        lo + (self.next_u64() % span) as i64
    }

    /// `step` 刻みで `[lo, hi]` から 1 つ (寸法を「切りのいい値」にする)
    pub fn stepped(&mut self, lo: f32, hi: f32, step: f32) -> f32 {
        let n = ((hi - lo) / step).floor() as i64;
        lo + step * self.int(0, n.max(0)) as f32
    }

    /// `p` の確率で true
    pub fn chance(&mut self, p: f32) -> bool {
        self.unit() < p
    }

    /// slice から 1 つ
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        debug_assert!(!items.is_empty());
        let i = (self.next_u64() % items.len() as u64) as usize;
        &items[i]
    }
}

#[cfg(test)]
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
        }
        let s = a.stepped(10.0, 100.0, 5.0);
        assert!((s / 5.0).fract().abs() < 1e-6, "{s}");
    }
}
