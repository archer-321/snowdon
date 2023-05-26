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

//! This crate is an implementation of snowflake IDs - an ID format introduced by Twitter as IDs for tweets.
//!
//! Snowflakes consist of a timestamp (with millisecond precision) and a sequence number to allow generators to create
//! multiple snowflakes in the same millisecond. The original version used by Twitter also includes a machine ID. This
//! allows distributed systems to create a unique snowflake without requiring coordination between the instances.
//!
//! While the original implementation of snowflake IDs includes a fixed layout and epoch, this implementation is
//! designed to work with various kinds of snowflake-like IDs. Our only requirements are that the snowflake contains a
//! timestamp and a sequence number in that order (other constant parts like the machine ID can still interleave).
//! Nevertheless, the original layout introduced by Twitter is implemented as [`ClassicLayout`].
//!
//! The original Twitter snowflake implementation can be found [here][snowflake-gh]. You can read more about snowflakes
//! in Twitter's [blog post][snowflake-blog] about them.
//!
//! # Serde compatibility
//!
//! [`Snowflake`] and [`SnowflakeComparator`] implement Serde's `Serialize` and `Deserialize` traits if the crate
//! feature `serde` is enabled. To enable Serde compatibility, simply opt in by changing your dependency line in
//! `Cargo.toml` to:
//!
//! ```toml
//! [dependencies]
//! # ...
//! snowdon = { version = "^0.1", features = ["serde"] }
//! ```
//!
//! Note that [`Generator`] *doesn't* implement (de-)serialization even if the feature is enabled. A generator's state
//! highly depends on the machine it's running on, and usually, there's no reason to serialize it or otherwise store and
//! load it somehow.
//!
//! # Example
//!
//! The example below implements a custom layout that uses 42 bits for the timestamp, 10 bits for a machine ID, and 12
//! bits for the sequence number. Note that this example doesn't include the machine ID implementation. There isn't a
//! single "valid" implementation of machine IDs, as they heavily depend on your application and the way you deploy
//! your application. However, common ways to derive a machine ID include using the machine's private IP address and
//! provisioning a machine ID using some coordinated service. To avoid defaulting to an implementation that doesn't
//! work for most users, we don't provide an implementation for this function at all.
#![cfg_attr(
    any(
        all(feature = "blocking", not(feature = "lock-free")),
        all(feature = "lock-free", not(feature = "blocking"))
    ),
    doc = "```"
)]
#![cfg_attr(all(feature = "blocking", feature = "lock-free"), doc = "```ignore")]
//! use snowdon::{Epoch, Generator, Layout, Snowflake, SnowflakeComparator};
//! use std::time::SystemTime;
//!
//! fn get_machine_id() -> u64 {
//!     // This is where you would implement your machine ID
//! #   0
//! }
//!
//! // We make our structure hidden, as this is an implementation detail that most
//! // users of our custom ID won't need
//! #[derive(Debug)]
//! #[doc(hidden)]
//! pub struct MySnowflakeParams;
//!
//! impl Layout for MySnowflakeParams {
//!     fn construct_snowflake(timestamp: u64, sequence_number: u64) -> u64 {
//!         assert!(
//!             !Self::exceeds_timestamp(timestamp)
//!                 && !Self::exceeds_sequence_number(sequence_number)
//!         );
//!         (timestamp << 22) | (get_machine_id() << 12) | sequence_number
//!     }
//!     fn get_timestamp(input: u64) -> u64 {
//!         input >> 22
//!     }
//!     fn exceeds_timestamp(input: u64) -> bool {
//!         input >= (1 << 42)
//!     }
//!     fn get_sequence_number(input: u64) -> u64 {
//!         input & ((1 << 12) - 1)
//!     }
//!     fn exceeds_sequence_number(input: u64) -> bool {
//!         input >= (1 << 12)
//!     }
//!     fn is_valid_snowflake(input: u64) -> bool {
//!         // Our snowflake format doesn't have any constant parts that we could
//!         // validate here
//!         true
//!     }
//! }
//! impl Epoch for MySnowflakeParams {
//!     fn millis_since_unix() -> u64 {
//!         // Our epoch for this example is the first millisecond of 2015
//!         1420070400000
//!     }
//! }
//!
//! // Define our snowflake and generator types
//! pub type MySnowflake = Snowflake<MySnowflakeParams, MySnowflakeParams>;
//! pub type MySnowflakeGenerator = Generator<MySnowflakeParams, MySnowflakeParams>;
//!
//! // Use our types in our API
//! pub struct UserProfile {
//!     name: String,
//!     age: u8,
//!     employee_id: MySnowflake,
//! }
//!
//! pub fn save_user_profile(profile: &UserProfile) {
//!     fn save_to_db(_id: MySnowflake, _profile: &UserProfile) {}
//!     // Snowflakes implement Copy, so we don't need to worry about moving the
//!     // value out of our profile here
//!     save_to_db(profile.employee_id, profile);
//! }
//!
//! // You can compare snowflakes with each other and even with arbitrary timestamps
//! // (using `SnowflakeComparator`)
//! let generator = MySnowflakeGenerator::default();
//! let first_snowflake = generator.generate().unwrap();
//! let second_snowflake = generator.generate().unwrap();
//! assert!(first_snowflake < second_snowflake);
//!
//! let timestamp = first_snowflake.get_timestamp().unwrap();
//! let comparator =
//!     SnowflakeComparator::from_system_time(SystemTime::UNIX_EPOCH).unwrap();
//! assert!(comparator < first_snowflake);
//! let comparator = SnowflakeComparator::from_system_time(timestamp).unwrap();
//! assert_eq!(first_snowflake, comparator);
//! // The comparator only represents a timestamp, so we can't expect it to be
//! // greater than `second_snowflake` here
#![doc = "```"]
//!
//! [snowflake-gh]: https://github.com/twitter-archive/snowflake/tree/b3f6a3c6ca8e1b6847baa6ff42bf72201e2c2231
//! [snowflake-blog]: https://blog.twitter.com/engineering/en_us/a/2010/announcing-snowflake

#![warn(missing_docs, missing_debug_implementations)]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(not(any(feature = "blocking", feature = "lock-free")))]
compile_error!("you must enable at least one generator implementation (blocking or lock-free)");

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::cmp;
use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
#[cfg(feature = "lock-free")]
use std::sync::atomic;
#[cfg(feature = "lock-free")]
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
#[cfg(feature = "blocking")]
use std::sync::Mutex;
use std::time::Duration;
#[cfg(not(test))]
use std::time::SystemTime;
#[cfg(test)]
use system_time_mock::SystemTime;

mod classic;
mod comparator;

// A mocked system time to run our unit tests
#[cfg(test)]
mod system_time_mock {
    use lazy_static::lazy_static;
    use std::sync::{Mutex, MutexGuard};
    use std::time::Duration;

    lazy_static! {
        static ref TIME: Mutex<u64> = Mutex::new(0);
        static ref TIME_LOCK: Mutex<()> = Mutex::new(());
    }

    /// A mocked [`SystemTime`](std::time::SystemTime) used to test Snowdon.
    // Skip coverage: We only use this system time implementation in tests, and our tests don't try to verify its
    // correctness.
    #[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
    #[repr(transparent)]
    pub struct SystemTime(u64);
    // End skip coverage

    impl SystemTime {
        pub const UNIX_EPOCH: Self = Self(0);

        /// Sets the current time to the given number of milliseconds since the Unix epoch.
        pub(crate) fn set_time(time: u64) {
            *TIME.lock().unwrap() = time;
        }

        /// Acquires a lock on the global time state.
        ///
        /// This function is used to ensure that tests don't mutate the global time state concurrently. In tests, you
        /// should acquire this lock and only drop the returned guard *after* your test has finished.
        pub(crate) fn acquire_lock() -> MutexGuard<'static, ()> {
            // If a previous test panicked, this lock will be poisoned. We don't care about the ZST inside, however,
            // as we only use this lock to ensure only one test is mutating the system time at any given moment.
            TIME_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
        }

        pub(crate) fn checked_add(&self, duration: Duration) -> Option<SystemTime> {
            // We intentionally exclude u64::MAX here to test times that overflow `SystemTime`'s underlying data type
            let duration = duration.as_millis();
            if self.0 as u128 + duration >= u64::MAX as u128 {
                return None;
            }
            Some(Self(self.0 + duration as u64))
        }

        pub(crate) fn duration_since(&self, earlier: SystemTime) -> Result<Duration, SystemTimeError> {
            if earlier.0 > self.0 {
                return Err(SystemTimeError);
            }
            Ok(Duration::from_millis(self.0 - earlier.0))
        }

        pub(crate) fn now() -> Self {
            Self(*TIME.lock().unwrap())
        }
    }

    pub(crate) struct SystemTimeError;
}

