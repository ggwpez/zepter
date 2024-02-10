// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

#![doc = include_str!("../README.md")]

pub mod autofix;
pub mod cmd;
pub mod config;
pub mod dag;
pub mod grammar;
pub mod mock;
mod tests;

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

/// Internal use only.
pub mod log {
	pub use crate::{debug, error, info, trace, warn};
}

#[macro_export]
macro_rules! info {
	($($arg:tt)*) => {
		#[cfg(feature = "logging")]
		{
			::log::info!($($arg)*);
		}
	};
}

#[macro_export]
macro_rules! warn {
	($($arg:tt)*) => {
		#[cfg(feature = "logging")]
		{
			::log::warn!($($arg)*);
		}
	};
}

#[macro_export]
macro_rules! error {
	($($arg:tt)*) => {
		#[cfg(feature = "logging")]
		{
			::log::error!($($arg)*);
		}
	};
}

#[macro_export]
macro_rules! debug {
	($($arg:tt)*) => {
		#[cfg(feature = "logging")]
		{
			::log::debug!($($arg)*);
		}
	};
}

#[macro_export]
macro_rules! trace {
	($($arg:tt)*) => {
		#[cfg(feature = "logging")]
		{
			::log::trace!($($arg)*);
		}
	};
}

/// Convert the error or a `Result` into a `String` error.
pub(crate) trait ErrToStr<R> {
	fn err_to_str(self) -> Result<R, String>;
}

impl<R, E: std::fmt::Display> ErrToStr<R> for Result<R, E> {
	fn err_to_str(self) -> Result<R, String> {
		self.map_err(|e| format!("{}", e))
	}
}
