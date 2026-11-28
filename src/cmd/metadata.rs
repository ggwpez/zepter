// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use cargo_metadata::{Dependency, Metadata, MetadataCommand, Package, Resolve};

/// Arguments for how to load cargo metadata from a workspace.
#[derive(Debug, Clone, clap::Parser, PartialEq)]
pub struct CargoArgs {
	/// Cargo manifest path or directory.
	///
	/// For directories it appends a `Cargo.toml`.
	#[arg(long, global = true)]
	pub manifest_path: Option<std::path::PathBuf>,

	/// Whether to only consider workspace crates.
	#[clap(long, global = true)]
	pub workspace: bool,

	/// Whether to use offline mode.
	#[clap(long, global = true)]
	pub offline: bool,

	/// Whether to use all the locked dependencies from the `Cargo.lock`.
	///
	/// Otherwise it may update some dependencies. For CI usage its a good idea to use it.
	#[clap(long, global = true)]
	pub locked: bool,

	#[clap(long, global = true)]
	pub all_features: bool,
}

impl CargoArgs {
	pub fn with_workspace(mut self, workspace: bool) -> Self {
		self.workspace = workspace;
		self
	}

	/// Load the metadata of the rust project.
	pub fn load_metadata(&self) -> Result<Metadata, String> {
		let err = match self.load_metadata_unsorted() {
			Ok(meta) => return Ok(meta),
			Err(err) => err,
		};

		if check_for_locked_error(&err) {
			Err("\nThe Cargo.lock file needs to be updated first since --locked is present.\n"
				.to_string())
		} else {
			Err(err)
		}
	}

	fn load_metadata_unsorted(&self) -> Result<Metadata, String> {
		let mut cmd = MetadataCommand::new();

		if let Some(ref manifest_path) = self.manifest_path {
			let manifest_path = if manifest_path.is_dir() {
				manifest_path.join("Cargo.toml")
			} else {
				manifest_path.clone()
			};
			cmd.manifest_path(&manifest_path);
		}

		cmd.features(cargo_metadata::CargoOpt::AllFeatures);

		if self.workspace {
			cmd.no_deps();
		}
		if self.offline {
			cmd.other_options(vec!["--offline".to_string()]);
		}
		if self.locked {
			cmd.other_options(vec!["--locked".to_string()]);
		}

		cmd.exec().map_err(format_metadata_error)
	}
}

fn format_metadata_error(err: cargo_metadata::Error) -> String {
	match err {
		cargo_metadata::Error::CargoMetadata { stderr } => {
			let stderr = stderr.trim();
			if stderr.is_empty() {
				"Failed to load metadata: `cargo metadata` exited with an error".to_string()
			} else {
				format!("Failed to load metadata (`cargo metadata` failed):\n{stderr}")
			}
		},
		other => format!("Failed to load metadata: {other}"),
	}
}

fn check_for_locked_error(err: &str) -> bool {
	err.contains("needs to be updated but --locked was passed to prevent this")
}

/// Resolve the dependency `dep` of `pkg` within the metadata.
///
/// This checks whether the dependency is a workspace or external crate and resolves it accordingly.
pub(crate) fn resolve_dep(
	pkg: &Package,
	dep: &Dependency,
	meta: &Metadata,
) -> Option<RenamedPackage> {
	match meta.resolve.as_ref() {
		Some(resolve) => resolve_dep_from_graph(pkg, dep, (meta, resolve)),
		None => resolve_dep_from_workspace(dep, meta),
	}
}

/// Resolve a dependency to a workspace member when its identity can be established from metadata.
pub(crate) fn resolve_dep_from_workspace(
	dep: &Dependency,
	meta: &Metadata,
) -> Option<RenamedPackage> {
	let pkg = meta.workspace_packages().into_iter().find(|pkg| {
		pkg.name == dep.name &&
			pkg.source == dep.source &&
			declaration_matches_version(dep, pkg) &&
			dep.path
				.as_ref()
				.is_none_or(|path| pkg.manifest_path.parent() == Some(path.as_path()))
	})?;
	Some(RenamedPackage::new(pkg.clone(), dep.rename.clone(), dep.optional))
}

// Cargo reports a versionless path dependency as `*`, even for prerelease
// packages. semver's registry requirement `*` does not match prereleases.
fn declaration_matches_version(dep: &Dependency, pkg: &Package) -> bool {
	(dep.path.is_some() && dep.req == semver::VersionReq::STAR) || dep.req.matches(&pkg.version)
}