/// A thread-safe snowflake generator for a given snowflake [layout](Layout) and [epoch](Epoch).
///
/// The generator has two type parameters to identify your snowflake format at compile-time. Refer to the example below
/// and the documentation of [`Layout`] and [`Epoch`] for details on how to define a generator for your snowflake
/// format.
///
/// # Example
// We need to exclude this test when running tests with --all-features because of rust-lang/rust#67295
#[cfg_attr(
    any(
        all(feature = "blocking", not(feature = "lock-free")),
        all(feature = "lock-free", not(feature = "blocking"))
    ),
    doc = "```"
)]
#[cfg_attr(all(feature = "blocking", feature = "lock-free"), doc = "```ignore")]
/// // Specify our custom snowflake
/// use snowdon::{ClassicLayout, Epoch, Generator, MachineId};
/// #[derive(Debug)]
/// pub struct SnowflakeParameters;
/// impl MachineId for SnowflakeParameters {
///     fn machine_id() -> u64 {
///         // Somehow derive a unique machine ID (e.g. from the private IP).
///         // For this example, we simply return a constant. Note that this
///         // *won't* work for multi-instance deployments!
///         0
///     }
/// }
/// impl Epoch for SnowflakeParameters {
///     fn millis_since_unix() -> u64 {
///         // For this example, our epoch starts with the first second of 2015
///         1420070400000
///     }
/// }
/// pub type SnowflakeGenerator =
///     Generator<ClassicLayout<SnowflakeParameters>, SnowflakeParameters>;
/// pub type Snowflake = snowdon::Snowflake<
///     ClassicLayout<SnowflakeParameters>,
///     SnowflakeParameters,
/// >;
///
/// // Create two snowflakes using our custom snowflake generator
///
/// use std::sync::Arc;
/// use std::thread;
/// let generator = Arc::new(SnowflakeGenerator::default());
/// let res1 = {
///     let generator = generator.clone();
///     thread::spawn(move || {
///         let snowflake = generator.generate().unwrap();
///         println!("Thread 1's snowflake is {snowflake}");
///         snowflake
///     })
/// };
/// let res2 = {
///     let generator = generator.clone();
///     thread::spawn(move || {
///         let snowflake = generator.generate().unwrap();
///         println!("Thread 2's snowflake is {snowflake}");
///         snowflake
///     })
/// };
/// assert_ne!(res1.join().unwrap(), res2.join().unwrap());
/// ```
#[derive(Debug, Clone)]
pub struct Generator<L, E>
where
    L: Layout,
    E: Epoch,
{
    // We use the regular `Mutex` from the standard library here, as we don't need to hold it across any await points.
    // When compared to the mutex available in Tokio, it's generally better to use the standard implementation wherever
    // possible, as the Tokio Mutex's ability to be held across await points introduces considerable overhead.
    #[cfg(feature = "blocking")]
    last_snowflake_blocking: Arc<Mutex<u64>>,
    #[cfg(feature = "lock-free")]
    last_snowflake_atomic: Arc<AtomicU64>,
    _marker: PhantomData<(L, E)>,
}

