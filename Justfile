default: test clippy format

test:
	@cargo test -- --ignored --nocapture

clippy:
	@cargo clippy --all-targets --all-features --tests -q
	@cargo clippy --all-targets --no-default-features -q

format:
	@cargo +nightly fmt --all -- --config-path .config/rustfmt.toml
	@taplo format --config .config/taplo.toml

fmt: format
f: fmt

fix: clippy-fix format

clippy-fix:
	@cargo clippy --all-targets --all-features --tests --fix -q --allow-dirty
	@cargo clippy --all-targets --no-default-features --fix -q --allow-dirty

install:
	@cargo install --locked --path .

release:
	#!/usr/bin/env bash
	set -e

	# Extract vesrion from Cargo.toml using perl
	version=$(perl -ne 'if (/version\s*=\s*"([^"]+)"/) { print $1; exit }' Cargo.toml)
	tag="v$version"
	echo "Releasing $version"

	# Create tag if not yet exists
	if ! git tag -l "$tag" | grep -q .; then
		echo "Please sign the tag: $tag"
		git tag -s -m "$version" $tag
	else
		echo "Tag $tag already exists"
	fi

	# Push tag
	read -p "Do you want to release $version [y/N] " confirm
	[[ "$confirm" == [yY] ]] || { echo "Aborted."; exit 1; }
	git push origin $tag
