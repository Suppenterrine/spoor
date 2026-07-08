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

    pub fn gen_bool(&mut self, probability: f64) -> bool {
        self.inner.gen_bool(probability.clamp(0.0, 1.0))
    }

    pub fn gen_index(&mut self, len: usize) -> Option<usize> {
        if len == 0 {
            None
        } else {
            Some(self.inner.gen_range(0..len as u64) as usize)
        }
    }
}
