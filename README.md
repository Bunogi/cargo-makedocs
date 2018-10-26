# cargo makedocs

# Usage
`cargo makedocs` will parse your current working directory's `Cargo.toml` for dependencies, and only build documentation for the direct dependencies. This saves you from having to type `cargo doc --no-deps -p <crate> ...`.
## Options
If you want to exclude one or more crates for being documented, simply pass `-e <cratename>` as many times as needed. Same goes in reverse for `-i`, which will document a crate even if it isn't part of your `Cargo.toml`.

The `--open` flag will open the documentation in your web browser(passes `--open` to `cargo doc`).

# License
cargo-makedocs is available under the MIT license, see LICENSE for more details.
