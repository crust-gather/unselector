# Run clippy
clippy:
	cargo clippy

# Run rustfmt
fmt:
	cargo fmt --all

# Run tests
test:
	cargo test --all

# Update dependencies
update:
	cargo +nightly update --breaking -Z unstable-options

build:
	cargo build

# Build without kube feature
build-no-kube:
	cargo build --no-default-features
