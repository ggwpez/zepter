// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

#![doc = include_str!("../README.md")]

pub mod autofix;
pub mod cmd;
pub mod dag;
pub mod mock;

pub mod prelude {
	pub use super::{
		dag::{Dag, Path},
		CrateId,
	};
}

/// Unique Id of a Rust crate.
///
/// These come in the form of:
/// `<NAME> <VERSION> (<SOURCE>)`  
/// You can get an idea by using `cargo metadata | jq '.packages' | grep '"id"'`.
pub type CrateId = String;
