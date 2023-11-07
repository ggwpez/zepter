// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

//! Very simple semver implementation. Only supports `major.minor.patch`.
//!
//! Used to lock in config format and binary version.

use serde::{de, Deserialize, Deserializer};
use std::fmt::{self, Display, Formatter};

/// A semantic version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Semver {
	/// The major version.
	pub major: u8,
	/// The minor version.
	pub minor: u8,
	/// The patch version.
	pub patch: u8,
}

impl TryFrom<&str> for Semver {
	type Error = ();

	fn try_from(s: &str) -> Result<Self, Self::Error> {
		let mut parts = s.split('.');
		let major = parts.next().ok_or(())?.parse().map_err(|_| ())?;
		let minor = parts.next().unwrap_or("0").parse().map_err(|_| ())?;
		let patch = parts.next().unwrap_or("0").parse().map_err(|_| ())?;

		Ok(Self { major, minor, patch })
	}
}

impl From<(u8, u8, u8)> for Semver {
	fn from((major, minor, patch): (u8, u8, u8)) -> Self {
		Self { major, minor, patch }
	}
}

impl Display for Semver {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
	}
}

impl Semver {
	pub fn from_serde<'de, D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		let s = String::deserialize(deserializer)?;
		Self::try_from(s.as_str()).map_err(|_| de::Error::custom("Invalid semver"))
	}

	/// Whether `self` is newer or equal to `other`.
	///
	/// "Newer" in this case means compatible in the semver sense.
	pub fn is_newer_or_equal(&self, other: &Self) -> bool {
		self.major > other.major ||
			(self.major == other.major &&
				(self.minor > other.minor ||
					(self.minor == other.minor && self.patch >= other.patch)))
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn parser_semver_works() {
		assert_eq!(Semver::try_from("1").unwrap(), Semver::from((1, 0, 0)));

		assert_eq!(Semver::try_from("1.2").unwrap(), Semver::from((1, 2, 0)));

		assert_eq!(Semver::try_from("1.2.3").unwrap(), Semver::from((1, 2, 3)));
	}

	#[test]
	fn semver_display_works() {
		assert_eq!(Semver::from((1, 2, 3)).to_string(), "1.2.3");
	}

	#[test]
	fn semver_from_serde_works() {
		#[derive(Deserialize)]
		struct Embedding {
			#[serde(deserialize_with = "Semver::from_serde")]
			version: Semver,
		}

		let s = r#"
			{ "version": "1.2.3" }
		"#;

		let embedding = serde_json::from_str::<Embedding>(s).unwrap();
		assert_eq!(embedding.version, Semver::from((1, 2, 3)));
	}

	#[test]
	fn semver_is_newer_or_equal_works() {
		assert!(Semver::from((1, 2, 3)).is_newer_or_equal(&Semver::from((1, 2, 3))));

		assert!(Semver::from((1, 2, 3)).is_newer_or_equal(&Semver::from((1, 2, 2))));
		assert!(Semver::from((1, 2, 3)).is_newer_or_equal(&Semver::from((1, 1, 3))));
		assert!(Semver::from((1, 2, 3)).is_newer_or_equal(&Semver::from((0, 2, 3))));

		assert!(!Semver::from((1, 2, 3)).is_newer_or_equal(&Semver::from((1, 2, 4))));
		assert!(!Semver::from((1, 2, 3)).is_newer_or_equal(&Semver::from((1, 3, 3))));
		assert!(!Semver::from((1, 2, 3)).is_newer_or_equal(&Semver::from((2, 2, 3))));
	}
}
