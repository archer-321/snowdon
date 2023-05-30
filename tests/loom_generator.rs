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

#[cfg(not(loom))]
compile_error!("this test requires the `loom` configuration option");

#[cfg(not(feature = "lock-free"))]
compile_error!("this test requires the `lock-free` feature");

use loom::sync::Arc;
use loom::thread;
use snowdon::system_time_mock::SystemTime;
use snowdon::{Epoch, Generator, Layout};
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};

struct SimpleParams;

impl Layout for SimpleParams {
    fn construct_snowflake(timestamp: u64, sequence_number: u64) -> u64 {
        assert!(!Self::exceeds_timestamp(timestamp) && !Self::exceeds_sequence_number(sequence_number));
        timestamp << 22 | sequence_number
    }
    fn get_timestamp(input: u64) -> u64 {
        input >> 22
    }
    fn exceeds_timestamp(input: u64) -> bool {
        input >= 1 << 42
    }
    fn get_sequence_number(input: u64) -> u64 {
        input & ((1 << 22) - 1)
    }
    fn exceeds_sequence_number(input: u64) -> bool {
        input >= 1 << 22
    }
    fn is_valid_snowflake(_input: u64) -> bool {
        true
    }
}

impl Epoch for SimpleParams {
    fn millis_since_unix() -> u64 {
        0
    }
}

type SimpleGenerator = Generator<SimpleParams, SimpleParams>;

#[test]
fn unique_lock_free() {
    loom::model(|| {
        let _g = SystemTime::acquire_lock();
        let next_time = Arc::new(AtomicU64::new(6));
        SystemTime::set_time(5);

        let generator = Arc::new(SimpleGenerator::default());

        // Create two snowflake generating threads and a thread incrementing the current time
        let snowflakes: Vec<_> = (0..2)
            .map(|_| {
                let generator = generator.clone();
                thread::spawn(move || {
                    // Generate 3 snowflakes and return them
                    (0..3)
                        .map(|_| generator.generate_lock_free().unwrap().get())
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let time = thread::spawn(move || {
            for _ in 0..2 {
                SystemTime::set_time(next_time.fetch_add(1, Ordering::Relaxed));
            }
        });
        let mut set = HashSet::with_capacity(6);
        for snowflake in snowflakes.into_iter().flat_map(|snowflake| snowflake.join().unwrap()) {
            set.insert(snowflake);
        }
        time.join().unwrap();
        assert_eq!(6, set.len());

    });
}
