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

//! A comparator implementation to compare [`Snowflake`] implementations with arbitrary timestamps.

use crate::{Epoch, Error, Layout, Result, Snowflake};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::cmp;
use std::hash::{Hash, Hasher};
use std::time::SystemTime;

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
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[repr(transparent)]
pub struct SnowflakeComparator {
    // Skip coverage: The `derive` above makes the line below show up as uncovered. However, we're not testing the
    // debug implementation or Clone / Copy (which are trivial for this type).
    timestamp: u64,
    // End skip coverage
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
        E::millis_since_unix()
            .checked_add(timestamp)
            .ok_or(Error::FatalSnowflakeExhaustion)
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

impl Hash for SnowflakeComparator {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.timestamp.hash(state);
    }
}

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

// Skip coverage: We don't test the coverage of our unit tests
#[cfg(test)]
mod tests {
    use crate::{ClassicLayout, Epoch, Error, Layout, MachineId, Snowflake, SnowflakeComparator};
    use std::time::{Duration, SystemTime};

    struct SimpleEpoch;

    impl Epoch for SimpleEpoch {
        fn millis_since_unix() -> u64 {
            0
        }
    }

    // The second... second of 1970
    const SECOND_SECOND: u64 = 1000;

    #[test]
    fn from_system_time() {
        let comparator =
            SnowflakeComparator::from_system_time(SystemTime::UNIX_EPOCH + Duration::from_millis(SECOND_SECOND))
                .unwrap();
        verify_comparator(comparator, SECOND_SECOND);

        // Timestamps that predate the Unix epoch should return an error
        assert_eq!(
            Error::InvalidEpoch,
            SnowflakeComparator::from_system_time(SystemTime::UNIX_EPOCH - Duration::from_millis(1)).unwrap_err(),
            "snowflake comparator didn't detect a \"negative\" timestamp"
        );
        // Timestamps that exceed the underlying data type should return an error as well
        assert_eq!(
            Error::FatalSnowflakeExhaustion,
            SnowflakeComparator::from_system_time(
                SystemTime::UNIX_EPOCH + Duration::from_millis(u64::MAX) + Duration::from_millis(1)
            )
            .unwrap_err(),
            "snowflake comparator accepted timestamp that exceeds its data type"
        );
    }

    #[test]
    fn from_timestamp() {
        let comparator = SnowflakeComparator::from_timestamp::<SimpleEpoch>(SECOND_SECOND).unwrap();
        verify_comparator(comparator, SECOND_SECOND);

        // Verify a non-zero epoch
        struct OtherEpoch;

        impl Epoch for OtherEpoch {
            fn millis_since_unix() -> u64 {
                SECOND_SECOND
            }
        }

        let comparator = SnowflakeComparator::from_timestamp::<OtherEpoch>(SECOND_SECOND).unwrap();
        verify_comparator(comparator, SECOND_SECOND * 2);

        // Timestamps that exceed the underlying data type should return an error
        assert_eq!(
            Error::FatalSnowflakeExhaustion,
            SnowflakeComparator::from_timestamp::<OtherEpoch>(u64::MAX).unwrap_err()
        );
    }

    #[test]
    fn from_raw_timestamp() {
        let comparator = SnowflakeComparator::from_raw_timestamp(SECOND_SECOND);
        verify_comparator(comparator, SECOND_SECOND);
    }

    #[allow(clippy::nonminimal_bool, clippy::eq_op)]
    fn verify_comparator(comparator: SnowflakeComparator, timestamp: u64) {
        // This test needs comparators that are smaller and greater than the provided one
        assert!(timestamp > u64::MIN && timestamp < u64::MAX);

        let (less, equal, greater) = (
            SnowflakeComparator::from_raw_timestamp(timestamp - 1),
            SnowflakeComparator::from_raw_timestamp(timestamp),
            SnowflakeComparator::from_raw_timestamp(timestamp + 1),
        );
        crate::snowflake_tests::validate_partial_ord(comparator, less, equal, greater);
        crate::snowflake_tests::validate_ord(comparator, less, equal, greater);

        #[derive(Debug)]
        struct SimpleParams;

        impl Layout for SimpleParams {
            fn construct_snowflake(timestamp: u64, sequence_number: u64) -> u64 {
                assert!(!Self::exceeds_timestamp(timestamp) && !Self::exceeds_sequence_number(sequence_number));
                timestamp << 32 | sequence_number
            }
            fn get_timestamp(input: u64) -> u64 {
                input >> 32
            }
            fn exceeds_timestamp(input: u64) -> bool {
                input > u32::MAX as u64
            }
            fn get_sequence_number(input: u64) -> u64 {
                input & u32::MAX as u64
            }
            fn exceeds_sequence_number(input: u64) -> bool {
                input > u32::MAX as u64
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

        type SimpleSnowflake = Snowflake<SimpleParams, SimpleParams>;

        let (less, equal, greater) = (
            SimpleSnowflake::from_raw((timestamp - 1) << 32 | 3).unwrap(),
            SimpleSnowflake::from_raw(timestamp << 32 | 2).unwrap(),
            SimpleSnowflake::from_raw((timestamp + 1) << 32 | 1).unwrap(),
        );
        crate::snowflake_tests::validate_partial_ord(comparator, less, equal, greater);
        // We can't implement Eq and Ord for comparators and snowflakes, as these traits don't have any type parameters,
        // so we can't test those implementations either.

        let snowflake = SimpleSnowflake::from_raw(timestamp << 32).unwrap();
        let (less, equal, greater) = (
            SnowflakeComparator::from_raw_timestamp(timestamp - 1),
            SnowflakeComparator::from_raw_timestamp(timestamp),
            SnowflakeComparator::from_raw_timestamp(timestamp + 1),
        );
        crate::snowflake_tests::validate_partial_ord(snowflake, less, equal, greater);
    }

    #[test]
    fn extreme_epoch() {
        // Test epochs that make snowflakes exceed the underlying data type of comparators (the PartialOrd
        // implementation still works as expected)
        struct ExtremeParams;

        impl Epoch for ExtremeParams {
            fn millis_since_unix() -> u64 {
                u64::MAX
            }
        }

        impl MachineId for ExtremeParams {
            fn machine_id() -> u64 {
                // Return a not-so-extreme constant 0
                0
            }
        }

        type ExtremeSnowflake = Snowflake<ClassicLayout<ExtremeParams>, ExtremeParams>;
        let extreme = ExtremeSnowflake::from_raw((u64::MAX << 23) >> 1).unwrap();
        let (small_comparator, large_comparator) = (
            SnowflakeComparator::from_raw_timestamp(0),
            SnowflakeComparator::from_raw_timestamp(u64::MAX),
        );
        assert!(small_comparator < extreme);
        assert!(large_comparator < extreme);
        assert!(extreme > small_comparator);
        assert!(extreme > large_comparator);
        assert!(extreme != small_comparator && extreme != large_comparator);
        assert!(small_comparator != extreme && large_comparator != extreme);

        // Create a snowflake with a timestamp that's only one millisecond past `large_comparator`
        let extreme = ExtremeSnowflake::from_raw(1 << 22).unwrap();
        assert!(small_comparator < extreme);
        assert!(large_comparator < extreme);
        assert!(extreme > small_comparator);
        assert!(extreme > large_comparator);
        assert!(extreme != small_comparator && extreme != large_comparator);
        assert!(small_comparator != extreme && large_comparator != extreme);
    }
}
// End skip coverage
