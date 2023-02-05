pub mod dag;
pub mod cmd;

use core::fmt::{Display, Formatter, Result};
use std::collections::{HashMap, BTreeMap};

#[derive(Clone, Debug)]
pub struct Crate {
	id: String,
	name: String,
	version: String,
	features: HashMap<String, Vec<String>>,
}

impl Crate {
	fn without_features(self) -> Self {
		Self {
			features: Default::default(),
			..self
		}
	}
}

impl Display for Crate {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		write!(f, "{}{}{}", self.name, if self.version.is_empty() { "".into() } else {format!(" v{}", self.version)}, if self.features.is_empty() { "".to_string() } else { format!(" ({:?})", self.features) })
	}
}

impl PartialEq for Crate {
	fn eq(&self, other: &Self) -> bool {
		self.id == other.id
	}
}

impl Eq for Crate {}


impl Ord for Crate {
	fn cmp(&self, other: &Self) -> std::cmp::Ordering {
		self.id.cmp(&other.id)
	}
}

impl PartialOrd for Crate {
	fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
		Some(self.cmp(other))
	}
}