impl<L, E> Generator<L, E>
where
    L: Layout,
    E: Epoch,
{
    /// Generates a new snowflake using the *blocking* snowflake implementation.
    ///
    /// This method is available if only the blocking snowflake implementation is enabled (the `blocking` feature). It
    /// behaves exactly like [`generate_blocking`](Self::generate_blocking). Refer to its documentation for more
    /// details.
    ///
    /// You should mostly use this feature if you'd like to be able to switch to the *lock-free* implementation in the
    /// future by simply disabling the `blocking` feature and enabling `lock-free`. As both implementations essentially
    /// provide the same public API (including the same errors and correctness guarantees if a snowflake is generated),
    /// you won't need to change code that uses this function when switching over to the other implementation.
    #[cfg(any(all(feature = "blocking", not(feature = "lock-free")), doc))]
    #[cfg_attr(docsrs, doc(cfg(all(feature = "blocking", not(feature = "lock-free")))))]
    #[inline]
    pub fn generate(&self) -> Result<Snowflake<L, E>> {
        self.generate_blocking()
    }

    /// Generates a new snowflake using the *lock-free* snowflake implementation.
    ///
    /// This method is available if only the lock-free snowflake implementation is enabled (the `lock-free` feature). It
    /// behaves exactly like [`generate_lock_free`](Self::generate_lock_free). Refer to its documentation for more
    /// details.
    ///
    /// You should mostly use this feature if you'd like to be able to switch to the *blocking* implementation in the
    /// future by simply disabling the `lock-free` feature and enabling `blocking`. As both implementations essentially
    /// provide the same public API (including the same errors and correctness guarantees if a snowflake is generated),
    /// you won't need to change code that uses this function when switching over to the other implementation.
    #[cfg(any(all(feature = "lock-free", not(feature = "blocking")), doc))]
    #[cfg_attr(docsrs, doc(cfg(all(feature = "lock-free", not(feature = "blocking")))))]
    #[inline]
    pub fn generate(&self) -> Result<Snowflake<L, E>> {
        self.generate_lock_free()
    }

    /// Generates a new snowflake using a blocking algorithm. I.e. this algorithm uses locks.
    ///
    /// The generated snowflake is guaranteed to be unique and monotonic. I.e., for every snowflake `b` returned
    /// *after* the snowflake `a` `a < b`. The returned snowflake uses the current system time.
    ///
    /// If you don't care whether the snowflake implementation you're using is lock-free or blocking, you should
    /// consider using [`Generator::generate`] instead. This allows switching the snowflake implementation across your
    /// application by simply switching crate features in your dependency declaration.
    ///
    /// # Errors
    ///
    /// If the system clock went backwards, this method will return [`Error::NonMonotonicClock`] instead. If the
    /// generator's [`Epoch`] is in the future, this will return [`Error::InvalidEpoch`]. If too many snowflakes have
    /// been generated in this millisecond and the sequence number would overflow, this returns
    /// [`Error::SnowflakeExhaustion`]. Usually, this means that you can retry the snowflake generation in the next
    /// millisecond. If the timestamp exceeds the limits of the generator's layout, this method returns
    /// [`Error::FatalSnowflakeExhaustion`] instead. Unlike the previous error, this error indicates that it won't be
    /// possible to generate snowflakes with this generator in the future.
    #[cfg(any(feature = "blocking", doc))]
    #[cfg_attr(docsrs, doc(cfg(feature = "blocking")))]
    pub fn generate_blocking(&self) -> Result<Snowflake<L, E>> {
        // We don't include any code in this function that could panic (outside this
        // `unwrap`), so we can safely unwrap this lock
        let mut last_snowflake = self.last_snowflake_blocking.lock().unwrap();
        // Acquire the timestamp *after* we got the lock to be more accurate about our
        // non-monotonic clock errors
        let time = Self::get_timestamp()?;
        let last_timestamp = L::get_timestamp(*last_snowflake);
        match last_timestamp.cmp(&time) {
            cmp::Ordering::Equal => {
                // We've already generated a snowflake for this timestamp, so increment the
                // sequence number
                let sequence = L::get_sequence_number(*last_snowflake)
                    .checked_add(1)
                    .ok_or(Error::SnowflakeExhaustion)?;
                if L::exceeds_sequence_number(sequence) {
                    return Err(Error::SnowflakeExhaustion);
                }
                let snowflake = L::construct_snowflake(time, sequence);
                *last_snowflake = snowflake;
                Ok(Snowflake {
                    inner: snowflake,
                    _marker: PhantomData,
                })
            }
            cmp::Ordering::Less => {
                // This is the first snowflake generated for this timestamp, so set the sequence
                // number to 0
                let snowflake = L::construct_snowflake(time, 0);
                *last_snowflake = snowflake;
                Ok(Snowflake {
                    inner: snowflake,
                    _marker: PhantomData,
                })
            }
            cmp::Ordering::Greater => {
                // The clock went backwards. Note that we can safely assume that the clock went
                // backwards, as we obtain the timestamp *after* acquiring the
                // lock.
                Err(Error::NonMonotonicClock)
            }
        }
    }

    /// Generates a new snowflake using a lock-free algorithm.
    ///
    /// The generated snowflake is guaranteed to be unique and monotonic. I.e., for every snowflake `b` returned
    /// *after* the snowflake `a` `a < b`. The returned snowflake uses the current system time.
    ///
    /// If you don't care whether the snowflake implementation you're using is lock-free or blocking, you should
    /// consider using [`Generator::generate`] instead. This allows switching the snowflake implementation across your
    /// application by simply switching crate features in your dependency declaration.
    ///
    /// # Errors
    ///
    /// If the system clock went backwards, this method will return [`Error::NonMonotonicClock`] instead. If the epoch
    /// is in the future, [`Error::InvalidEpoch`] is returned instead. If too many snowflakes have been generated in
    /// this millisecond and the sequence number would overflow, this returns [`Error::SnowflakeExhaustion`]. Usually,
    /// this means that you can retry the snowflake generation in the next millisecond. If the timestamp exceeds the
    /// limits of the generator's layout, this method returns [`Error::FatalSnowflakeExhaustion`] instead. Unlike the
    /// previous error, this error indicates that it won't be possible to generate snowflakes with this generator in the
    /// future.
    ///
    /// # Lock-freedom
    ///
    /// The algorithm used to generate snowflakes with this method is *lock-free*. This means that suspension of one
    /// thread can't result in failure or suspension of another thread. Moreover, at any given time, there's guaranteed
    /// progress across the system. I.e., at least one thread successfully generates a snowflake at any given point.
    #[cfg(any(feature = "lock-free", doc))]
    #[cfg_attr(docsrs, doc(cfg(feature = "lock-free")))]
    pub fn generate_lock_free(&self) -> Result<Snowflake<L, E>> {
        // Initially, load the last generated snowflake once. If the CAS loop below has to loop, we overwrite this value
        // with the result of our CAS operation.
        // Note that we don't have to see all previous threads' modifications at this point. Our CAS operation below
        // establishes an inter-thread happens-before relationship with other threads. I.e., we only successfully
        // generate a new snowflake if we saw all other threads' store operations on this variable. If we see an
        // outdated value here (or if another thread interleaves), we simply try again.
        // Naturally, this load operation can't be reordered after the store below, as the value loaded here carries a
        // dependency into the load operation of the CAS operation below.
        let mut last_snowflake = self.last_snowflake_atomic.load(atomic::Ordering::Relaxed);
        // This CAS loop is the same as `AtomicU64::fetch_update`. However, we use the
        // manual loop implementation, as our update algorithm (the snowflake
        // generation) is fairly complex, and implementing the loop manually more
        // closely matches the model implemented in the `spin` directory.
        // Skip coverage: This loop will only be executed once in our current unit tests, as it's hard to force the CAS
        // operation at the bottom to fail (deterministically).
        loop {
            // End skip coverage (loop statement above)
            let time = Self::get_timestamp()?;
            let last_timestamp = L::get_timestamp(last_snowflake);
            let sequence = match last_timestamp.cmp(&time) {
                cmp::Ordering::Equal => {
                    // The timestamp didn't change, so increment the sequence number
                    L::get_sequence_number(last_snowflake)
                        .checked_add(1)
                        .and_then(|sequence| {
                            if L::exceeds_sequence_number(sequence) {
                                None
                            } else {
                                Some(sequence)
                            }
                        })
                        .ok_or(Error::SnowflakeExhaustion)?
                }
                cmp::Ordering::Less => {
                    // The timestamp increased, so reset the sequence number
                    0
                }
                cmp::Ordering::Greater => {
                    // The clock went backwards. Note that we can safely assume that the clock went
                    // backwards, as we obtain the timestamp *after* loading the
                    // last snowflake atomically.
                    return Err(Error::NonMonotonicClock);
                }
            };
            let new_snowflake = L::construct_snowflake(time, sequence);
            // The load operation of the `compare_exchange_weak` below needs to see all previous writes. I.e., other
            // writes to this value need to be in an inter-thread happens-before relationship with this load. We need to
            // load the value with Acquire ordering and store it with Release to make this thread synchronize with other
            // threads.
            // Note that we don't necessarily need to see all previous stores if this operation fails. Refer to the
            // comment above the `load` operation above for details.
            match self.last_snowflake_atomic.compare_exchange_weak(
                last_snowflake,
                new_snowflake,
                atomic::Ordering::AcqRel,
                atomic::Ordering::Relaxed,
            ) {
                Ok(_) => {
                    return Ok(Snowflake {
                        inner: new_snowflake,
                        _marker: PhantomData,
                    });
                }
                // Skip coverage: With our current testing setup, it's essentially impossible to reach this state
                // deterministically. We can consider re-enabling this if we ever use something like `loom` for our
                // verification (TODO).
                Err(current_value) => {
                    // Our CAS operation failed; store the current value and try again
                    last_snowflake = current_value
                } // End skip coverage
            }
        }
    }

    /// Gets the current timestamp in milliseconds since this generator's epoch.
    ///
    /// If this generator's epoch is *after* the current timestamp, this returns [`Error::InvalidEpoch`]. If the
    /// timestamp wouldn't fit in this generator's layout, this function returns [`Error::FatalSnowflakeExhaustion`].
    fn get_timestamp() -> Result<u64> {
        let time = SystemTime::now()
            .duration_since(
                SystemTime::UNIX_EPOCH
                    // We use `checked_add` here because of the unlikely event that the epoch overflows `SystemTime`'s
                    // underlying data type
                    .checked_add(Duration::from_millis(E::millis_since_unix()))
                    .ok_or(Error::FatalSnowflakeExhaustion)?,
            )
            .map_err(|_| Error::InvalidEpoch)?
            .as_millis();
        if time > u64::MAX as u128 || L::exceeds_timestamp(time as u64) {
            return Err(Error::FatalSnowflakeExhaustion);
        }
        Ok(time as u64)
    }

    /// A small helper function for unit tests to set this generator's last generated snowflake.
    ///
    /// This function is intended to speed up snowflake exhaustion tests. Instead of having to generate tons of
    /// snowflakes, this function allows setting the last snowflake's sequence number directly.
    ///
    /// Naturally, you must not use this function in any way outside unit tests.
    #[doc(hidden)]
    #[cfg(test)]
    fn set_last_snowflake(&self, last_snowflake: u64) {
        // Skip branch coverage: This code is explicitly only used for unit tests, so we're not verifying the branch
        // in the assertion below.
        assert!(L::is_valid_snowflake(last_snowflake));
        // End skip branch coverage
        #[cfg(feature = "blocking")]
        {
            let mut guard = self.last_snowflake_blocking.lock().unwrap();
            *guard = last_snowflake;
        }

        #[cfg(feature = "lock-free")]
        {
            self.last_snowflake_atomic
                .store(last_snowflake, atomic::Ordering::Release);
        }
    }
}

impl<L, E> Default for Generator<L, E>
where
    L: Layout,
    E: Epoch,
{
    /// Returns a new generator that has generated a single snowflake in the first millisecond of its epoch.
    ///
    /// If the system clock is in the same millisecond as your epoch, the first snowflake that this generator creates
    /// will have the sequence number `1`. While this doesn't affect most users (with a fixed epoch in the past), this
    /// is something to keep in mind if your code relies on the guarantee that there aren't any "holes" in the returned
    /// snowflakes' sequence numbers.
    fn default() -> Self {
        Self {
            #[cfg(feature = "blocking")]
            last_snowflake_blocking: Arc::new(Mutex::new(0)),
            #[cfg(feature = "lock-free")]
            last_snowflake_atomic: Arc::new(AtomicU64::new(0)),
            _marker: PhantomData,
        }
    }
}

// Skip coverage: We don't test the coverage of our unit tests.
#[cfg(test)]
mod generator_tests {
    use crate::system_time_mock::SystemTime;
    use crate::{Epoch, Error, Generator, Layout, Result, Snowflake};
    use std::collections::{HashSet, VecDeque};

    #[derive(Debug)]
    struct SimpleSnowflakeParams;

    impl Layout for SimpleSnowflakeParams {
        fn construct_snowflake(timestamp: u64, sequence_number: u64) -> u64 {
            assert!(!Self::exceeds_timestamp(timestamp) && !Self::exceeds_sequence_number(sequence_number));
            timestamp << 12 | sequence_number
        }
        fn get_timestamp(input: u64) -> u64 {
            input >> 12
        }
        fn exceeds_timestamp(input: u64) -> bool {
            input >= (1 << 51)
        }
        fn get_sequence_number(input: u64) -> u64 {
            input & ((1 << 12) - 1)
        }
        fn exceeds_sequence_number(input: u64) -> bool {
            input >= (1 << 12)
        }
        fn is_valid_snowflake(input: u64) -> bool {
            input >> 63 == 0
        }
    }

    impl Epoch for SimpleSnowflakeParams {
        fn millis_since_unix() -> u64 {
            // Return the first millisecond of 2023
            1672531200000
        }
    }

    type SimpleSnowflake = Snowflake<SimpleSnowflakeParams, SimpleSnowflakeParams>;
    type SimpleSnowflakeGenerator = Generator<SimpleSnowflakeParams, SimpleSnowflakeParams>;

