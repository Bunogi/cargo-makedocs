# cargo makedocs [![Build Status](https://travis-ci.org/Bunogi/cargo-makedocs.svg?branch=master)](https://travis-ci.org/Bunogi/cargo-makedocs) [![Crates.io Status](https://img.shields.io/crates/v/cargo-makedocs.svg)](https://crates.io/crates/cargo-makedocs)

# Installation
`cargo install cargo-makedocs`

# Usage
`cargo makedocs` will parse your current working directory's `Cargo.toml` and `Cargo.lock` for dependencies, and only build documentation for the direct dependencies. This saves you from having to type `cargo doc --no-deps -p <crate> ...`.
## Options
If you want to exclude one or more crates for being documented, simply pass `-e <cratename>` as many times as needed. Same goes in reverse for `-i`, which will document a crate even if it isn't part of your `Cargo.toml`.

The `--open` flag will open the documentation in your web browser(passes `--open` to `cargo doc`).

## Same (renamed) crate twice
Cargo will not document the same crate twice even if you have renamed it. This means that you can't, for example, get the documentation for both futures 0.1 and 0.3. To resolve such a situation, simply use the `-e` flag:
```
cargo makedocs -e futures01 # assuming futures 0.1 is named futures01
```



# License
cargo-makedocs is available under the MIT license, see LICENSE for more details.
