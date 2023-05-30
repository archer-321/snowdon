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

//! An implementation of the classic [`Snowflake`] layout introduced by Twitter.

use crate::{Epoch, Layout, Snowflake};
use std::marker::PhantomData;

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

    /// Returns whether the given machine ID exceeds the maximum supported by this layout.
    #[inline]
    fn exceeds_machine_id(input: u64) -> bool {
        input >= 1 << Self::MACHINE_ID_BITS
    }
}

impl<I> Layout for ClassicLayout<I>
where
    I: MachineId,
{
    #[inline]
    fn construct_snowflake(timestamp: u64, sequence_number: u64) -> u64 {
        let machine_id = I::machine_id();
        assert!(
            !Self::exceeds_timestamp(timestamp)
                && !Self::exceeds_sequence_number(sequence_number)
                && !Self::exceeds_machine_id(machine_id)
        );
        (timestamp << (Self::MACHINE_ID_BITS + Self::SEQUENCE_NUMBER_BITS))
            | (machine_id << Self::SEQUENCE_NUMBER_BITS)
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

// Skip coverage: We don't test the coverage of our unit tests
#[cfg(test)]
mod tests {
    use crate::sync::atomic::{AtomicU64, Ordering};
    use crate::sync::{Mutex, MutexGuard};
    use crate::{ClassicLayout, ClassicLayoutSnowflakeExtension, Epoch, Layout, MachineId, Snowflake};

    static MACHINE_ID: AtomicU64 = AtomicU64::new(0);
    static MACHINE_LOCK: Mutex<()> = Mutex::new(());

    struct SimpleMachineId;

    impl SimpleMachineId {
        fn set_id(id: u64) {
            MACHINE_ID.store(id, Ordering::Relaxed);
        }
        fn acquire_lock() -> MutexGuard<'static, ()> {
            MACHINE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
        }
    }

    impl MachineId for SimpleMachineId {
        fn machine_id() -> u64 {
            MACHINE_ID.load(Ordering::Relaxed)
        }
    }

    // We use the type below for tests that don't need changing machine IDs to prevent having to acquire a lock on its
    // state

    struct ZeroMachineId;

    impl MachineId for ZeroMachineId {
        fn machine_id() -> u64 {
            0
        }
    }

    #[test]
    fn construct_snowflake() {
        let _g = SimpleMachineId::acquire_lock();
        SimpleMachineId::set_id(0);

        // First, verify the individual parts
        assert_eq!(
            (1 << 12) - 1,
            ClassicLayout::<SimpleMachineId>::construct_snowflake(0, (1 << 12) - 1),
            "`construct_snowflake` used an unrelated sequence number"
        );
        SimpleMachineId::set_id((1 << 10) - 1);
        assert_eq!(
            (1 << 10) - 1,
            ClassicLayout::<SimpleMachineId>::construct_snowflake(0, 0) >> 12
        );
        SimpleMachineId::set_id(0);
        assert_eq!(
            (1 << 41) - 1,
            ClassicLayout::<SimpleMachineId>::construct_snowflake((1 << 41) - 1, 0) >> 22
        );

        // Verify that the largest snowflake still has a leading 0. It's guaranteed that this is the largest snowflake,
        // as we test that the code panics for larger values with the tests below.
        SimpleMachineId::set_id((1 << 10) - 1);
        assert_eq!(
            0,
            ClassicLayout::<SimpleMachineId>::construct_snowflake((1 << 41) - 1, (1 << 12) - 1) >> 63
        );

        // Verify that this layout doesn't introduce any bits for the smallest snowflake
        SimpleMachineId::set_id(0);
        assert_eq!(0, ClassicLayout::<SimpleMachineId>::construct_snowflake(0, 0));
    }

    // Ensure that values that exceed the layout's bit counts can't be used to construct snowflakes
    #[test]
    #[should_panic]
    fn extreme_timestamp() {
        let _ = ClassicLayout::<ZeroMachineId>::construct_snowflake(1 << 41, 0);
    }

    #[test]
    #[should_panic]
    fn extreme_sequence_number() {
        let _ = ClassicLayout::<ZeroMachineId>::construct_snowflake(0, 1 << 12);
    }

    #[test]
    #[should_panic]
    fn extreme_machine_id() {
        let _g = SimpleMachineId::acquire_lock();
        SimpleMachineId::set_id(1 << 10);
        let _ = ClassicLayout::<SimpleMachineId>::construct_snowflake(0, 0);
    }

    #[test]
    fn getters() {
        assert_eq!(0, ClassicLayout::<ZeroMachineId>::get_timestamp((1 << 22) - 1));
        assert_eq!(
            123,
            ClassicLayout::<ZeroMachineId>::get_timestamp(123 << 22 | ((1 << 22) - 1))
        );
        assert_eq!(
            (1 << 41) - 1,
            ClassicLayout::<ZeroMachineId>::get_timestamp(u64::MAX >> 1)
        );
        assert_eq!(
            0,
            ClassicLayout::<ZeroMachineId>::get_sequence_number((u64::MAX << 13) >> 1)
        );
        assert_eq!(
            123,
            ClassicLayout::<ZeroMachineId>::get_sequence_number((u64::MAX << 13) >> 1 | 123)
        );
        assert_eq!(
            (1 << 12) - 1,
            ClassicLayout::<ZeroMachineId>::get_sequence_number(u64::MAX >> 1)
        );
        assert_eq!(
            0,
            ClassicLayout::<ZeroMachineId>::get_machine_id((u64::MAX << 23) >> 1 | ((1 << 12) - 1))
        );
        assert_eq!(
            123,
            ClassicLayout::<ZeroMachineId>::get_machine_id((u64::MAX << 23) >> 1 | 123 << 12 | ((1 << 12) - 1))
        );
        assert_eq!(
            (1 << 10) - 1,
            ClassicLayout::<ZeroMachineId>::get_machine_id(u64::MAX >> 1)
        );
    }

    #[test]
    fn snowflake_extension() {
        struct SimpleEpoch;

        impl Epoch for SimpleEpoch {
            fn millis_since_unix() -> u64 {
                0
            }
        }

        type SimpleSnowflake = Snowflake<ClassicLayout<ZeroMachineId>, SimpleEpoch>;
        assert_eq!(0, SimpleSnowflake::from_raw(0).unwrap().get_machine_id());
        assert_eq!(234, SimpleSnowflake::from_raw(234 << 12).unwrap().get_machine_id());
        assert_eq!(
            (1 << 10) - 1,
            SimpleSnowflake::from_raw(u64::MAX >> 1).unwrap().get_machine_id()
        );
    }
}
// End skip coverage
