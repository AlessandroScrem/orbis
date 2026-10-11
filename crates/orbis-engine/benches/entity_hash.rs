use criterion::{Criterion, criterion_group, criterion_main};
use legion::*;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash as StdHash, Hasher};

/// Run benchmark:
///
/// '''rust, ignore
/// cargo bench --bench entity_hash_bench
/// '''
mod entity_hash {
    use super::*;
    // use std::hash::Hasher;

    pub trait Hash {
        fn fast_hash(&self) -> u64;
    }

    impl Hash for Entity {
        fn fast_hash(&self) -> u64 {
            struct SimpleHasher(u64);

            impl std::hash::Hasher for SimpleHasher {
                fn finish(&self) -> u64 {
                    self.0
                }
                fn write(&mut self, bytes: &[u8]) {
                    for &b in bytes {
                        self.0 = self.0.wrapping_mul(31).wrapping_add(b as u64);
                    }
                }
            }

            let mut hasher = SimpleHasher(0);
            std::hash::Hash::hash(self, &mut hasher); // Hash interno di Legion
            hasher.finish()
        }
    }
}

use orbis_engine::EntityRawU64;
pub trait FastHash {
    fn fast_hash2(&self) -> u64;
    fn fast_hash3(&self) -> u64;
}

impl FastHash for Entity {
    #[inline(always)]
    fn fast_hash2(&self) -> u64 {
        let raw = self.as_raw_u64();

        // Mix semplice: XOR e shift per diffondere i bit
        let mut h = raw;
        h ^= h >> 33;
        h = h.wrapping_mul(0xff51afd7ed558ccd);
        h ^= h >> 33;
        h = h.wrapping_mul(0xc4ceb9fe1a85ec53);
        h ^= h >> 33;
        h
    }

    #[inline(always)]
    fn fast_hash3(&self) -> u64 {
        let mut h = self.as_raw_u64();
        h ^= h >> 32;
        h ^= h >> 17;
        h ^= h >> 7;
        h ^ (h.wrapping_mul(0x9E3779B97F4A7C15))
    }
}

fn bench_hashes(c: &mut Criterion) {
    let mut world = World::default();
    let entities: Vec<_> = (0..100_000).map(|_| world.push(())).collect();

    use entity_hash::Hash;

    println!("Time calculated for {} hashes : ", entities.len());

    // Benchmark SimpleHasher
    c.bench_function("simple_hasher", |b| {
        b.iter(|| {
            for &e in &entities {
                std::hint::black_box(e.fast_hash());
            }
        });
    });

    // Benchmark DefaultHasher (Legion)
    c.bench_function("default_hasher", |b| {
        b.iter(|| {
            for &e in &entities {
                let mut hasher = DefaultHasher::new();
                e.hash(&mut hasher);
                std::hint::black_box(hasher.finish());
            }
        });
    });

    // Benchmark SimpleHasher
    c.bench_function("Fast_hasher2", |b| {
        b.iter(|| {
            for &e in &entities {
                std::hint::black_box(e.fast_hash2());
            }
        });
    });

    // Benchmark SimpleHasher
    c.bench_function("Fast_hasher3", |b| {
        b.iter(|| {
            for &e in &entities {
                std::hint::black_box(e.fast_hash3());
            }
        });
    });
}

criterion_group!(benches, bench_hashes);
criterion_main!(benches);
