// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

//! Grammar helpers for printing correct English.

/// Add an plural `s` for English grammar iff `n != 1`.
pub(crate) fn plural(n: usize) -> &'static str {
	if n == 1 {
		""
	} else {
		"s"
	}
}
