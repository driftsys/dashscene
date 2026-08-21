# Changelog

All notable changes to this package are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the version tracks
the Cargo workspace rather than moving on its own.

## [Unreleased]

### Added

- The C# declaration of boundary B — the value types `crates/dashpaint-abi`
  holds to a C representation (story #1239).
- The C# host on the C ABI: P/Invoke declarations for all fourteen entry points,
  a thread-affine managed lifetime, the `ds_last_error_message` channel on every
  failure, and the committed frame under a lease that checks each array's stride
  before a row is read (story #1121).
- A `Frame Loop` sample — a `MonoBehaviour` that loads a `.dsb`, ticks it, and
  takes each committed frame. It draws nothing; the painter is story #1122.
- `.meta` files for every path Unity imports, without which a Git-URL package
  delivers nothing (R-E2), and a `unity` field declaring `6000.3` (R-E1).