    macro_rules! impl_test {
        ($test_impl:ident, $test:ident, $test_blocking:ident, $test_lock_free:ident) => {
            #[test]
            #[cfg(any(
                all(feature = "blocking", not(feature = "lock-free")),
                all(feature = "lock-free", not(feature = "blocking"))
            ))]
            fn $test() {
                let _g = SystemTime::acquire_lock();
                $test_impl(Generator::generate);
            }
            #[test]
            #[cfg(feature = "blocking")]
            fn $test_blocking() {
                let _g = SystemTime::acquire_lock();
                $test_impl(Generator::generate_blocking);
            }
            #[test]
            #[cfg(feature = "lock-free")]
            fn $test_lock_free() {
                let _g = SystemTime::acquire_lock();
                $test_impl(Generator::generate_lock_free);
            }
        };
    }

    // Naively test whether a single generator returns unique IDs.
    impl_test!(test_unique, unique, unique_blocking, unique_lock_fre);

    fn test_unique(generator: fn(&SimpleSnowflakeGenerator) -> Result<SimpleSnowflake>) {
        // Set the time to the second millisecond of 2023
        SystemTime::set_time(1672531200001);
        let gen = SimpleSnowflakeGenerator::default();
        let mut snowflakes: HashSet<_> = (0..10).map(|_| generator(&gen).unwrap()).collect();
        // Increase the time by one millisecond
        SystemTime::set_time(1672531200002);
        for _ in 0..10 {
            snowflakes.insert(generator(&gen).unwrap());
        }
        assert_eq!(20, snowflakes.len());
    }

    impl_test!(test_monotonic, monotonic, monotonic_blocking, monotonic_lock_free);

    fn test_monotonic(generator: fn(&SimpleSnowflakeGenerator) -> Result<SimpleSnowflake>) {
        SystemTime::set_time(1672531200001);
        let gen = SimpleSnowflakeGenerator::default();
        let mut snowflakes: VecDeque<_> = (0..10).map(|_| generator(&gen).unwrap()).collect();
        SystemTime::set_time(1672531200002);
        for _ in 0..10 {
            snowflakes.push_back(generator(&gen).unwrap());
        }
        let mut previous = snowflakes.pop_front().unwrap();
        for snowflake in snowflakes {
            assert!(previous < snowflake);
            previous = snowflake;
        }
    }

    impl_test!(
        test_snowflake_exhaustion,
        snowflake_exhaustion,
        snowflake_exhaustion_blocking,
        snowflake_exhaustion_lock_free
    );

    fn test_snowflake_exhaustion(generator: fn(&SimpleSnowflakeGenerator) -> Result<SimpleSnowflake>) {
        SystemTime::set_time(1672531200001);
        let gen = SimpleSnowflakeGenerator::default();
        // Generate 4096 snowflakes to exhaust our sequence number - there are more efficient ways to do this, but this
        // is the only (real) way the sequence number will fill up.
        for _ in 0..(1 << 12) {
            match generator(&gen) {
                Err(Error::FatalSnowflakeExhaustion) => {
                    panic!("fatal snowflake exhaustion without being limited by the epoch");
                }
                Err(Error::SnowflakeExhaustion) => {
                    panic!("snowflakes exhausted without being limited by the sequence number");
                }
                Err(_) => {
                    panic!("unexpected error while generating a snowflake");
                }
                Ok(_) => {}
            }
        }
        // This generator call should fail
        let result = generator(&gen);
        assert_eq!(Error::SnowflakeExhaustion, result.unwrap_err());

        SystemTime::set_time(1672531200002);
        let _ = generator(&gen).unwrap();
    }

    // Admittedly, this test won't affect 99% of our users, and if our code would fail here, it's most likely because of
    // a bug in the user's code, as they've specified a snowflake layout that doesn't fulfill our requirements.
    #[test]
    fn malicious_snowflake_exhaustion() {
        let _g = SystemTime::acquire_lock();
        // Specify an "illegal" snowflake format that doesn't include a timestamp
        #[derive(Debug)]
        struct Bad;

        impl Layout for Bad {
            fn construct_snowflake(_timestamp: u64, sequence_number: u64) -> u64 {
                sequence_number
            }
            fn get_timestamp(_input: u64) -> u64 {
                0
            }
            fn exceeds_timestamp(_input: u64) -> bool {
                // Lie about the timestamp
                false
            }
            fn get_sequence_number(input: u64) -> u64 {
                input
            }
            fn exceeds_sequence_number(_input: u64) -> bool {
                // Technically, this implementation is valid. If the value overflows, we can't detect this here.
                false
            }
            fn is_valid_snowflake(_input: u64) -> bool {
                true
            }
        }

        SystemTime::set_time(1672531200000);
        type BadGen = Generator<Bad, SimpleSnowflakeParams>;
        let gen = BadGen::default();
        gen.set_last_snowflake(u64::MAX);
        #[cfg(any(
            all(feature = "blocking", not(feature = "lock-free")),
            all(feature = "lock-free", not(feature = "blocking"))
        ))]
        assert_eq!(Error::SnowflakeExhaustion, BadGen::generate(&gen).unwrap_err());
        #[cfg(feature = "blocking")]
        assert_eq!(Error::SnowflakeExhaustion, BadGen::generate_blocking(&gen).unwrap_err());
        #[cfg(feature = "lock-free")]
        assert_eq!(
            Error::SnowflakeExhaustion,
            BadGen::generate_lock_free(&gen).unwrap_err()
        );
    }

    #[test]
    fn test_extreme_epoch() {
        let _g = SystemTime::acquire_lock();
        #[derive(Debug)]
        struct BadEpoch;

        impl Epoch for BadEpoch {
            fn millis_since_unix() -> u64 {
                u64::MAX
            }
        }

        type BadGen = Generator<SimpleSnowflakeParams, BadEpoch>;

        SystemTime::set_time(u64::MAX);
        let gen = BadGen::default();
        #[cfg(any(
            all(feature = "blocking", not(feature = "lock-free")),
            all(feature = "lock-free", not(feature = "blocking"))
        ))]
        assert_eq!(Error::FatalSnowflakeExhaustion, gen.generate().unwrap_err());
        #[cfg(feature = "blocking")]
        assert_eq!(Error::FatalSnowflakeExhaustion, gen.generate_blocking().unwrap_err());
        #[cfg(feature = "lock-free")]
        assert_eq!(Error::FatalSnowflakeExhaustion, gen.generate_lock_free().unwrap_err());
    }

    impl_test!(
        test_non_monotonic_clock,
        non_monotonic_clock,
        non_monotonic_clock_blocking,
        non_monotonic_clock_lock_free
    );

    fn test_non_monotonic_clock(generator: fn(&SimpleSnowflakeGenerator) -> Result<SimpleSnowflake>) {
        SystemTime::set_time(1672531200002);
        let gen = SimpleSnowflakeGenerator::default();
        let before = generator(&gen).unwrap();

        // Simulate a non-monotonic clock
        SystemTime::set_time(1672531200001);
        let result = generator(&gen);
        assert_eq!(
            Error::NonMonotonicClock,
            result.expect_err("non-monotonic clock not detected")
        );

        // The generator should work again once we increase the time again
        SystemTime::set_time(1672531200002);
        let after = generator(&gen).expect("snowflake generation failed after clock was adjusted back");
        assert!(before < after);

        SystemTime::set_time(1672531200003);
        let _ = generator(&gen).unwrap();
    }

    impl_test!(
        test_invalid_epoch,
        invalid_epoch,
        invalid_epoch_blocking,
        invalid_epoch_lock_free
    );

    fn test_invalid_epoch(generator: fn(&SimpleSnowflakeGenerator) -> Result<SimpleSnowflake>) {
        // Set the time so that our epoch is in the future
        SystemTime::set_time(1672531199999);
        let gen = SimpleSnowflakeGenerator::default();
        assert_eq!(Error::InvalidEpoch, generator(&gen).unwrap_err());
        // Set the time to the same millisecond as the generator
        SystemTime::set_time(1672531200000);
        assert_eq!(
            Snowflake::from_raw(1).unwrap(),
            generator(&gen).expect("generator failed to generate a snowflake after the")
        );
        // Verify that the generator still works after the epoch's millisecond
        assert!(generator(&gen).is_ok());
    }

    impl_test!(
        test_type_limits,
        type_limits,
        type_limits_blocking,
        type_limits_lock_free
    );

    fn test_type_limits(generator: fn(&SimpleSnowflakeGenerator) -> Result<SimpleSnowflake>) {
        SystemTime::set_time(1672531200000 + (1 << 51));
        let gen = SimpleSnowflakeGenerator::default();
        assert_eq!(Error::FatalSnowflakeExhaustion, generator(&gen).unwrap_err());
        SystemTime::set_time(u64::MAX);
        assert_eq!(Error::FatalSnowflakeExhaustion, generator(&gen).unwrap_err());
    }
}
// End skip coverage

