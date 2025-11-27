default: test clippy format

test:
	cargo test -- --ignored --nocapture

clippy:
	cargo clippy --all-targets --all-features --tests -q
	cargo clippy --all-targets --no-default-features -q

format:
	cargo +nightly fmt --check --all-targets --all-features --tests

fix: clippy-fix format-fix

clippy-fix:
	cargo clippy --all-targets --all-features --tests --fix -q --allow-dirty
	cargo clippy --all-targets --no-default-features --fix -q --allow-dirty

format-fix:
	cargo +nightly fmt --fix --all-targets --all-features --tests
