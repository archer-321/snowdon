# Contributing to Snowdon

We appreciate your interest in contributing to Snowdon! We aim to make Snowdon an excellent library, but we rely on
community contributions to make it even better.

This file explains requesting features, reporting bugs, or contributing code. Before opening an issue or a pull request,
please read the applicable section in this file to ensure you include the required information.

## Issues

If you'd like to request a new feature or report a bug, please open an issue using the appropriate template. The
templates contain various sections and comments explaining what information is required.

## Pull Requests

You can contribute code or documentation changes by opening a Pull Request in [Snowdon's GitHub repository][github].
When submitting a Pull Request, please ensure that your patches follow the [commit message guidelines][commits]
and [code style][code-style] outlined below.

We leave your commits untouched when merging Pull Requests wherever possible. Therefore, it's best to include minor
changes like fixed typos or formatting changes by amending the parent commit. However, it's preferred to move different
changes into different commits. Each commit should introduce a single distinct change. That said, we consider looking
through the individual commits of a Pull Request as part of reviewing your code, so if you need clarification on whether
something should have a separate commit, it's best to submit separate commits and squash them if necessary.

While we require that all patches follow the guidelines outlined in this file, we want to stress that all contributions
are welcome. If you need clarification on something described in this file, it's best to submit a Pull Request or issue.
We'll be happy to help you if any changes are necessary.

[github]: https://github.com/archer-321/snowdon

## Commits

[commits]: #commits

We use commit messages to describe the changes they introduce. Specifically, we require that all commits have a
descriptive summary and body. You can look into the repository's commit log for example commit messages.

The commit summary should be at most 50 characters. If your summary exceeds this limit, try to use abbreviations or
leave out parts that add little information. An example of a good commit summary
is `comparator: Implement Clone for comparators`. Note that using `SnowflakeComparator` instead of `comparators` would
make the summary exceed the 50 characters limit.

The first part of the summary should be the affected module. This part mostly matches Rust modules, but some other (
non-rust) parts of the library use custom "modules" like `spin:`, `cargo:`, or `meta:` (for non-Rust documentation like
README.md). If multiple (Rust) modules are affected by the change, pick the most relevant or super module if all modules
are equally important. The module part starts with a _lowercase_ letter, uses kebab-case (even if the Rust module
doesn't), and ends with a single colon immediately followed by a space (`: `). Modules allow us to quickly identify
which parts of the program a specific commit changes.

The commit body (excluding the summary) should contain a detailed description of all changes in the commit. While it
might be OK to omit minor changes like fixed typos, you should try to describe all changes the commit introduces. In
addition to answering the question "What changed?", your commit body should outline _why_ we should change this. While
you don't have to write a full RFC in your commit messages, it's essential to log the reasoning behind a specific change
in case we need to revisit it.

## Code style

[code-style]: #code-style

We mostly follow Rust's API guidelines for this crate. Using those guidelines, we aim to make the crate as idiomatic and
interoperable as possible. When submitting a patch for Snowdon, please go through the [Rust API Guidelines Checklist] to
ensure that your code aligns with the rest of the crate as much as possible.

### Maximum line width and `rustfmt`

This crate uses an increased maximum line width of 120 characters. This increase allows displaying two files next to
each other without requiring horizontal scrolling on most modern displays while giving us valuable extra space.

When contributing to this crate, please use `rustfmt` (`cargo fmt`) to format your changes. Moreover, it would help if
you considered enabling the nightly `rustfmt` flags in `.rustfmt-nightly.toml` by following the instructions at the top
of that file. Naturally, you'll need to install the nightly toolchain for that.

[Rust API Guidelines Checklist]: https://rust-lang.github.io/api-guidelines/checklist.html