/// A trait specifying the composition of a snowflake.
///
/// This trait specifies how a snowflake is constructed, whether it supports a given timestamp or sequence number, and
/// how to extract those details from an integer representation of a snowflake.
///
/// If you're looking for the classic layout implementation introduced by Twitter, refer to [`ClassicLayout`]. If you
/// want to implement a different layout implementation yourself, refer to the individual functions' documentation for
/// examples and the exact requirements.
pub trait Layout {
    /// Combines the given timestamp and sequence number into a 64-bit snowflake integer.
    ///
    /// # Guarantees for parameters
    ///
    /// When calling this function, you must verify that neither [`exceeds_timestamp`](Self::exceeds_timestamp) nor
    /// [`exceeds_sequence_number`](Self::exceeds_sequence_number) return `true` for the respective parameters.
    /// Implementations of this function *might* panic if the input can't be stored in the layout they implement.
    /// Alternatively, these functions *might* construct incorrect snowflakes if a parameter exceeds the maximum
    /// supported by their layout.
    ///
    /// Especially if an implementation returns incorrect snowflakes in those cases, it's advised not to call this code
    /// yourself. If you need to construct a snowflake with a custom timestamp to compare it to other snowflakes, check
    /// out [`SnowflakeComparator`] instead. In addition to providing a constructor that directly takes a [`SystemTime`]
    /// instance, this separate type makes sure raw timestamps can't be accidentally used as "real" snowflake IDs in
    /// APIs that expect a unique ID.
    ///
    /// # General layout of a snowflake
    ///
    /// Your generated snowflake **must** store the given timestamp *in front of* the sequence number. Moreover, all
    /// other parts of a snowflake **must** be constant across the snowflake generating instance. Layouts not following
    /// these requirements *will* produce incorrect snowflakes. E.g., if you store the timestamp after the sequence
    /// number, the resulting snowflakes will no longer be monotonic.
    ///
    /// # Instance-wide constant data
    ///
    /// This method is used to construct snowflakes in a [`Generator`]. If your snowflake layout includes
    /// (instance-wide) constant data like machine IDs or a leading 0, you must include this here. As this is an
    /// associated function rather than a method, there's no instance of this layout that you could store constants in.
    /// Instead, you'll have to obtain this data from an instance-wide global state. Considering that this state must
    /// remain constant across the snowflake generating instance, however, this shouldn't be a problem.
    ///
    /// # Example
    ///
    /// The example below constructs the classic snowflake layout introduced by Twitter. Specifically, the first bit of
    /// this layout is guaranteed to be a constant `0`, and the snowflake uses 41 bits for the timestamp, 10 bits for
    /// the machine ID, and 12 bits for the sequence number.
    ///
    /// ```
    /// use snowdon::Layout;
    ///
    /// struct MyLayout;
    /// # fn get_machine_id() -> u64 { 0 }
    ///
    /// impl Layout for MyLayout {
    ///     fn construct_snowflake(timestamp: u64, sequence_number: u64) -> u64 {
    ///         assert!(
    ///             !Self::exceeds_timestamp(timestamp)
    ///                 && !Self::exceeds_sequence_number(sequence_number)
    ///         );
    ///         // `exceeds_timestamp` enforces that the timestamp is at most 41 bits,
    ///         // so the first bit will always be 0
    ///         (timestamp << 22) | (get_machine_id() << 12) | sequence_number
    ///     }
    ///
    ///     // ...
    /// #   fn get_timestamp(input: u64) -> u64 {
    /// #       unimplemented!()
    /// #   }
    /// #   fn exceeds_timestamp(input: u64) -> bool {
    /// #       false
    /// #   }
    /// #   fn get_sequence_number(input: u64) -> u64 {
    /// #       unimplemented!()
    /// #   }
    /// #   fn exceeds_sequence_number(input: u64) -> bool {
    /// #       false
    /// #   }
    /// #   fn is_valid_snowflake(input: u64) -> bool {
    /// #       unimplemented!()
    /// #   }
    /// }
    ///
    /// // For this example, get_machine_id always returns 0
    /// assert_eq!(0x400002, MyLayout::construct_snowflake(1, 2));
    /// ```
    fn construct_snowflake(timestamp: u64, sequence_number: u64) -> u64;
    /// Returns the timestamp stored in the given snowflake.
    ///
    /// The returned timestamp is represented as the number of milliseconds since the first millisecond of the
    /// snowflake's epoch. Most importantly, it's *not* the number of milliseconds since the Unix epoch unless the
    /// snowflake uses this epoch.
    ///
    /// Instead of using this function directly, you should use [`Snowflake::get_timestamp()`] to retrieve a snowflake's
    /// timestamp.
    ///
    /// # Example
    ///
    /// The example below extracts the 41-bit timestamp from a snowflake using the classic layout introduced by Twitter.
    ///
    /// ```
    /// use snowdon::Layout;
    ///
    /// struct MyLayout;
    ///
    /// impl Layout for MyLayout {
    /// #   fn construct_snowflake(timestamp: u64, sequence_number: u64) -> u64 {
    /// #       unimplemented!()
    /// #   }
    ///     fn get_timestamp(input: u64) -> u64 {
    ///         // The first bit is guaranteed to be a 0, so we don't have to remove this constant part here
    ///         input >> 22
    ///     }
    ///
    ///     //...
    /// #   fn exceeds_timestamp(input: u64) -> bool {
    /// #       unimplemented!()
    /// #   }
    /// #   fn get_sequence_number(input: u64) -> u64 {
    /// #       unimplemented!()
    /// #   }
    /// #   fn exceeds_sequence_number(input: u64) -> bool {
    /// #       unimplemented!()
    /// #   }
    /// #   fn is_valid_snowflake(input: u64) -> bool {
    /// #       unimplemented!()
    /// #   }
    /// }
    ///
    /// assert_eq!(1, MyLayout::get_timestamp(0x400002));
    /// ```
    fn get_timestamp(input: u64) -> u64;
    /// Returns whether the given timestamp exceeds the number of bits dedicated to timestamps in this layout.
    ///
    /// If this function returns `false`, the given timestamp must fit inside a snowflake without losing or overwriting
    /// any bits.
    ///
    /// You generally don't need to call this function yourself. [Generators](Generator) already check whether the
    /// current timestamp fits into the layout when generating a new snowflake.
    ///
    /// # Example
    ///
    /// The example below implements this function for the classic layout introduced by Twitter.
    ///
    /// ```
    /// use snowdon::Layout;
    ///
    /// struct MyLayout;
    ///
    /// impl Layout for MyLayout {
    /// #   fn construct_snowflake(timestamp: u64, sequence_number: u64) -> u64 {
    /// #       unimplemented!()
    /// #   }
    /// #   fn get_timestamp(input: u64) -> u64 {
    /// #       unimplemented!()
    /// #   }
    ///     fn exceeds_timestamp(input: u64) -> bool {
    ///         input >= 1 << 41
    ///     }
    ///
    ///     // ...
    /// #   fn get_sequence_number(input: u64) -> u64 {
    /// #       unimplemented!()
    /// #   }
    /// #   fn exceeds_sequence_number(input: u64) -> bool {
    /// #       unimplemented!()
    /// #   }
    /// #   fn is_valid_snowflake(input: u64) -> bool {
    /// #       unimplemented!()
    /// #   }
    /// }
    ///
    /// assert!(!MyLayout::exceeds_timestamp(0x1FFFFFFFFFF));
    /// assert!(MyLayout::exceeds_timestamp(0x20000000000));
    /// ```
    fn exceeds_timestamp(input: u64) -> bool;
    /// Returns the sequence number of the given snowflake.
    ///
    /// With Snowdon's guarantee that there are no "holes" in sequence numbers generated by [`Generator`], a sequence
    /// number of `n` means that this is the `n + 1`th snowflake generated for the same timestamp.
    ///
    /// Instead of using this function directly, you should use [`Snowflake::get_sequence_number`] to retrieve the
    /// sequence number of a snowflake.
    ///
    /// # Example
    ///
    /// ```
    /// use snowdon::Layout;
    ///
    /// struct MyLayout;
    ///
    /// impl Layout for MyLayout {
    /// #   fn construct_snowflake(timestamp: u64, sequence_number: u64) -> u64 {
    /// #       unimplemented!()
    /// #   }
    /// #   fn get_timestamp(input: u64) -> u64 {
    /// #       unimplemented!()
    /// #   }
    /// #   fn exceeds_timestamp(input: u64) -> bool {
    /// #       unimplemented!()
    /// #   }
    ///     fn get_sequence_number(input: u64) -> u64 {
    ///         input & ((1 << 12) - 1)
    ///     }
    ///
    ///     // ...
    /// #   fn exceeds_sequence_number(input: u64) -> bool {
    /// #       unimplemented!()
    /// #   }
    /// #   fn is_valid_snowflake(input: u64) -> bool {
    /// #       unimplemented!()
    /// #   }
    /// }
    ///
    /// assert_eq!(2, MyLayout::get_sequence_number(0x400002));
    /// ```
    fn get_sequence_number(input: u64) -> u64;
    /// Returns whether the given sequence number exceeds the number of bits dedicated in this layout.
    ///
    /// If this function returns `false`, the given sequence number must fit inside a snowflake without losing or
    /// overwriting any bits.
    ///
    /// You generally don't need to call this function yourself. [Generators](Generator) already check whether the
    /// current sequence number fits into the layout when generating a new snowflake.
    ///
    /// # Example
    ///
    /// The example below implements this function for the classic layout introduced by Twitter.
    ///
    /// ```
    /// use snowdon::Layout;
    ///
    /// struct MyLayout;
    ///
    /// impl Layout for MyLayout {
    /// #   fn construct_snowflake(timestamp: u64, sequence_number: u64) -> u64 {
    /// #       unimplemented!()
    /// #   }
    /// #   fn get_timestamp(input: u64) -> u64 {
    /// #       unimplemented!()
    /// #   }
    /// #   fn exceeds_timestamp(input: u64) -> bool {
    /// #       unimplemented!()
    /// #   }
    /// #   fn get_sequence_number(input: u64) -> u64 {
    /// #       unimplemented!()
    /// #   }
    ///     fn exceeds_sequence_number(input: u64) -> bool {
    ///         input >= 1 << 12
    ///     }
    ///
    ///     // ...
    /// #   fn is_valid_snowflake(input: u64) -> bool {
    /// #       unimplemented!()
    /// #   }
    /// }
    ///
    /// assert!(!MyLayout::exceeds_sequence_number(0xFFF));
    /// assert!(MyLayout::exceeds_sequence_number(0x1000));
    /// ```
    fn exceeds_sequence_number(input: u64) -> bool;
    /// Returns whether the given snowflake is a valid integer representation of a snowflake.
    ///
    /// If your snowflake layout contains constant parts that are constant *throughout the entire deployment* (like a
    /// leading 0), you should verify those parts here. However, you should **not** check whether the snowflake was
    /// generated by this instance (e.g. by checking that the instance ID is the same as this instance's ID). This
    /// function is meant to check whether the snowflake could be a valid snowflake for the entire deployment.
    ///
    /// If your snowflake format doesn't include any constant parts or all constant parts are instance-specific (like
    /// the snowflake layout used by Discord), it's OK to simply always return `true` here.
    ///
    /// Usually, you don't have to call this function yourself. Using [`Snowflake::from_raw`] will automatically call
    /// this function and return an error if the given snowflake is invalid.
    ///
    /// # Example
    ///
    /// ```
    /// use snowdon::Layout;
    ///
    /// struct MyLayout;
    ///
    /// impl Layout for MyLayout {
    /// #   fn construct_snowflake(timestamp: u64, sequence_number: u64) -> u64 {
    /// #       unimplemented!()
    /// #   }
    /// #   fn get_timestamp(input: u64) -> u64 {
    /// #       unimplemented!()
    /// #   }
    /// #   fn exceeds_timestamp(input: u64) -> bool {
    /// #       unimplemented!()
    /// #   }
    /// #   fn get_sequence_number(input: u64) -> u64 {
    /// #       unimplemented!()
    /// #   }
    /// #   fn exceeds_sequence_number(input: u64) -> bool {
    /// #       unimplemented!()
    /// #   }
    ///     fn is_valid_snowflake(input: u64) -> bool {
    ///         input < 1 << 63
    ///     }
    ///
    ///     // ...
    /// }
    ///
    /// assert!(MyLayout::is_valid_snowflake(0x7FFFFFFFFFFFFFFF));
    /// assert!(!MyLayout::is_valid_snowflake(0x8000000000000000));
    /// ```
    fn is_valid_snowflake(input: u64) -> bool;
}

