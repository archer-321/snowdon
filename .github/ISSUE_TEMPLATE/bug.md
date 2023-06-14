---
name: Bug Report
about: Use this template to report incorrect or unexpected behaviour.
labels: bug, needs-triage
---

<!-- Please double-check that this bug has yet to be reported. -->

## Affected component

<!--
    Which part of Snowdon is affected by this bug? This part should be a concise description of the component that
    you're experiencing the problem with.
    Examples:
    Creating a `SnowflakeComparator` using a `SystemTime` instance
    The blocking snowflake generator
-->

## Version and features

<!-- Please fill out the fields below. If no crate features are required, you can say `none` -->

- Snowdon version:
- Required crate features:

## Description

<!--
    This part is a short description of the bug you're experiencing. Usually, a single sentence should be enough.
    Examples:
    The crate panics when trying to pass `SystemTime::UNIX_EPOCH` to `SnowflakeComparator::from_system_time`.
    The blocking generator returns the same snowflake twice when called >4096 times per second.
-->

## Minimal verifiable example

<!--
    Create a minimal example that shows the problem you're experiencing. Try to keep it as concise as possible and
    focus on the issue you're reporting.
    You should paste the code in the ``` block below. If your code needs other dependencies, please mention them outside
    the code block.
-->

<details>
<summary>Example</summary>

```rust

panic!("Paste your example code here");

```

</details>

## Expected behaviour

<!--
    Which behaviour did you expect?
-->

## Actual behaviour

<!--
    What's the actual behaviour? How does it differ from the expected behaviour above?
-->