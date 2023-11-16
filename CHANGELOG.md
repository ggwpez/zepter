# Changelog

## [0.15.0] - 2023-11-14

### Added
- Subcommand `transpose features strip-dev-only` to remove dev-only features as preparation for publishing.
- Arg `--dep-kinds` to `propagate-feature` to allow to exclude specific dep kinds.

## [0.14.0] - 2023-11-07

### Added
- Global flag `--fix-hint={on,off}` to hide the hint of how to fix something.

### Fixed
- Typos in the fix-hint output.

### Changed
- Workflows do not show the fix-hint anymore since they can provide their own.