/// A trait that defines a [`Snowflake`]'s constant epoch.
///
/// This trait only requires one associated function that returns the epoch's timestamp in milliseconds since the Unix
/// epoch. Specifically, this means that you can't access any instance to retrieve the epoch. This is an intentional
/// design decision, as this discourages dynamic epoch specifications. The epoch returned by this trait's associated
/// function **must** be constant throughout the application. Even if you make the epoch configurable, you *must* always
/// return the same value here to ensure that snowflakes generated by Snowdon are compatible with each other.
///
/// Note that epochs may not precede the Unix epoch. I.e., your epoch can't be before the first millisecond of 1970
/// (UTC).
///
/// # Example
///
/// Implementing this trait is usually pretty straightforward. The example below implements an epoch starting with the
/// first millisecond of 2015:
///
/// ```
/// use snowdon::Epoch;
/// struct MyEpoch;
///
/// impl Epoch for MyEpoch {
///     fn millis_since_unix() -> u64 {
///         // Return the first millisecond of 2015 (UTC)
///         1420070400000
///     }
/// }
/// ```
pub trait Epoch {
    /// Returns the number of milliseconds since the Unix epoch.
    ///
    /// The returned epoch must remain constant throughout the application's runtime. Moreover, the epoch must not
    /// precede the Unix epoch.
    ///
    /// Instead of using this function to interpret a timestamp extracted from a snowflake, you should use
    /// [`Snowflake::get_timestamp`] instead. This method also detects potential problems that can occur if the
    /// timestamp can't be represented with [`SystemTime`]'s underlying data type.
    ///
    /// Refer to the [trait's documentation](Self) for an example implementation.
    fn millis_since_unix() -> u64;
}

/// A snowflake ID.
///
/// Snowflakes are essentially sequence numbers associated with timestamps. They are meant to provide IDs that are
/// * unique (no snowflake will be generated twice)
/// * and monotonic (a snowflake `a` that precedes the snowflake `b` will be smaller: `a < b`).
///
/// With a custom [layout](Layout), users can easily extend their snowflake format to also include a machine ID. This
/// makes snowflakes very useful for distributed computing, as generators can provide IDs that are *unique* without any
/// coordination between the ID-generating instances.
///
/// # Layouts
///
/// Note that the `Snowflake` type in Snowdon is meant to support various kinds of snowflake layouts. Specifically, we
/// only require that snowflakes contain a timestamp and a sequence number in that order. Other (constant) parts may
/// interleave as described in [`Layout::construct_snowflake`]. Every other detail of snowflakes is realized by the
/// snowflake's `Layout` and `Epoch` implementations. If you're looking for the original layout introduced by Twitter,
/// you can use [`ClassicLayout`]. Refer to its documentation for requirements and details on how to use this layout for
/// your snowflakes.
///
/// # Using snowflakes in your API
///
/// To make it harder to misuse snowflakes (e.g. by using a snowflake with an API that expects a different layout or
/// epoch), snowflakes have two type parameters. To avoid having to specify them everytime you're using them in your
/// API, you should consider specifying a type as explained in the example below.
///
/// # Example
///
/// The example below outlines how to use the `ClassicLayout` together with a custom epoch (the first second of 2015) to
/// define your application's snowflake type. For examples implementing custom layouts, refer to [`Layout`]'s individual
/// function documentations.
///
/// Note that the implementation below doesn't match Discord's use of snowflakes. Instead of only dedicating 41 bits to
/// the timestamp in a snowflake, Discord uses the full remaining 42 bits. This means that the first bit isn't
/// guaranteed to be `0`, and you can't use the `ClassicLayout` to implement Discord's snowflake IDs.
///
/// ```
/// // Create a "parameter" zero-sized type that implements our epoch and machine
/// // ID. You might want to make this type hidden in the documentation, as it's
/// // an implementation detail usually not needed by the users of your snowflake
/// // format.
/// use snowdon::{ClassicLayout, Epoch, Generator, MachineId, Snowflake};
/// #[doc(hidden)]
/// pub struct SnowflakeParameters;
///
/// impl MachineId for SnowflakeParameters {
///     fn machine_id() -> u64 {
///         // Determine the machine ID - this step depends on your application and
///         // the information available to the individual instances. Things you
///         // might want to consider to derive this ID are private IP addresses or
///         // an ID assigned by a third-party service.
///         0
///     }
/// }
///
/// impl Epoch for SnowflakeParameters {
///     fn millis_since_unix() -> u64 {
///         // Return the first millisecond of 2015
///         1420070400000
///     }
/// }
///
/// // Specify our custom snowflake type and a generator that produces it
/// pub type AwesomeSnowflake =
///     Snowflake<ClassicLayout<SnowflakeParameters>, SnowflakeParameters>;
/// pub type AwesomeSnowflakeGenerator =
///     Generator<ClassicLayout<SnowflakeParameters>, SnowflakeParameters>;
///
/// // Finally, use our custom type in our application's API
/// pub struct Message {
///     id: AwesomeSnowflake,
///     author: String,
///     content: String,
/// }
///
/// impl Message {
///     // ...
///     pub fn get_id(&self) -> AwesomeSnowflake {
///         // Snowflakes implement Copy, so we can easily return this without
///         // cloning it manually
///         self.id
///     }
///
///     pub fn set_id(&mut self, id: AwesomeSnowflake) {
///         // The type parameters of snowflakes ensure that the snowflake `id` uses
///         // the same layout *and* epoch as our existing snowflake. Moreover,
///         // users can't accidentally use a (non-unique) SnowflakeComparator here.
///         self.id = id;
///     }
/// }
/// ```
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[repr(transparent)]
pub struct Snowflake<L, E>
where
    L: Layout,
    E: Epoch,
{
    inner: u64,
    // Skip coverage: This is a ZST. It's, by definition, going to disappear at compile time, so we can't get any
    // coverage for this.
    _marker: PhantomData<(L, E)>,
    // End skip coverage
}

