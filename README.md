# Snowdon

Snowdon is a lightweight snowflake ID implementation in Rust. It offers a simple blocking and a lock-free thread-safe
generator implementation. Moreover, it supports custom Snowflake formats and epochs.

## Snowflake IDs

Snowflake IDs are unique IDs that include the timestamp of their "birth" (milliseconds since a fixed epoch) and a
sequence number to allow the creation of multiple snowflakes in a single millisecond. They can also include a machine
or generator ID for unique IDs across multiple generating instances without synchronization.

Twitter originally introduced snowflakes as IDs for tweets. While their implementation had a fixed epoch and snowflake
layout, most other services that adopted snowflakes have changed the epoch or layout (e.g. by using all 42 bits to store
the timestamp or by omitting the machine ID). Thus, when referring to "snowflake IDs", we mean timestamps associated
with a sequence number and, optionally, (instance-specific) constant data like a machine ID or the leading 0 in
Twitter's snowflakes.

## Project goals and non-goals

Snowdon aims to be a general-purpose snowflake ID implementation. We have the following goals:

- Provide a correct thread-safe implementation of snowflake IDs.<br/>
  The IDs generated by Snowdon should be unique, monotonic, and consistent with other machines in a deployment. You can
  learn more about correctness in the [section below](#correctness).
- Efficiently generate snowflake IDs.<br/>
  Snowflake IDs should work everywhere in an application. As this forces the ID generator to generate countless
  snowflakes, it must be as efficient as possible.
- Support custom snowflake formats.<br/>
  Unlike other ID formats, snowflakes usually differ from the original implementation, e.g. by using a different epoch
  or changing the number of bits used to store the various parts of a snowflake. Snowdon makes it easy to define and use
  these custom formats without requiring users to re-implement snowflakes from scratch.
- Be lightweight.<br/>
  Snowdon aims to be usable across various applications, so it's crucial to keep it lightweight to avoid unnecessarily
  increasing the resulting binary size or compile time.

However, we define the following non-goals to outline what Snowdon is *not*:

- Provide specific implementations for the various types of snowflakes used in existing software.<br/>
  Countless services have adopted snowflake IDs, and most of them use custom epochs, machine IDs, or even custom bit
  counts to represent the various parts of a snowflake in the 64-bit integer. Supporting specific implementations by
  providing a default implementation in Snowdon adds unnecessary clutter to the library that most users won't need.
  Instead, we aim to make custom snowflake formats as simple as possible. However, we do support the original Twitter
  layout of Snowflakes combined with a custom epoch, as many projects default to this layout.
- Provide macros or a DSL to make snowflake specifications easier to write.<br/>
  While we could create macros to remove some boilerplate from custom snowflake implementations, this would
  unnecessarily increase the compile time while only slightly reducing the overall level of boilerplate, as generators
  only need to be specified once.
- Provide coordination for multi-instance deployments.<br/>
  Snowflakes are designed to allow uncoordinated *unique* IDs by including a machine ID in each snowflake. If IDs must
  be unique across a multi-generator deployment, users must add a unique generator ID/machine ID to their snowflake
  format. It is up to the user to verify that the machine ID they pass to Snowdon generators is unique.
- Provide a means to automatically generate unique machine IDs e.g. by using the machine's IP address.<br/>
  The format of the machine ID highly depends on the application. Most simple approaches don't work for every
  deployment, and more general ways to assign machine IDs require cross-instance synchronization. Thus, it's up to the
  user to derive machine IDs in a way that suits their application.
- Provide a tool to specify snowflake layouts or epochs dynamically. Snowdon's primary focus is creating and using
  "real" snowflake IDs. Its API is designed to work with instance-wide constant layouts and epochs to avoid having to
  specify them with every call to one of the library's functions. However, this doesn't mean that all constants must
  be defined at compile time. E.g., it's fine to obtain the application's epoch at runtime before the first snowflake is
  generated.

## Usage

To use this crate, simply add the following line to your `Cargo.toml` file in the `dependencies` table:

```toml
snowdon = "^0.2"
```

If you want to use the lock-free implementation, it's recommended to disable the `blocking` feature (enabled by default)
by disabling this crate's default features:

```toml
snowdon = { version = "^0.2", default-features = false, features = ["lock-free"] }
```

To enable Serde support for snowflakes and snowflake comparators, enable the `serde` feature:

```toml
snowdon = { version = "^0.2", default-features = false, features = ["serde"] }
```

The types of this library are designed to make it hard to misuse them. Specifically, snowflakes have two type parameters
that represent their layout (the composition of a snowflake - this defines how many bits are used for the individual
parts of the snowflake) and their epoch (the timestamp that the individual timestamps in snowflakes are based on).
Because of this, it's impossible to use snowflakes in an API that expects a different layout or epoch.

Moreover, this crates doesn't provide an (easy) way to convert timestamps into snowflakes. Instead, it provides a
comparator type that can be compared with snowflakes directly. However, users of this crate can't accidentally use those
comparators in places that expect snowflakes.

Our documentation includes various examples for all complex methods. When implementing a trait, refer to the function's
documentation for detailed requirements and examples.

## SemVer compatibility

Snowdon takes Semantic Versioning compatibility seriously. In addition to the [SemVer specification][semver], we
guarantee that *patch* releases for versions starting with `0` (e.g. `0.1.1`) don't include breaking changes. I.e.,
you should be able to (automatically) update from version `0.1.0` to version `0.1.1` without having to check for
breaking changes. This behaviour effectively matches [Cargo's SemVer compatibility][cargo-semver].

[semver]: https://semver.org/

[cargo-semver]: https://doc.rust-lang.org/stable/cargo/reference/resolver.html#semver-compatibility

## Minimum Rust version policy

Our current minimum supported Rust version is `1.56.1`. We might increase our MSRV in the future, but we try to only do
so if there's a good reason for it. Only having a few dependencies that each have a stable MSRV policy, we shouldn't
need to increase our MSRV transitively because of a dependency update.

Note that benchmarks and tests require a higher Rust version, as they add dependencies that don't support the MSRV
above.

## Correctness

As detailed in [Project goals and non-goals](#project-goals-and-non-goals), Snowdon aims to be a **correct**
implementation of Twitter's Snowflake algorithm. All generators this crate implements should be safe to use across
multiple threads. More specifically, we try to ensure the following behaviour:

- All snowflakes returned by the same generator are **unique**. I.e., no snowflake will be returned twice.
- There are no "gaps" in the sequence number. If a snowflake with the sequence number `1` exists, another snowflake with
  the same timestamp and the sequence number `0` must also exist.
- Snowflakes are strictly monotonic. If the snowflake `b` was generated *after* the snowflake `a`, `a < b`.
- Snowdon uses the current system time for the snowflake. While this should be an obvious correctness requirement, all
  snowflake-generating instances in a deployment must adhere to this rule.

Note that "before" and "after" in the list above refer to the actual time. I.e., even if the system clock is not
monotonic, Snowdon only returns a snowflake `b` after producing the snowflake `a` for all subsequent invocations if
`a < b`.

In an attempt to verify the lock-free algorithm, a PROMELA implementation for it is provided in the `spin` directory.
Refer to the [README](spin/README.md) in that directory for more information about the model and our lock-free snowflake
implementation.

**Please note** that while we *try* to provide a correct and efficient implementation, there's no mathematical proof
that Snowdon's implementation is indeed correct. We do our best to ensure that our code is correct, but if you're
planning to use Snowdon in a production environment, you should verify that our implementation meets your expectations.
If you find any bugs or potential efficiency improvements, please open an issue as described
in [CONTRIBUTING.md](CONTRIBUTING.md).

## Benchmarks

Snowdon provides benchmarks in `benches`. You can run them with

```shell
cargo bench --all-features
```

When running the test on an AMD Ryzen 9 5900X with Linux 6.3.5, we get the following results:

| Benchmark                        | Average time per snowflake (in ns) | Estimated snowflake IDs per second |
|----------------------------------|------------------------------------|------------------------------------|
| Lock-free generator (sequential) | 29.188                             | 34260655                           |
| Blocking generator (sequential)  | 29.038                             | 34437633                           |
| Lock-free generator (10 threads) | 84.954                             | 11771076                           |
| Blocking generator (10 threads)  | 1594.4                             | 627195                             |

The last two entries in the table above are for an extreme scenario where ten threads concurrently generate snowflakes
in a loop. It's meant to simulate severe contention between threads, and with most applications, the actual times per
snowflake should be closer to the sequential benchmarks. Moreover, most implementations outperform the limitations of
snowflake layouts with 12 sequence number bits. Under almost all circumstances, the generators are fast enough for a
regular application's ID requirements. This includes applications that need to generate more than 100000 IDs per second.

## Licence

Copyright (C) 2023 Archer <archer@nefarious.dev>

Snowdon is licensed under either of

- Apache License 2.0 (SPDX-License-Identifier: Apache-2.0)<br/>
  [COPYING-APACHE](COPYING-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>
- MIT License (Expat License; SPDX-License-Identifier: MIT)<br/>
  [COPYING-MIT](COPYING-MIT) or <https://opensource.org/license/mit/>

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as
defined in the Apache-2.0 licence, shall be dual licensed as above, without any additional terms or conditions.
