# Makefile for x402 Facilitator

.PHONY: all
all: pre-commit

# Build the project in release mode
.PHONY: build
build:
	cargo build --release

# Update dependencies to their latest compatible versions
.PHONY: update
update:
	cargo update

# Run the facilitator in release mode
.PHONY: run
run:
	cargo run --release

# Run all tests
.PHONY: test
test:
	cargo test

# Run Clippy linter with nightly toolchain, fixing issues automatically
.PHONY: clippy
clippy:
	cargo +nightly clippy --fix \
		--workspace \
		--all-targets \
		--allow-dirty \
		--allow-staged \
		-- -D warnings

# Format the code using rustfmt with nightly toolchain
.PHONY: fmt
fmt:
	cargo +nightly fmt

# Generate documentation and open it in the browser
.PHONY: doc
doc:
	cargo +nightly doc --no-deps --open

# Generate CHANGELOG.md using git-cliff
.PHONY: cliff
cliff:
	git cliff --output CHANGELOG.md

# Check for unused dependencies using cargo-udeps
.PHONY: udeps
udeps:
	cargo +nightly udeps

# Build Docker image
.PHONY: docker
docker:
	docker build -t x402-facilitator .

# Run pre-commit checks
.PHONY: pre-commit
pre-commit:
	$(MAKE) build
	$(MAKE) test
	$(MAKE) clippy
	$(MAKE) fmt
