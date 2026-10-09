//! Детерминированный PRNG и вывод производных seed'ов.
//!
//! Свой генератор, а не `rand`: обфускация должна быть **воспроизводимой между
//! версиями инструмента** - алгоритм замирает в формате, а не в изменении API
//! чужого крейта. Схема: SplitMix64 для инициализации + xoshiro256++ для генерации.

/// Генератор xoshiro256++.
#[derive(Debug, Clone)]
pub struct Rng {
    s: [u64; 4],
}

const PHI: u64 = 0x9E37_79B9_7F4A_7C15;

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(PHI);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn rotl(x: u64, k: u32) -> u64 {
    x.rotate_left(k)
}

impl Rng {
    /// Генератор из мастер-сиди.
    pub fn from_seed(seed: u64) -> Self {
        let mut sm = seed;
        let s = [
            splitmix64(&mut sm),
            splitmix64(&mut sm),
            splitmix64(&mut sm),
            splitmix64(&mut sm),
        ];
        Self { s }
    }

    /// Следующее 64-битное значение.
    pub fn next_u64(&mut self) -> u64 {
        let result = rotl(self.s[0].wrapping_add(self.s[3]), 23).wrapping_add(self.s[0]);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = rotl(self.s[3], 45);
        result
    }

    /// Значение из `[0, bound)` без смещения (отбраковка перекоса).
    pub fn below(&mut self, bound: u64) -> u64 {
        assert!(bound > 0, "bound должен быть больше нуля");
        // Пространство [0, 2^64) режем на куски размером bound; хвост отбрасываем.
        let limit = u64::MAX - (u64::MAX % bound);
        loop {
            let v = self.next_u64();
            if v < limit {
                return v % bound;
            }
        }
    }

    /// Случайный элемент среза.
    pub fn choose<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            None
        } else {
            Some(&items[self.below(items.len() as u64) as usize])
        }
    }

    /// Перемешать срез на месте (Fisher–Yates).
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below((i + 1) as u64) as usize;
            items.swap(i, j);
        }
    }

    /// Случайные байты.
    pub fn fill_bytes(&mut self, out: &mut [u8]) {
        for chunk in out.chunks_mut(8) {
            let v = self.next_u64().to_le_bytes();
            let n = chunk.len();
            chunk.copy_from_slice(&v[..n]);
        }
    }

    /// Вектор случайных байтов.
    pub fn bytes(&mut self, len: usize) -> Vec<u8> {
        let mut v = vec![0u8; len];
        self.fill_bytes(&mut v);
        v
    }
}

/// FNV-1a 64 - детерминированный вывод производного сида.
///
/// Разделение доменов обязательно: пассы не должны делить одну и ту же
/// последовательность (иначе переименование и шифрование «случайно»
/// коррелируют, и это видно статистикой).
pub fn derive_seed(master: u64, domain: &str, index: u64) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |byte: u8| {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    };
    for b in master.to_le_bytes() {
        mix(b);
    }
    for b in domain.as_bytes() {
        mix(*b);
    }
    for b in index.to_le_bytes() {
        mix(b);
    }
    hash
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]
    use super::*;

    #[test]
    fn deterministic_across_instances() {
        let mut a = Rng::from_seed(42);
        let mut b = Rng::from_seed(42);
        let av: Vec<u64> = (0..32).map(|_| a.next_u64()).collect();
        let bv: Vec<u64> = (0..32).map(|_| b.next_u64()).collect();
        assert_eq!(av, bv);
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = Rng::from_seed(1);
        let mut b = Rng::from_seed(2);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn below_respects_bound() {
        let mut r = Rng::from_seed(7);
        for _ in 0..1000 {
            assert!(r.below(10) < 10);
        }
    }

    #[test]
    fn derive_seed_is_domain_separated() {
        assert_ne!(derive_seed(1, "rename", 0), derive_seed(1, "strings", 0));
        assert_ne!(derive_seed(1, "rename", 0), derive_seed(1, "rename", 1));
        assert_eq!(derive_seed(1, "rename", 3), derive_seed(1, "rename", 3));
    }

    #[test]
    fn shuffle_is_permutation() {
        let mut r = Rng::from_seed(9);
        let mut v: Vec<u32> = (0..50).collect();
        r.shuffle(&mut v);
        let mut sorted = v.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..50).collect::<Vec<u32>>());
    }
}
