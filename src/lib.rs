pub mod dag;
pub mod cmd;

use core::fmt::{Display, Formatter, Result};

#[derive(Clone, Debug)]
pub struct Crate {
	name: String,
	version: String,
	enabled_features: Vec<String>,
	has_features: Vec<String>,
}

impl Crate {
	fn without_features(self) -> Self {
		Self {
			enabled_features: Vec::new(),
			has_features: Vec::new(),
			..self
		}
	}

	fn remove_features(&mut self) {
		self.enabled_features.clear();
		self.has_features.clear();
	}
}

impl Display for Crate {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		write!(f, "{}{}{}", self.name, if self.version.is_empty() { "".into() } else {format!(" v{}", self.version)}, if self.enabled_features.is_empty() { "".to_string() } else { format!(" ({})", self.enabled_features.join(", ")) })
	}
}

// eq of name, version and enabled_features
impl PartialEq for Crate {
	fn eq(&self, other: &Self) -> bool {
		self.name == other.name && self.version == other.version && self.enabled_features == other.enabled_features
	}
}

impl Eq for Crate {}


// Ord of name, version and enabled_features
impl Ord for Crate {
	fn cmp(&self, other: &Self) -> std::cmp::Ordering {
		self.name.cmp(&other.name).then(self.version.cmp(&other.version)).then(self.enabled_features.cmp(&other.enabled_features))
	}
}

impl PartialOrd for Crate {
	fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
		Some(self.cmp(other))
	}
}