impl<L, E> Snowflake<L, E>
where
    L: Layout,
    E: Epoch,
{
    /// Returns the snowflake for the given integer representation.
    ///
    /// If this snowflake's layout requires certain constant parts in every snowflake that can be used to detect invalid
    /// snowflakes (like a leading 0), this function returns `Error::InvalidSnowflake` instead if the provided integer
    /// representation is invalid.
    pub fn from_raw(input: u64) -> Result<Self> {
        if !L::is_valid_snowflake(input) {
            return Err(Error::InvalidSnowflake);
        }
        Ok(Self {
            inner: input,
            _marker: PhantomData,
        })
    }

    /// Returns the integer representation of this snowflake.
    ///
    /// Depending on the snowflake's [`Layout`], this integer could maintain (some of) its properties when cast to
    /// smaller or signed integers.
    #[inline]
    pub fn get(&self) -> u64 {
        self.inner
    }

    /// Returns the timestamp of this snowflake's birth.
    ///
    /// If the time can't be represented with a `SystemTime` instance, [`Error::FatalSnowflakeExhaustion`] is returned
    /// instead. With most snowflake layouts and epochs, it should be impossible to store such a timestamp in the
    /// snowflake, though.
    ///
    /// If you're looking for the raw number of milliseconds since the snowflake's epoch (or an infallible way to
    /// retrieve a snowflake's timestamp), you should use [`get_timestamp_raw`](Self::get_timestamp_raw).
    pub fn get_timestamp(&self) -> Result<SystemTime> {
        SystemTime::UNIX_EPOCH
            .checked_add(Duration::from_millis(
                E::millis_since_unix()
                    .checked_add(L::get_timestamp(self.inner))
                    .ok_or(Error::FatalSnowflakeExhaustion)?,
            ))
            .ok_or(Error::FatalSnowflakeExhaustion)
    }

    /// Returns the number of milliseconds since this snowflake's epoch.
    ///
    /// If you're looking for a [`SystemTime`] instance, you should use [`get_timestamp`](Self::get_timestamp) instead.
    #[inline]
    pub fn get_timestamp_raw(&self) -> u64 {
        L::get_timestamp(self.inner)
    }

    /// Returns this snowflake's sequence number.
    ///
    /// With snowdon's guarantee that there are no "holes" in this number, a sequence number of `n` means that this is
    /// the `n + 1`th snowflake generated for this timestamp.
    #[inline]
    pub fn get_sequence_number(&self) -> u64 {
        L::get_sequence_number(self.inner)
    }

    /// Returns a [`SnowflakeComparator`] for this snowflake's timestamp.
    ///
    /// If the comparator's underlying data type can't represent this snowflake's timestamp, this returns an
    /// [`Error::FatalSnowflakeExhaustion`] instead. As explained in [`SnowflakeComparator`'s
    /// documentation](SnowflakeComparator#limitations), you should be able to unwrap this method's result if your code
    /// doesn't need to deal with timestamps this large.
    pub fn get_comparator(&self) -> Result<SnowflakeComparator> {
        SnowflakeComparator::from_timestamp::<E>(L::get_timestamp(self.inner))
    }
}

impl<L, E> Clone for Snowflake<L, E>
where
    L: Layout,
    E: Epoch,
{
    fn clone(&self) -> Self {
        *self
    }
}

impl<L, E> Copy for Snowflake<L, E>
where
    L: Layout,
    E: Epoch,
{
}

impl<L, E> Display for Snowflake<L, E>
where
    L: Layout,
    E: Epoch,
{
    /// Displays the snowflake as a decimal-encoded integer.
    ///
    /// You can losslessly convert this method's output back into the same snowflake.
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl<L, E> PartialEq for Snowflake<L, E>
where
    L: Layout,
    E: Epoch,
{
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl<L, E> Eq for Snowflake<L, E>
where
    L: Layout,
    E: Epoch,
{
}

impl<L, E> Hash for Snowflake<L, E>
where
    L: Layout,
    E: Epoch,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.inner.hash(state);
    }
}

impl<L, E> PartialOrd for Snowflake<L, E>
where
    L: Layout,
    E: Epoch,
{
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<L, E> Ord for Snowflake<L, E>
where
    L: Layout,
    E: Epoch,
{
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.inner.cmp(&other.inner)
    }
}

// Skip coverage: We don't test the coverage of our unit tests
#[cfg(test)]
mod snowflake_tests {
    use crate::system_time_mock::SystemTime;
    use crate::{Epoch, Error, Layout, Snowflake};
    use std::cmp::{max, max_by, min, min_by, Ordering};
    use std::time::Duration;

