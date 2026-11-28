// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use std::{fs, path::Path, process::Command};

fn package(root: &Path, name: &str, manifest: &str) {
	let dir = root.join(name);
	fs::create_dir_all(dir.join("src")).unwrap();
	fs::write(dir.join("Cargo.toml"), manifest).unwrap();
	fs::write(dir.join("src/lib.rs"), "").unwrap();
}

#[test]
fn weak_forward_is_conditional_only_without_activation() {
	let workspace = tempfile::tempdir().unwrap();
	let root = workspace.path();
	fs::write(root.join("Cargo.toml"), "[workspace]\nresolver = \"2\"\nmembers = [\"parent\", \"target\", \"other\", \"platform\"]\n").unwrap();
	package(
		root,
		"parent",
		r#"
[package]
name = "parent"
version = "0.1.0"
edition = "2021"
[dependencies]
aliased = { package = "target-package", path = "../target", optional = true }
other = { path = "../other", optional = true }
[dev-dependencies]
aliased = { package = "target-package", path = "../target" }
[target.'cfg(unix)'.dependencies]
platform = { path = "../platform", optional = true }
[target.'cfg(windows)'.dependencies]
platform = { path = "../platform" }
[features]
direct = ["dep:aliased", "aliased?/wanted"]
strong = ["aliased/other", "aliased?/wanted"]
transitive = ["local", "aliased?/wanted"]
local = ["dep:aliased"]
conditional = ["aliased?/wanted"]
unrelated = ["dep:other", "aliased?/wanted"]
weak_only = ["aliased?/other", "aliased?/wanted"]
platform_weak = ["platform?/wanted"]
"#,
	);
	package(
		root,
		"target",
		r#"
[package]
name = "target-package"
version = "0.1.0"
edition = "2021"
[features]
wanted = []
other = []
"#,
	);
	for name in ["other", "platform"] {
		package(root, name, &format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[features]\nwanted = []\n"));
	}

	let why = |package: &str| {
		let output = Command::new(env!("CARGO_BIN_EXE_zepter"))
			.args([
				"lint",
				"why-enabled",
				"--package",
				package,
				"--feature",
				"wanted",
				"--offline",
				"--log",
				"warn",
				"--manifest-path",
			])
			.arg(root.join("Cargo.toml"))
			.output()
			.unwrap();
		assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
		String::from_utf8(output.stdout).unwrap()
	};
	assert_eq!(
		why("target-package"),
		concat!(
			"Feature wanted/target-package is enabled by:\n",
			"  parent/conditional (if aliased is active)\n",
			"  parent/direct\n",
			"  parent/strong\n",
			"  parent/transitive\n",
			"  parent/unrelated (if aliased is active)\n",
			"  parent/weak_only (if aliased is active)\n",
		)
	);
	assert_eq!(
		why("platform"),
		"Feature wanted/platform is enabled by:\n  parent/platform_weak (if platform is active)\n"
	);
}
