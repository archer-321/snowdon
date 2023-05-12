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
//! # Example
//!
//! The example below implements a custom layout that uses 42 bits for the timestamp, 10 bits for a machine ID, and 12
//! bits for the sequence number. Note that this example doesn't include the machine ID implementation. There isn't a
//! single "valid" implementation of machine IDs, as they heavily depend on your application and the way you deploy
//! your application. However, common ways to derive a machine ID include using the machine's private IP address and
//! provisioning a machine ID using some coordinated service. To avoid defaulting to an implementation that doesn't
//! work for most users, we don't provide an implementation for this function at all.
//!
//! ```
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
//! ```
//!
//! [snowflake-gh]: https://github.com/twitter-archive/snowflake/tree/b3f6a3c6ca8e1b6847baa6ff42bf72201e2c2231
//! [snowflake-blog]: https://blog.twitter.com/engineering/en_us/a/2010/announcing-snowflake

#![warn(missing_docs, missing_debug_implementations)]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(not(any(feature = "blocking", feature = "lock-free")))]
compile_error!("you must enable at least one generator implementation (blocking or lock-free)");

use std::cmp;
use std::fmt::{Display, Formatter};
use std::marker::PhantomData;
#[cfg(feature = "lock-free")]
use std::sync::atomic;
#[cfg(feature = "lock-free")]
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
#[cfg(feature = "blocking")]
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

