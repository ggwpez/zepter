# Changelog

## [1.1.0] - 2024-02-06

### Changed
- Make `transpose` smarter and add tests for `dependency lift-to-workspace`.

## [1.0.2] - 2024-02-02

### Fixed
- Let `transpose` exit with code 1 on error instead of panic.

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
