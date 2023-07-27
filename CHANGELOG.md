# Release 0.2.0 (WIP)

## Features

- `Cargo.toml` now correctly includes both licences (`Apache-2.0 OR MIT`) in the `license` field.

## Breaking changes

- The default generator implementation was switched to `lock-free`.</br>
  I.e. the `blocking` feature is now turned off by default, and `lock-free` is enabled. If your code uses
  `Generator::generate_blocking()`, you'll have to enable the `blocking` feature explicitly. However, unless you want to
  keep using the blocking implementation, no changes are required if you use `Generator::generate`.
- Snowflakes are now serialized as a u64 instead of a structure.

# Release 0.1.0

The initial release of Snowdon. This release introduces the following features:

- a lock-free snowflake ID generator implementation,
- a blocking snowflake ID generator implementation,
- a snowflake comparator implementation enabling users to compare snowflakes with arbitrary timestamps,
- a flexible Snowflake type allowing static enforcement of specific layout and epoch implementations without adding any
  runtime overhead,
- and an implementation for the classic snowflake ID layout introduced by Twitter.

Moreover, this release adds unit tests, an integration test using loom, and a PROMELA implementation of our lock-free
algorithm that can be verified with SPIN.

Refer to the release's documentation for details on the introduced APIs. Happy generating!