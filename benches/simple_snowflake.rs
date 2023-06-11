/*
 * Copyright © 2023 Archer <archer@nefarious.dev>
 * Licensed under the Apache License, Version 2.0 (the "Licence");
 * you may not use this file except in compliance with the Licence.
 * You may obtain a copy of the Licence at
 *     https://www.apache.org/licenses/LICENSE-2.0
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the Licence is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the Licence for the specific language governing permissions and
 * limitations under the Licence.
 */

//! A benchmark comparing the blocking generator implementation with the lock-free implementation.
//!
//! This benchmark uses a trivial snowflake implementation to test the generator implementations without any potential
//! overhead introduced by (future versions of) the classic layout implementation.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use parking_lot::{Condvar, Mutex};
use snowdon::{Epoch, Generator, Layout, Snowflake};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

// This gives us about 139 years until we have to change the epoch below
const TIMESTAMP_BITS: usize = 42;
// We need a lot of bits here, as the generator could easily run out of snowflakes with the classic layout on a modern
// CPU. With 22 bits, we can generate 4,194,304 snowflakes per millisecond, so we'd have to generate one snowflake every
// 4 ns to exceed this limit. When running the benchmark on my local hardware, I need ~83.5 ns for a snowflake using the
// lock-free implementation. Once processors become even faster (or atomics even cheaper), we might have to increase the
// number of bits.
const SEQUENCE_BITS: usize = 22;
const SEQUENCE_MASK: u64 = (1 << SEQUENCE_BITS) - 1;

struct MySnowflakeParams;

impl Layout for MySnowflakeParams {
    #[inline]
    fn construct_snowflake(timestamp: u64, sequence_number: u64) -> u64 {
        assert!(!Self::exceeds_timestamp(timestamp) && !Self::exceeds_sequence_number(sequence_number));
        timestamp << SEQUENCE_BITS | sequence_number
    }
    #[inline]
    fn timestamp(input: u64) -> u64 {
        input >> SEQUENCE_BITS
    }
    #[inline]
    fn exceeds_timestamp(input: u64) -> bool {
        input >= 1 << TIMESTAMP_BITS
    }
    #[inline]
    fn sequence_number(input: u64) -> u64 {
        input & SEQUENCE_MASK
    }
    #[inline]
    fn exceeds_sequence_number(input: u64) -> bool {
        input >= 1 << SEQUENCE_BITS
    }
    #[inline]
    fn is_valid_snowflake(_input: u64) -> bool {
        // We don't have any constant parts in our snowflake that we could verify
        true
    }
}

impl Epoch for MySnowflakeParams {
    #[inline]
    fn millis_since_unix() -> u64 {
        // Return the first millisecond of 2023
        1672531200000
    }
}

type MySnowflake = Snowflake<MySnowflakeParams, MySnowflakeParams>;
type MyGenerator = Generator<MySnowflakeParams, MySnowflakeParams>;

fn bench_generator(iters: u64, generator_impl: fn(&MyGenerator) -> snowdon::Result<MySnowflake>) -> Duration {
    let generator = Arc::new(MyGenerator::default());
    let start_benchmark = Arc::new((Mutex::new(false), Condvar::new()));
    // Start 20 threads that are waiting for the benchmark to start
    let threads = (0..10)
        .map(|_| {
            let generator = generator.clone();
            let start = start_benchmark.clone();
            thread::spawn(move || {
                let (start_benchmark, cvar) = &*start;
                let mut started = start_benchmark.lock();
                // Wait for the benchmark to start and immediately release the lock
                if !*started {
                    cvar.wait(&mut started);
                    drop(started);
                }
                for _ in 0..iters {
                    let _ = black_box(generator_impl(&generator).unwrap());
                }
            })
        })
        .collect::<Vec<_>>();
    let (start_benchmark, cvar) = &*start_benchmark;
    let mut start_benchmark = start_benchmark.lock();

    let start = Instant::now();
    *start_benchmark = true;
    drop(start_benchmark);
    cvar.notify_all();
    for thread in threads {
        thread.join().unwrap();
    }
    start.elapsed()
}

fn generators(c: &mut Criterion) {
    let mut group = c.benchmark_group("Generators");
    group.bench_function("blocking (10 threads)", |b| {
        b.iter_custom(|iters| bench_generator(iters, Generator::generate_blocking))
    });
    group.bench_function("lock-free (10 threads)", |b| {
        b.iter_custom(|iters| bench_generator(iters, Generator::generate_lock_free))
    });
}

fn generators_sequential(c: &mut Criterion) {
    let mut group = c.benchmark_group("Generators (sequential)");
    let generator = MyGenerator::default();
    group.bench_function("blocking", |b| b.iter(|| black_box(generator.generate_blocking())));
    // Create a fresh generator to reset the sequence number
    group.bench_function("lock-free", |b| b.iter(|| black_box(generator.generate_lock_free())));
}

criterion_group!(benches, generators, generators_sequential);
criterion_main!(benches);
