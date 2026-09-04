# Justfile for Rust project using Cargo

all: fmt clippy-fix

# Build the project in release mode
build:
    cargo build --workspace --release

# Check the project for compilation errors without producing binaries
check:
    cargo check --workspace

# Update dependencies to their latest compatible versions
update:
    cargo update

# Run the project in release mode
run:
    cargo run --release

# Run all tests
test:
    cargo test --workspace

# Run benchmarks
bench:
    cargo bench

# Run Clippy linter with nightly toolchain (check only, for CI)
# Uses workspace lints from Cargo.toml
clippy:
    cargo +nightly clippy --workspace \
        --all-targets \
        -- -D warnings

# Run Clippy linter with auto-fix (for development)
clippy-fix:
    cargo +nightly clippy --workspace \
        --fix \
        --all-targets \
        --allow-dirty \
        --allow-staged \
        -- -D warnings

# Format facilitator only.
fmt:
    cargo +nightly fmt --package facilitator

# Generate documentation for all crates and open it in the browser
doc:
    cargo +nightly doc --no-deps --open
