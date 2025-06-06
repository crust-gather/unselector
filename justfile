# Run clippy
clippy:
	cargo clippy

# Run rustfmt
fmt:
	cargo fmt --all

# Run tests
test:
	cargo test --all