/// A thread-safe snowflake generator for a given snowflake [layout](Layout) and [epoch](Epoch).
///
/// The generator has two type parameters to identify your snowflake format at compile-time. Refer to the example below
/// and the documentation of [`Layout`] and [`Epoch`] for details on how to define a generator for your snowflake
/// format.
///
/// # Example
///
/// ```
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
        loop {
            let time = Self::get_timestamp()?;
            let last_timestamp = L::get_timestamp(last_snowflake);
            let sequence = match last_timestamp.cmp(&time) {
                cmp::Ordering::Equal => {
                    // The timestamp didn't change, so increment the sequence number
                    L::get_sequence_number(last_snowflake)
                        .checked_add(1)
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
                Err(current_value) => {
                    // Our CAS operation failed; store the current value and try again
                    last_snowflake = current_value
                }
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
#[repr(transparent)]
pub struct Snowflake<L, E>
    where
        L: Layout,
        E: Epoch,
{
    inner: u64,
    _marker: PhantomData<(L, E)>,
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
    /// instead. With most snowflake layouts, it should be impossible to store such a timestamp in the snowflake,
    /// though.
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
{}

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
{}

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

/// A type to compare [`Snowflake`]s with timestamps.
///
/// Snowflakes already form a total order. However, this is only loosely based on the snowflake's timestamp, as the
/// entire snowflake is taken into consideration when comparing two snowflakes. I.e., if your snowflake layout contains
/// instance-specific constant parts like a machine ID, a snowflake that was generated *after* another snowflake can
/// still be smaller.
///
/// To compare a snowflake with a given timestamp, simply create a comparator using
/// [`from_system_time`](Self::from_system_time) or [`Snowflake::get_comparator`].
///
/// # Limitations
///
/// Note that some timestamps that can theoretically be represented in a `Snowflake` can't be represented by this type.
/// To exceed this type's limit, you'd need a timestamp that's roughly 585 million years after the Unix epoch, however.
/// In those cases, this type's constructors return an error instead. If you don't need to worry about timestamps this
/// large in your implementation, you should be able to unwrap all [`Result`]s returned by this type's associated
/// functions.
#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct SnowflakeComparator {
    timestamp: u64,
}

impl SnowflakeComparator {
    /// Creates a new snowflake comparator for the given system time.
    ///
    /// If the system time can't be represented by the underlying data type, this returns
    /// [`Error::FatalSnowflakeExhaustion`] instead. If `time` precedes the Unix epoch, this returns
    /// [`Error::InvalidEpoch`] instead.
    ///
    /// If you're sure your timestamp doesn't precede the Unix epoch and doesn't exceed the limits discussed in the
    /// [structure documentation](Self#limitations), you can safely unwrap this function's result.
    ///
    /// If you want to create a comparator from a snowflake's timestamp, you should use [`Snowflake::get_comparator`]
    /// instead.
    ///
    /// # Example
    ///
    /// ```
    /// use snowdon::{Snowflake, SnowflakeComparator};
    /// use std::time::{Duration, SystemTime};
    ///
    /// // This example uses Twitter's snowflake layout and epoch
    /// let snowflake = Snowflake::from_raw(1541815603606036480).unwrap();
    /// // Create a comparator for the first second of 2022
    /// let comparator = SnowflakeComparator::from_system_time(
    ///     SystemTime::UNIX_EPOCH + Duration::from_secs(1640995200),
    /// )
    /// .unwrap();
    /// assert!(comparator < snowflake);
    /// // Create a comparator for the first second of 2023
    /// let comparator = SnowflakeComparator::from_system_time(
    ///     SystemTime::UNIX_EPOCH + Duration::from_secs(1672531200),
    /// )
    /// .unwrap();
    /// assert!(comparator > snowflake);
    /// # use snowdon::{ClassicLayout, Epoch, MachineId};
    /// # struct SnowflakeParams;
    /// # impl MachineId for SnowflakeParams {
    /// #     fn machine_id() -> u64 {
    /// #         unimplemented!()
    /// #     }
    /// # }
    /// # impl Epoch for SnowflakeParams {
    /// #     fn millis_since_unix() -> u64 {
    /// #         1288834974657
    /// #     }
    /// # }
    /// # fn foo(_foo: Snowflake<ClassicLayout<SnowflakeParams>, SnowflakeParams>) {}
    /// # foo(snowflake);
    /// ```
    pub fn from_system_time(time: SystemTime) -> Result<Self> {
        let timestamp = time
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|_| Error::InvalidEpoch)?
            .as_millis();
        if timestamp > u64::MAX as u128 {
            return Err(Error::FatalSnowflakeExhaustion);
        }
        Ok(Self {
            timestamp: timestamp as u64,
        })
    }

    /// Creates a new snowflake comparator for the given timestamp.
    ///
    /// The timestamp is interpreted in the context of the [`Epoch`] provided as a type parameter. I.e., it's the number
    /// of milliseconds since the start of the given epoch.
    ///
    /// If the underlying data type can't represent the requested timestamp, this function returns
    /// [`Error::FatalSnowflakeExhaustion`] instead.
    ///
    /// If you want to avoid passing your snowflake epoch to this function everytime you compare snowflakes with a
    /// timestamp, you might want to use [`from_system_time`](Self::from_system_time) instead.
    ///
    /// You should consider using [`Snowflake::get_comparator`] instead if you're creating a comparator using another
    /// snowflake's timestamp.
    ///
    /// # Example
    ///
    /// ```
    /// use snowdon::{Snowflake, SnowflakeComparator};
    /// use std::time::{Duration, SystemTime};
    ///
    /// #[derive(Debug)]
    /// struct TwitterEpoch;
    ///
    /// impl Epoch for TwitterEpoch {
    ///     fn millis_since_unix() -> u64 {
    ///         // The epoch used by Twitter
    ///         1288834974657
    ///     }
    /// }
    ///
    /// // This example uses Twitter's snowflake layout and epoch
    /// let snowflake = Snowflake::from_raw(1541815603606036480).unwrap();
    /// // Create a comparator using the timestamp of our snowflake
    /// let comparator = SnowflakeComparator::from_timestamp::<TwitterEpoch>(
    ///     1541815603606036480 >> 22,
    /// )
    /// .unwrap();
    /// assert_eq!(snowflake, comparator);
    /// // Instead of constructing the comparator ourselves, we can use
    /// // `get_comparator`:
    /// assert_eq!(comparator, snowflake.get_comparator().unwrap());
    /// // Create a comparator for the first second of 2022
    /// let comparator =
    ///     SnowflakeComparator::from_timestamp::<TwitterEpoch>(352160225343).unwrap();
    /// assert!(comparator < snowflake);
    /// # use snowdon::{ClassicLayout, Epoch, MachineId};
    /// # #[derive(Debug)]
    /// # struct SnowflakeParams;
    /// # impl MachineId for SnowflakeParams {
    /// #     fn machine_id() -> u64 {
    /// #         unimplemented!()
    /// #     }
    /// # }
    /// # fn foo(_foo: Snowflake<ClassicLayout<SnowflakeParams>, TwitterEpoch>) {}
    /// # foo(snowflake);
    /// ```
    pub fn from_timestamp<E>(timestamp: u64) -> Result<Self>
        where
            E: Epoch,
    {
        Ok(Self {
            timestamp: Self::convert_epoch_timestamp::<E>(timestamp)?,
        })
    }

    /// Creates a new snowflake comparator from the given timestamp.
    ///
    /// Note that the timestamp passed to this function is the number of milliseconds since the **Unix epoch**. I.e.,
    /// if the timestamp you want to compare your snowflakes with uses the snowflakes' epoch, you'll have to convert it
    /// to a timestamp using the Unix epoch before passing it to this function.
    ///
    /// Usually, you shouldn't use this function directly. Instead, use [`from_system_time`](Self::from_system_time) or
    /// [`from_timestamp`](Self::from_timestamp) to get a snowflake comparator that works with your snowflakes.
    ///
    /// If you want to create a comparator from a snowflake's timestamp, you should use [`Snowflake::get_comparator`]
    /// instead.
    pub fn from_raw_timestamp(timestamp: u64) -> Self {
        Self { timestamp }
    }

    /// Converts the given epoch-based timestamp to a timestamp using the Unix epoch.
    ///
    /// If the underlying data type doesn't support the requested timestamp, this returns
    /// [`Error::FatalSnowflakeExhaustion`] instead.
    ///
    /// This function only returns errors on overflows. I.e., any timestamp that can be represented in a u64 using the
    /// Unix epoch is guaranteed to be smaller than the custom epoch timestamp passed to this function if this returns
    /// an error.
    fn convert_epoch_timestamp<E>(timestamp: u64) -> Result<u64>
        where
            E: Epoch,
    {
        let timestamp = SystemTime::UNIX_EPOCH
            .checked_add(Duration::from_millis(
                E::millis_since_unix()
                    .checked_add(timestamp)
                    .ok_or(Error::FatalSnowflakeExhaustion)?,
            ))
            .ok_or(Error::FatalSnowflakeExhaustion)?
            .duration_since(SystemTime::UNIX_EPOCH)
            // We've constructed the timestamp using a positive duration and the Unix epoch above, so this will always
            // be after the Unix epoch. Thus, we can safely unwrap this result.
            .unwrap()
            .as_millis();
        if timestamp > u64::MAX as u128 {
            return Err(Error::FatalSnowflakeExhaustion);
        }
        Ok(timestamp as u64)
    }
}

impl PartialEq for SnowflakeComparator {
    /// Returns whether this snowflake comparator represents the same timestamp as the other.
    fn eq(&self, other: &Self) -> bool {
        self.timestamp == other.timestamp
    }
}

impl<L, E> PartialEq<Snowflake<L, E>> for SnowflakeComparator
    where
        L: Layout,
        E: Epoch,
{
    /// Returns whether this snowflake comparator represents the same timestamp as the provided snowflake.
    fn eq(&self, other: &Snowflake<L, E>) -> bool {
        let other = match Self::convert_epoch_timestamp::<E>(other.get_timestamp_raw()) {
            Ok(other) => other,
            Err(_) => {
                // If the provided snowflake's timestamp can't be represented by an unsigned 64-bit integer, it's
                // guaranteed to be different from this comparator's timestamp (which we know is representable)
                return false;
            }
        };
        self.timestamp == other
    }
}

impl<L, E> PartialEq<SnowflakeComparator> for Snowflake<L, E>
    where
        L: Layout,
        E: Epoch,
{
    /// Returns whether this snowflake's timestamp is the same as the provided comparator's timestamp.
    fn eq(&self, other: &SnowflakeComparator) -> bool {
        let timestamp = match SnowflakeComparator::convert_epoch_timestamp::<E>(self.get_timestamp_raw()) {
            Ok(timestamp) => timestamp,
            Err(_) => {
                // If this snowflake's timestamp can't be represented in a u64 using the Unix epoch, it's guaranteed to
                // be different from the given comparator's timestamp (which can be represented)
                return false;
            }
        };
        timestamp == other.timestamp
    }
}

impl Eq for SnowflakeComparator {}

impl PartialOrd for SnowflakeComparator {
    /// Returns how this snowflake comparator compares to the other comparator.
    ///
    /// Specifically, this returns [`Ordering::Less`](cmp::Ordering::Less), [`Ordering::Equal`](cmp::Ordering::Equal),
    /// or [`Ordering::Greater`](cmp::Ordering::Greater) if this comparator's timestamp precedes the other comparator's
    /// timestamp, is equal to it, or succeeds it, respectively.
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<L, E> PartialOrd<Snowflake<L, E>> for SnowflakeComparator
    where
        L: Layout,
        E: Epoch,
{
    /// Returns how this snowflake comparator compares to the provided snowflake.
    ///
    /// Specifically, this returns [`Ordering::Less`](cmp::Ordering::Less), [`Ordering::Equal`](cmp::Ordering::Equal),
    /// or [`Ordering::Greater`](cmp::Ordering::Greater) if this comparator's timestamp precedes the snowflake's
    /// timestamp, is equal to it, or succeeds it, respectively.
    fn partial_cmp(&self, other: &Snowflake<L, E>) -> Option<cmp::Ordering> {
        // If the timestamp overflows a u64, the timestamp in this comparator is guaranteed to be less.
        // For now, this will essentially never happen, but we can consider using more bits in about 585 million years
        // when our Unix-epoch-based approach stops working Kappa
        let other = match Self::convert_epoch_timestamp::<E>(other.get_timestamp_raw()) {
            Ok(other) => other,
            Err(_) => return Some(cmp::Ordering::Less),
        };
        Some(self.timestamp.cmp(&other))
    }
}

impl<L, E> PartialOrd<SnowflakeComparator> for Snowflake<L, E>
    where
        L: Layout,
        E: Epoch,
{
    /// Returns how this snowflake compares to the given snowflake comparator.
    ///
    /// Specifically, this returns [`Ordering::Less`](cmp::Ordering::Less), [`Ordering::Equal`](cmp::Ordering::Equal),
    /// or [`Ordering::Greater`](cmp::Ordering::Greater) if this snowflake's timestamp precedes the comparator's
    /// timestamp, is equal to it, or succeeds it, respectively.
    fn partial_cmp(&self, other: &SnowflakeComparator) -> Option<cmp::Ordering> {
        // Similarly to the `PartialOrd` implementation above, our timestamp is guaranteed to be greater than the
        // comparator if it would overflow a u64
        let timestamp = match SnowflakeComparator::convert_epoch_timestamp::<E>(self.get_timestamp_raw()) {
            Ok(timestamp) => timestamp,
            Err(_) => return Some(cmp::Ordering::Greater),
        };
        Some(timestamp.cmp(&other.timestamp))
    }
}

impl Ord for SnowflakeComparator {
    /// Returns how this snowflake comparator compares to the other comparator.
    ///
    /// Specifically, this returns [`Ordering::Less`](cmp::Ordering::Less), [`Ordering::Equal`](cmp::Ordering::Equal),
    /// or [`Ordering::Greater`](cmp::Ordering::Greater) if this comparator's timestamp precedes the other comparator's
    /// timestamp, is equal to it, or succeeds it, respectively.
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.timestamp.cmp(&other.timestamp)
    }
}

/// A [`Layout`] implementation for the classic snowflake layout introduced by Twitter.
///
/// Snowflakes constructed with this layout consist of a leading `0` bit, 41 bits for a timestamp in milliseconds, 10
/// bits for an instance ID, and 12 bits for the sequence number. The leading `0` bit guarantees that snowflakes with
/// this layout keep their properties (namely, monotonicity) when converted into signed 64-bit integers.
///
/// Note that this layout doesn't specify the snowflake's epoch, however. Even when using this layout, you'll have to
/// specify your own epoch by implementing [`Epoch`].
///
/// # Example
///
/// ```
/// use snowdon::{
///     ClassicLayout, ClassicLayoutSnowflakeExtension, Epoch, Generator,
///     MachineId, Snowflake,
/// };
///
/// struct SnowflakeParams;
///
/// impl Epoch for SnowflakeParams {
///     fn millis_since_unix() -> u64 {
///         // The epoch used by Twitter for their snowflake IDs
///         1288834974657
///     }
/// }
///
/// impl MachineId for SnowflakeParams {
///     fn machine_id() -> u64 {
///         // Somehow obtain this machine's ID (e.g. from the private IP
///         // address or a configuration file)
/// #       0
///     }
/// }
///
/// // Make our snowflake specification available to the rest of the application
/// type MySnowflake =
///     Snowflake<ClassicLayout<SnowflakeParams>, SnowflakeParams>;
/// type MySnowflakeGenerator =
///     Generator<ClassicLayout<SnowflakeParams>, SnowflakeParams>;
///
/// // Use our snowflake format
/// let snowflake = MySnowflake::from_raw(1541815603606036480).unwrap();
/// assert_eq!(367597485448, snowflake.get_timestamp_raw());
/// assert_eq!(0x017A, snowflake.get_machine_id());
/// assert_eq!(0, snowflake.get_sequence_number());
/// ```
#[derive(Debug)]
pub struct ClassicLayout<I>
    where
        I: MachineId,
{
    _marker: PhantomData<I>,
}

impl<I> ClassicLayout<I>
    where
        I: MachineId,
{
    const TIMESTAMP_BITS: usize = 41;
    const TIMESTAMP_MASK: u64 =
        ((1 << Self::TIMESTAMP_BITS) - 1) << (Self::MACHINE_ID_BITS + Self::SEQUENCE_NUMBER_BITS);
    const MACHINE_ID_BITS: usize = 10;
    const MACHINE_ID_MASK: u64 = ((1 << Self::MACHINE_ID_BITS) - 1) << Self::SEQUENCE_NUMBER_BITS;
    const SEQUENCE_NUMBER_BITS: usize = 12;
    const SEQUENCE_NUMBER_MASK: u64 = (1 << Self::SEQUENCE_NUMBER_BITS) - 1;

    /// Returns the machine ID of the given snowflake.
    ///
    /// Usually, you shouldn't call this function directly. Instead, use
    /// [`get_machine_id`](ClassicLayoutSnowflakeExtension::get_machine_id) directly on the snowflake by importing
    /// `ClassicLayoutSnowflakeExtension`.
    #[inline]
    pub fn get_machine_id(input: u64) -> u64 {
        (input & Self::MACHINE_ID_MASK) >> Self::SEQUENCE_NUMBER_BITS
    }
}

impl<I> Layout for ClassicLayout<I>
    where
        I: MachineId,
{
    #[inline]
    fn construct_snowflake(timestamp: u64, sequence_number: u64) -> u64 {
        assert!(!Self::exceeds_timestamp(timestamp) && !Self::exceeds_sequence_number(sequence_number));
        (timestamp << (Self::MACHINE_ID_BITS + Self::SEQUENCE_NUMBER_BITS))
            | (I::machine_id() << Self::SEQUENCE_NUMBER_BITS)
            | sequence_number
    }

    #[inline]
    fn get_timestamp(input: u64) -> u64 {
        (input & Self::TIMESTAMP_MASK) >> (Self::MACHINE_ID_BITS + Self::SEQUENCE_NUMBER_BITS)
    }

    #[inline]
    fn exceeds_timestamp(input: u64) -> bool {
        input >= 1 << Self::TIMESTAMP_BITS
    }

    #[inline]
    fn get_sequence_number(input: u64) -> u64 {
        input & Self::SEQUENCE_NUMBER_MASK
    }

    #[inline]
    fn exceeds_sequence_number(input: u64) -> bool {
        input >= 1 << Self::SEQUENCE_NUMBER_BITS
    }

    #[inline]
    fn is_valid_snowflake(input: u64) -> bool {
        // Check whether the 64th bit is set to 0
        input < 1 << 63
    }
}

/// An extension for [`Snowflake`]s to get the snowflake's machine ID.
///
/// This trait is implemented for all snowflakes that use the [`ClassicLayout`] layout implementation.
///
/// # Example
///
/// ```
/// # use snowdon::{Epoch, MachineId};
/// use snowdon::{
///     ClassicLayout, ClassicLayoutSnowflakeExtension, Generator, Snowflake,
/// };
/// # struct SnowflakeParams;
/// # impl Epoch for SnowflakeParams {
/// #     fn millis_since_unix() -> u64 {
/// #         // The epoch used by Twitter for their snowflake IDs
/// #         1288834974657
/// #     }
/// # }
/// # impl MachineId for SnowflakeParams {
/// #     fn machine_id() -> u64 {
/// #         // Somehow obtain this machine's ID (e.g. from the private IP
/// #         // address or a configuration file)
/// #        0
/// #     }
/// # }
///
/// type MySnowflake =
///     Snowflake<ClassicLayout<SnowflakeParams>, SnowflakeParams>;
/// type MySnowflakeGenerator =
///     Generator<ClassicLayout<SnowflakeParams>, SnowflakeParams>;
///
/// let snowflake = MySnowflake::from_raw(1541815603606036480).unwrap();
/// assert_eq!(0x017A, snowflake.get_machine_id());
/// assert_eq!(
///     snowflake.get_machine_id(),
///     ClassicLayout::<SnowflakeParams>::get_machine_id(snowflake.get())
/// );
/// ```
pub trait ClassicLayoutSnowflakeExtension {
    /// Returns the snowflake's machine ID.
    ///
    /// Refer to the [trait documentation](Self) for an example.
    fn get_machine_id(&self) -> u64;

    /// Returns this snowflake as a positive signed integer.
    ///
    /// This layout guarantees that the first bit of a snowflake generated with it is `0`, so snowflakes using the
    /// `ClassicLayout` can safely be serialized as a signed 64-bit integer.
    fn get_i64(&self) -> i64;
}

impl<I, E> ClassicLayoutSnowflakeExtension for Snowflake<ClassicLayout<I>, E>
    where
        I: MachineId,
        E: Epoch,
{
    #[inline]
    fn get_machine_id(&self) -> u64 {
        ClassicLayout::<I>::get_machine_id(self.get())
    }

    #[inline]
    fn get_i64(&self) -> i64 {
        // This layout guarantees a constant `0` as the first bit. I.e., we can safely convert this to a signed integer
        // without having to worry about the resulting integer being negative.
        self.inner as i64
    }
}

/// A trait that defines a [`Snowflake`]'s constant machine ID.
///
/// This trait only requires a single associated function. I.e., there's no instance available when determining the
/// machine ID. This is an intentional design decision to discourage dynamic implementations of this value. Regardless
/// of how you determine this machine's unique ID, it's important that this function always returns the same value.
///
/// There isn't a single machine ID implementation that we could encourage here, so we're not providing an example.
/// If your implementation requires accessing remote resources to determine this machine's ID, however, you should
/// consider using the `lazy_static` crate to avoid having to re-obtain this ID for every generated snowflake.
pub trait MachineId {
    /// Returns this machine's unique ID.
    ///
    /// The returned ID must remain constant throughout the runtime of this instance.
    fn machine_id() -> u64;
}

/// Errors that can occur when generating or using a [`Snowflake`].
#[derive(Debug)]
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

impl std::error::Error for Error {}

/// The primary result type of Snowdon.
pub type Result<T> = std::result::Result<T, Error>;
