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

/// Resolve the dependency `dep` within the workspace.
///
/// Errors if `dep` is not a workspace member.
pub(crate) fn resolve_dep_from_workspace(
	dep: &Dependency,
	meta: &Metadata,
) -> Option<RenamedPackage> {
	for work in meta.workspace_packages() {
		if work.name == dep.name {
			let pkg = meta.packages.iter().find(|pkg| pkg.id == work.id).cloned();
			return pkg.map(|pkg| RenamedPackage::new(pkg, dep.rename.clone(), dep.optional))
		}
	}
	None
}

/// Resolve the dependency `dep` of `pkg` within the resolve graph.
///
/// The resolve graph should only be used for external crates. I did not try what happens for
/// workspace members - better don't do it.
pub(crate) fn resolve_dep_from_graph(
	pkg: &Package,
	dep: &Dependency,
	(meta, resolve): (&Metadata, &Resolve),
) -> Option<RenamedPackage> {
	let dep_name = dep.rename.clone().unwrap_or(dep.name.clone()).replace('-', "_");
	let resolved_pkg = resolve.nodes.iter().find(|node| node.id == pkg.id)?;
	let resolved_dep_id = resolved_pkg.deps.iter().find(|node| node.name == dep_name)?;
	let resolve_dep = meta.packages.iter().find(|pkg| pkg.id == resolved_dep_id.pkg)?;

	Some(RenamedPackage::new(resolve_dep.clone(), dep.rename.clone(), dep.optional))
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
