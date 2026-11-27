// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use super::*;

fn parse(yaml: &str) -> Result<WorkflowFile, String> {
	yaml.parse()
}

#[test]
fn workflow_file_from_yaml_works() {
	let cfg = WorkflowFile::from_path("presets/polkadot.yaml").unwrap();
	assert_eq!(cfg.workflows.len(), 2);
	assert_eq!(cfg.workflow("check").unwrap().0.len(), 2);
	assert_eq!(cfg.workflow("default").unwrap().0.len(), 2);
}

#[test]
fn from_path_missing_file() {
	let err = WorkflowFile::from_path("nonexistent.yaml").unwrap_err();
	assert!(err.contains("Failed to read config file"), "{err}");
}

#[test]
fn rejects_unsupported_format_version() {
	let yaml = "
version:
  format: 2
  binary: 0.1.0
workflows: {}
";
	let err = parse(yaml).unwrap_err();
	assert!(err.contains("version '1'"), "{err}");
}

#[test]
fn rejects_invalid_yaml() {
	let Err(err) = parse("not: valid: yaml: {{{}}}") else { panic!("expected error") };
	assert!(err.contains("yaml parsing"), "{err}");
}

#[test]
fn workflow_lookup_missing_returns_none() {
	let yaml = "
version:
  format: 1
  binary: 0.1.0
workflows:
  check:
    - ['lint', 'propagate-feature']
";
	let cfg = parse(yaml).unwrap();
	assert!(cfg.workflow("nonexistent").is_none());
}

#[test]
fn resolve_references() {
	let yaml = "
version:
  format: 1
  binary: 0.1.0
workflows:
  base:
    - ['lint', 'propagate-feature']
  derived:
    - [ $base.0, '--fix' ]
";
	let cfg = parse(yaml).unwrap();
	let derived = cfg.workflow("derived").unwrap();
	assert_eq!(derived.0.len(), 1);
	assert!(derived.0[0].0.contains(&"lint".to_string()));
	assert!(derived.0[0].0.contains(&"--fix".to_string()));
}

#[test]
fn resolve_bad_index_errors() {
	let yaml = "
version:
  format: 1
  binary: 0.1.0
workflows:
  base:
    - ['lint']
  broken:
    - [ '$base.notanumber' ]
";
	let err = parse(yaml).unwrap_err();
	assert!(err.contains("Failed to parse index"), "{err}");
}

#[test]
fn resolve_missing_workflow_errors() {
	let yaml = "
version:
  format: 1
  binary: 0.1.0
workflows:
  broken:
    - [ '$nonexistent.0' ]
";
	let err = parse(yaml).unwrap_err();
	assert!(err.contains("Failed to find workflow"), "{err}");
}

#[test]
fn fmt_help_with_links() {
	let cfg = WorkflowFile::from_path("presets/polkadot.yaml").unwrap();
	let help = cfg.fmt_help().unwrap();
	assert!(help.contains("Polkadot-SDK"), "{help}");
	assert!(help.contains("For more information"), "{help}");
}

#[test]
fn fmt_help_without_links() {
	let yaml = "
version:
  format: 1
  binary: 0.1.0
workflows: {}
help:
  text: |
    some help text
  links: []
";
	let cfg = parse(yaml).unwrap();
	let help = cfg.fmt_help().unwrap();
	assert!(help.contains("some help text"), "{help}");
	assert!(!help.contains("For more information"), "{help}");
}

#[test]
fn fmt_help_none() {
	let yaml = "
version:
  format: 1
  binary: 0.1.0
workflows: {}
";
	let cfg = parse(yaml).unwrap();
	assert!(cfg.fmt_help().is_none());
}

#[test]
fn fmt_help_preserves_text_without_trailing_newline() {
	let yaml = "
version:
  format: 1
  binary: 0.1.0
workflows: {}
help:
  text: 'no trailing newline'
  links: []
";
	let cfg = parse(yaml).unwrap();
	let help = cfg.fmt_help().unwrap();
	assert!(help.contains("no trailing newline"), "{help}");
}

/// BUG: `$ref` without a `.index` should return Err, not panic.
#[test]
fn resolve_ref_without_dot_returns_error() {
	let yaml = "
version:
  format: 1
  binary: 0.1.0
workflows:
  base:
    - ['lint']
  broken:
    - [ '$base' ]
";
	let err = parse(yaml).unwrap_err();
	assert!(err.contains("base"), "{err}");
}

/// BUG: out-of-bounds index should return Err, not panic.
#[test]
fn resolve_out_of_bounds_index_returns_error() {
	let yaml = "
version:
  format: 1
  binary: 0.1.0
workflows:
  base:
    - ['lint']
  broken:
    - [ '$base.99' ]
";
	let err = parse(yaml).unwrap_err();
	assert!(err.contains("99"), "{err}");
}

#[test]
fn check_cfg_compatibility_passes_for_current_version() {
	let yaml = format!(
		"
version:
  format: 1
  binary: {}
workflows: {{}}
",
		clap::crate_version!()
	);
	let cfg = parse(&yaml).unwrap();
	cfg.check_cfg_compatibility().unwrap();
}

#[test]
fn check_cfg_compatibility_fails_for_future_version() {
	let yaml = "
version:
  format: 1
  binary: 255.0.0
workflows: {}
";
	let cfg = parse(yaml).unwrap();
	let err = cfg.check_cfg_compatibility().unwrap_err();
	assert!(err.contains("too old"), "{err}");
}