/// Resolve a dependency using its package identity, manifest alias, kind and target.
pub(crate) fn resolve_dep_from_graph(
	pkg: &Package,
	dep: &Dependency,
	(meta, resolve): (&Metadata, &Resolve),
) -> Option<RenamedPackage> {
	let resolved_pkg = resolve.nodes.iter().find(|node| node.id == pkg.id)?;
	let renamed = dep.rename.as_ref().map(|name| name.replace('-', "_"));
	let mut matches = resolved_pkg.deps.iter().filter_map(|node| {
		if renamed.as_ref().is_some_and(|name| node.name != *name) ||
			!node
				.dep_kinds
				.iter()
				.any(|kind| kind.kind == dep.kind && kind.target == dep.target)
		{
			return None
		}
		let candidate = meta.packages.iter().find(|pkg| pkg.id == node.pkg)?;
		// Cargo has already resolved the source and version, including patches
		// and versionless git dependencies on prerelease packages.
		if candidate.name != dep.name {
			return None
		}
		let name_matches = match &renamed {
			Some(_) => true,
			None =>
				candidate.targets.iter().any(|target| {
					(target.is_lib() ||
						target.is_proc_macro() ||
						target.is_rlib() || target.is_dylib() ||
						target.is_cdylib() || target.is_staticlib()) &&
						target.name == node.name
				}),
		};
		name_matches.then_some(candidate)
	});
	let candidate = matches.next()?;
	// Do not pick an arbitrary package if the graph is ambiguous.
	if matches.any(|other| other.id != candidate.id) {
		return None
	}
	Some(RenamedPackage::new(candidate.clone(), dep.rename.clone(), dep.optional))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenamedPackage {
	pub pkg: Package,
	pub rename: Option<String>,
	pub optional: bool,
}

impl RenamedPackage {
	pub fn new(pkg: Package, rename: Option<String>, optional: bool) -> Self {
		Self { pkg, rename, optional }
	}

	pub(crate) fn manifest_name(&self) -> &str {
		self.rename.as_deref().unwrap_or(self.pkg.name.as_str())
	}

	pub fn name(&self) -> String {
		self.manifest_name().to_owned()
	}

	pub fn original_name(&self) -> String {
		self.pkg.name.to_string()
	}

	pub fn display_name(&self) -> String {
		match &self.rename {
			Some(rename) => format!("{} (renamed from {})", rename, self.pkg.name),
			None => self.pkg.name.to_string(),
		}
	}
}

impl Ord for RenamedPackage {
	fn cmp(&self, other: &Self) -> std::cmp::Ordering {
		// Yikes... dafuq is this?!
		//bincode::serialize(self).unwrap().cmp(&bincode::serialize(other).unwrap())

		(&self.pkg.name, &self.pkg.id).cmp(&(&other.pkg.name, &other.pkg.id))
	}
}

impl PartialOrd for RenamedPackage {
	fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
		Some(self.cmp(other))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn resolves_alias_by_kind_and_target_from_cargo_metadata() {
		use std::fs;

		let dir = tempfile::tempdir().unwrap();
		let root = dir.path();
		fs::create_dir_all(root.join(".cargo")).unwrap();
		fs::create_dir_all(root.join("src")).unwrap();
		fs::write(root.join("src/lib.rs"), "").unwrap();
		fs::write(
			root.join(".cargo/config.toml"),
			"[source.crates-io]\nreplace-with = 'vendored-sources'\n\
			 [source.vendored-sources]\ndirectory = 'vendor'\n",
		)
		.unwrap();
		fs::write(
			root.join("Cargo.toml"),
			r#"
[package]
name = "fixture"
version = "0.1.0"
edition = "2021"

[workspace]
members = ["workspace-only"]
exclude = ["vendor"]

[dependencies]
chosen = { package = "shared-package", version = "1" }
other = { package = "shared-package", version = "5" }
lib-package = "1"
workspace-only = { path = "workspace-only" }

[dev-dependencies]
chosen = { package = "shared-package", version = "2" }

[build-dependencies]
chosen = { package = "shared-package", version = "3" }

[target.'cfg(windows)'.dependencies]
chosen = { package = "shared-package", version = "4" }

[features]
forward = ["chosen/extra"]
"#,
		)
		.unwrap();
		fs::create_dir_all(root.join("workspace-only/src")).unwrap();
		fs::write(
			root.join("workspace-only/Cargo.toml"),
			"[package]\nname = 'workspace-only'\nversion = '1.0.0-dev'\nedition = '2021'\n",
		)
		.unwrap();
		fs::write(root.join("workspace-only/src/lib.rs"), "").unwrap();
		for (name, versions, lib_name) in [
			("shared-package", &[1, 2, 3, 4, 5][..], "shared_package"),
			("lib-package", &[1][..], "unusual_lib"),
		] {
			for version in versions {
				let package = root.join(format!("vendor/{name}-{version}.0.0"));
				fs::create_dir_all(package.join("src")).unwrap();
				fs::write(package.join("src/lib.rs"), "").unwrap();
				fs::write(
					package.join("Cargo.toml"),
					format!(
						"[package]\nname = '{name}'\nversion = '{version}.0.0'\nedition = '2021'\n\
					 [lib]\nname = '{lib_name}'\n[features]\nextra = []\n"
					),
				)
				.unwrap();
				fs::write(package.join(".cargo-checksum.json"), r#"{"files":{},"package":null}"#)
					.unwrap();
			}
		}

		let metadata = MetadataCommand::new()
			.current_dir(root)
			.other_options(vec!["--offline".into()])
			.exec()
			.unwrap();
		let fixture = metadata.packages.iter().find(|pkg| pkg.name == "fixture").unwrap();
		for dep in &fixture.dependencies {
			let resolved = resolve_dep(fixture, dep, &metadata).unwrap();
			let expected = if dep.name == "shared-package" {
				if dep.rename.as_deref() == Some("other") {
					5
				} else if dep.target.is_some() {
					4
				} else {
					match dep.kind {
						cargo_metadata::DependencyKind::Normal => 1,
						cargo_metadata::DependencyKind::Development => 2,
						cargo_metadata::DependencyKind::Build => 3,
						_ => panic!("unexpected dependency kind"),
					}
				}
			} else {
				1
			};
			assert_eq!(resolved.pkg.name, dep.name);
			assert_eq!(resolved.pkg.version.major, expected, "{dep:?}");
		}
		let graph = crate::cmd::lint::build_feature_dag(&metadata, &metadata.packages).unwrap();
		let from = crate::cmd::lint::CrateAndFeature(fixture.id.repr.clone(), "forward".into());
		for pkg in metadata.packages.iter().filter(|p| p.name == "shared-package") {
			let reached =
				graph.implied_path(&from, |node| node.0 == pkg.id.repr && node.1 == "extra");
			assert_eq!(reached.is_some(), matches!(pkg.version.major, 1 | 4), "{}", pkg.id);
		}

		let workspace_metadata = MetadataCommand::new()
			.current_dir(root)
			.no_deps()
			.other_options(vec!["--offline".into()])
			.exec()
			.unwrap();
		let fixture = workspace_metadata.packages.iter().find(|pkg| pkg.name == "fixture").unwrap();
		let workspace_dep =
			fixture.dependencies.iter().find(|dep| dep.name == "workspace-only").unwrap();
		assert_eq!(
			resolve_dep(fixture, workspace_dep, &workspace_metadata).unwrap().pkg.name,
			"workspace-only"
		);
		let mut wrong_version = workspace_dep.clone();
		wrong_version.req = "2".parse().unwrap();
		assert!(resolve_dep(fixture, &wrong_version, &workspace_metadata).is_none());
		let mut wrong_path = workspace_dep.clone();
		wrong_path.path = Some(root.join("src").try_into().unwrap());
		assert!(resolve_dep(fixture, &wrong_path, &workspace_metadata).is_none());
		let mut wrong_source = workspace_dep.clone();
		wrong_source.path = None;
		wrong_source.source = fixture
			.dependencies
			.iter()
			.find(|dep| dep.name == "shared-package")
			.unwrap()
			.source
			.clone();
		assert!(resolve_dep(fixture, &wrong_source, &workspace_metadata).is_none());
	}

	#[test]
	fn format_metadata_error_preserves_multiline_stderr() {
		let err = cargo_metadata::Error::CargoMetadata {
			stderr: "error: failed to parse manifest\n\nCaused by:\n  invalid toml\n".to_string(),
		};
		let formatted = format_metadata_error(err);
		assert_eq!(
			formatted,
			"Failed to load metadata (`cargo metadata` failed):\nerror: failed to parse manifest\n\nCaused by:\n  invalid toml"
		);
		assert!(!formatted.contains("\\n"));
	}

	#[test]
	fn format_metadata_error_handles_empty_stderr() {
		let err = cargo_metadata::Error::CargoMetadata { stderr: "  \n".to_string() };
		assert_eq!(
			format_metadata_error(err),
			"Failed to load metadata: `cargo metadata` exited with an error"
		);
	}

	#[test]
	fn locked_error_still_detected_after_formatting() {
		let err = cargo_metadata::Error::CargoMetadata {
			stderr: "error: the lock file Cargo.lock needs to be updated but --locked was passed to prevent this\n"
				.to_string(),
		};
		let formatted = format_metadata_error(err);
		assert!(check_for_locked_error(&formatted));
	}
}
