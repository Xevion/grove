# Grove — development recipes

# Apply patches to vendored submodules
patch:
    cd vendor/gpui-component && git apply ../../patches/*.patch

# Run the application
run *ARGS:
    cargo run {{ARGS}}

# Run all CI checks locally: fmt, clippy, deny, machete, test
check:
    cargo fmt --check
    cargo clippy --locked -- -D warnings
    cargo +nightly clippy --lib --target wasm32-unknown-unknown --locked -- -D warnings
    cargo deny -L error check --hide-inclusion-graph advisories bans sources
    cargo machete
    cargo nextest run --locked --no-tests=fail

# Run tests
test *ARGS:
    cargo nextest run --locked --no-tests=fail {{ARGS}}