    #[derive(Debug)]
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
            input >= 1 << 41
        }
        fn get_sequence_number(input: u64) -> u64 {
            input & ((1 << 12) - 1)
        }
        fn exceeds_sequence_number(input: u64) -> bool {
            input >= 1 << 12
        }
        fn is_valid_snowflake(input: u64) -> bool {
            input >> 63 == 0
        }
    }

    impl Epoch for SimpleParams {
        fn millis_since_unix() -> u64 {
            0
        }
    }

    type SimpleSnowflake = Snowflake<SimpleParams, SimpleParams>;

    #[test]
    fn invalid_from_raw() {
        assert_eq!(
            Error::InvalidSnowflake,
            SimpleSnowflake::from_raw(1 << 63).unwrap_err(),
            "`Snowflake::from_raw` allowed creating an invalid snowflake"
        );
        assert_eq!(
            (1 << 63) - 1,
            SimpleSnowflake::from_raw((1 << 63) - 1)
                .expect("`Snowflake::from_raw` rejected a valid snowflake")
                .get(),
            "`Snowflake::from_raw` produced an unrelated snowflake"
        );
    }

    #[test]
    fn get_timestamp() {
        let snowflake = SimpleSnowflake::from_raw(123 << 22).unwrap();
        assert_eq!(
            SystemTime::UNIX_EPOCH.checked_add(Duration::from_millis(123)).unwrap(),
            snowflake
                .get_timestamp()
                .expect("snowflake failed to return its timestamp"),
            "snowflake returned an unrelated timestamp"
        );
        assert_eq!(
            123,
            snowflake.get_timestamp_raw(),
            "snowflake returned an unrelated raw timestamp"
        );
    }

    #[test]
    fn get_timestamp_custom_epoch() {
        struct CustomEpoch;

        const EPOCH: u64 = 1024;

        impl Epoch for CustomEpoch {
            fn millis_since_unix() -> u64 {
                EPOCH
            }
        }

        let snowflake = Snowflake::<SimpleParams, CustomEpoch>::from_raw(234 << 22)
            .expect("failed to create snowflake with custom epoch");
        assert_eq!(
            SystemTime::UNIX_EPOCH
                .checked_add(Duration::from_millis(234 + EPOCH))
                .unwrap(),
            snowflake
                .get_timestamp()
                .expect("snowflake failed to return its timestamp with a custom epoch"),
            "snowflake didn't use the custom epoch to return a timestamp or returned an otherwise unrelated timestamp"
        );
        assert_eq!(
            234,
            snowflake.get_timestamp_raw(),
            "snowflake returned an invalid raw timestamp (possibly converting it to another epoch)"
        );
    }

    #[test]
    fn get_sequence_number() {
        assert_eq!(
            0,
            SimpleSnowflake::from_raw(0).unwrap().get_sequence_number(),
            "snowflake with sequence number 0 returned invalid sequence number"
        );
        assert_eq!(
            (1 << 12) - 1,
            SimpleSnowflake::from_raw((1 << 12) - 1).unwrap().get_sequence_number(),
            "snowflake with max sequence number returned invalid sequence number"
        );
        assert_eq!(
            0,
            SimpleSnowflake::from_raw(1 << 22).unwrap().get_sequence_number(),
            "snowflake failed to return sequence number 0 with a non-zero timestamp"
        );
        assert_eq!(
            (1 << 12) - 1,
            SimpleSnowflake::from_raw(1 << 22 | ((1 << 12) - 1))
                .unwrap()
                .get_sequence_number(),
            "snowflake failed to return max sequence number with a non-zero timestamp"
        );
    }

    #[test]
    fn comparators() {
        let (foo, bar, baz) = (
            SimpleSnowflake::from_raw(0).unwrap(),
            SimpleSnowflake::from_raw(1 << 22).unwrap(),
            SimpleSnowflake::from_raw(2 << 22).unwrap(),
        );
        assert!(foo < bar && bar < baz);
        let (foo, bar, baz) = (
            foo.get_comparator().unwrap(),
            bar.get_comparator().unwrap(),
            baz.get_comparator().unwrap(),
        );
        assert!(foo < bar && bar < baz, "snowflakes returned unrelated comparators");
    }

    #[test]
    #[allow(clippy::nonminimal_bool)]
    fn ord() {
        for timestamp in 0..1 {
            let foo = SimpleSnowflake::from_raw(timestamp << 22 | 2).unwrap();
            let (smaller, equal, greater) = (
                SimpleSnowflake::from_raw(timestamp << 22 | 1).unwrap(),
                SimpleSnowflake::from_raw(timestamp << 22 | 2).unwrap(),
                SimpleSnowflake::from_raw(timestamp << 22 | 3).unwrap(),
            );
            // PartialEq
            assert_ne!(foo, smaller);
            assert!(!(foo == smaller));
            assert_eq!(foo, equal);
            assert!(!(foo != equal));
            assert_ne!(foo, greater);
            assert!(!(foo == greater));

            // Eq
            assert_eq!(foo, foo);
            assert_eq!(foo, equal);
            assert_eq!(equal, foo);
            // We don't check transitivity here, as the test would be trivial

            // PartialOrd
            assert_ne!(Ordering::Equal, PartialOrd::partial_cmp(&foo, &smaller).unwrap());
            assert_eq!(Ordering::Equal, PartialOrd::partial_cmp(&foo, &equal).unwrap());
            assert_ne!(Ordering::Equal, PartialOrd::partial_cmp(&foo, &greater).unwrap());
            assert_eq!(Ordering::Less, PartialOrd::partial_cmp(&smaller, &foo).unwrap());
            assert_ne!(Ordering::Less, PartialOrd::partial_cmp(&foo, &equal).unwrap());
            assert_eq!(Ordering::Less, PartialOrd::partial_cmp(&foo, &greater).unwrap());
            assert_eq!(Ordering::Greater, PartialOrd::partial_cmp(&foo, &smaller).unwrap());
            assert_ne!(Ordering::Greater, PartialOrd::partial_cmp(&foo, &equal).unwrap());
            assert_eq!(Ordering::Greater, PartialOrd::partial_cmp(&greater, &foo).unwrap());
            assert!(foo <= equal && foo <= greater);
            assert!(!(foo <= smaller));
            assert!(foo >= smaller && foo >= equal);
            assert!(!(foo >= greater));

            // Ord
            assert_eq!(
                PartialOrd::partial_cmp(&foo, &smaller).unwrap(),
                Ord::cmp(&foo, &smaller)
            );
            assert_eq!(
                PartialOrd::partial_cmp(&smaller, &foo).unwrap(),
                Ord::cmp(&smaller, &foo)
            );
            assert_eq!(PartialOrd::partial_cmp(&foo, &equal).unwrap(), Ord::cmp(&foo, &equal));
            assert_eq!(PartialOrd::partial_cmp(&equal, &foo).unwrap(), Ord::cmp(&equal, &foo));
            assert_eq!(
                PartialOrd::partial_cmp(&foo, &greater).unwrap(),
                Ord::cmp(&foo, &greater)
            );
            assert_eq!(
                PartialOrd::partial_cmp(&greater, &foo).unwrap(),
                Ord::cmp(&greater, &foo)
            );
            assert_eq!(max(foo, smaller), max_by(foo, smaller, Ord::cmp));
            assert_eq!(max(smaller, foo), max_by(smaller, foo, Ord::cmp));
            assert_eq!(max(foo, equal), max_by(foo, equal, Ord::cmp));
            assert_eq!(max(equal, foo), max_by(equal, foo, Ord::cmp));
            assert_eq!(max(foo, greater), max_by(foo, greater, Ord::cmp));
            assert_eq!(max(greater, foo), max_by(greater, foo, Ord::cmp));
            assert_eq!(min(foo, smaller), min_by(foo, smaller, Ord::cmp));
            assert_eq!(min(smaller, foo), min_by(smaller, foo, Ord::cmp));
            assert_eq!(min(foo, equal), min_by(foo, equal, Ord::cmp));
            assert_eq!(min(equal, foo), min_by(equal, foo, Ord::cmp));
            assert_eq!(min(foo, greater), min_by(foo, greater, Ord::cmp));
            assert_eq!(min(greater, foo), min_by(greater, foo, Ord::cmp));
            // We don't check clamp here, as the tests above should imply correct behaviour there
        }
    }

    #[test]
    fn timestamp_overflow() {
        #[derive(Debug)]
        struct BadEpoch;

        impl Epoch for BadEpoch {
            fn millis_since_unix() -> u64 {
                u64::MAX
            }
        }

        let snowflake = Snowflake::<SimpleParams, BadEpoch>::from_raw((u32::MAX as u64) << 22).unwrap();
        assert_eq!(Error::FatalSnowflakeExhaustion, snowflake.get_timestamp().unwrap_err());
        let snowflake = Snowflake::<SimpleParams, BadEpoch>::from_raw((1 << 63) - 1).unwrap();
        assert_eq!(Error::FatalSnowflakeExhaustion, snowflake.get_timestamp().unwrap_err());
    }
}
// End skip coverage

pub use classic::{ClassicLayout, ClassicLayoutSnowflakeExtension, MachineId};
pub use comparator::SnowflakeComparator;

/// Errors that can occur when generating or using a [`Snowflake`].
#[derive(Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// An error that occurs if the system clock went backwards.
    ///
    /// It might be possible to generate new snowflakes in the future, but it's not guaranteed that new snowflakes can
    /// be generated anytime soon.
    NonMonotonicClock,
    /// An error that occurs if an [`Epoch`] is invalid.
    ///
    /// Specifically, this error indicates that an epoch is in the future or precedes the Unix epoch.
    InvalidEpoch,
    /// An error that occurs if too many snowflakes have been generated in this millisecond.
    ///
    /// Unlike [`FatalSnowflakeExhaustion`](Self::FatalSnowflakeExhaustion), this error indicates that it might be
    /// possible to generate another snowflake in the next millisecond.
    SnowflakeExhaustion,
    /// An error that occurs if a timestamp exceeds the limits of the underlying data structure.
    ///
    /// Specifically, this error can occur if a snowflake's layout doesn't dedicate enough bits to store the current
    /// timestamp or if a timestamp stored in a snowflake can't be represented as a [`SystemTime`] instance.
    FatalSnowflakeExhaustion,
    /// An error that occurs when attempting to load an invalid snowflake.
    ///
    /// This error primarily occurs when passing an integer representation of a snowflake to [`Snowflake::from_raw`]
    /// that is incompatible with constant parts required by the snowflake's [`Layout`]. Refer to
    /// [`Layout::is_valid_snowflake`] for more details.
    InvalidSnowflake,
}

// Skip coverage: Other than duplicating the constant strings below, we can't test this display implementation anyway
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NonMonotonicClock => {
                write!(f, "can't generate a new snowflake - the clock went backwards")
            }
            Error::InvalidEpoch => {
                write!(f, "invalid epoch provided")
            }
            Error::SnowflakeExhaustion => {
                write!(f, "too many snowflakes were generated this millisecond")
            }
            Error::FatalSnowflakeExhaustion => {
                write!(f, "the timestamp can't be represented by the underlying data structure")
            }
            Error::InvalidSnowflake => {
                write!(f, "the integer representation is not a valid snowflake")
            }
        }
    }
}
// End skip coverage

impl std::error::Error for Error {}

/// The primary result type of Snowdon.
pub type Result<T> = std::result::Result<T, Error>;
