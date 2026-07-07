use rand::Rng;
use rand_chacha::ChaCha8Rng;
use rand::SeedableRng;

pub struct SeededRng {
    inner: ChaCha8Rng,
}

impl SeededRng {
    pub fn new(seed: u64) -> Self {
        let inner = ChaCha8Rng::seed_from_u64(seed);
        Self { inner }
    }

    pub fn with_rng(rng: ChaCha8Rng) -> Self {
        Self { inner: rng }
    }

    pub fn gen_bool(&mut self, probability: f64) -> bool {
        self.inner.gen_bool(probability.clamp(0.0, 1.0))
    }

    pub fn gen_range(&mut self, lo: u64, hi: u64) -> u64 {
        self.inner.gen_range(lo..=hi)
    }

    pub fn gen_index(&mut self, len: usize) -> Option<usize> {
        if len == 0 {
            None
        } else {
            Some(self.inner.gen_range(0..len as u64) as usize)
        }
    }

    pub fn into_inner(self) -> ChaCha8Rng {
        self.inner
    }
}
