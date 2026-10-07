# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
The workspace carries a single global version (see `docs/architecture.org`);
`scripts/bump_version.sh <major|minor|patch>` cuts release entries.

## [Unreleased]

### Fixed

- CI: coverage job uses `cargo llvm-cov nextest` (the `--nextest` flag was
  removed in llvm-cov 0.9) with `--no-tests=warn`; benchmark job runs plain
  `cargo bench` (`--output-format bencher` is a nightly-only libtest option).
- CI: use the valid `dtolnay/rust-toolchain@stable` ref (the previous `@v1`
  ref does not exist and would have failed every toolchain job on first run).
- CI: mutation-testing workflow reports surviving mutants via artifacts
  instead of failing the scheduled job.
- `LICENSE-MIT` copyright holder corrected to the project author; repository
  URL placeholder (`your-username`) replaced with the real remote in
  `Cargo.toml` and the README badge.

### Added

- SonarCloud static analysis: `sonar-project.properties` and a
  `SonarCloud Analysis` CI job (token via the `SONAR_TOKEN` secret).
- Cargo workspace with six crates: `taskboard-domain`, `taskboard-state`,
  `taskboard-sync-nextcloud`, `taskboard-storage-sqlite`,
  `taskboard-ui-slint`, `taskboard-app` (edition 2024, single global semver).
- Dual-tier licensing: `MIT OR Apache-2.0` for library crates,
  `AGPL-3.0-only` for application/UI crates.
- CI pipeline (fmt/clippy, cargo-deny, nextest + doc tests, llvm-cov
  coverage, criterion benchmarks) plus weekly mutation-testing and Miri
  workflows.
- Project documentation: `docs/architecture.org`,
  `docs/coding_guidelines.org`, `docs/testing_strategy.org`, and ADRs
  0001–0003; `README.org` and `AGENTS.md` agent guidance.
- Release tooling: `scripts/bump_version.sh`, `CHANGELOG.md`.
- Repository hygiene: `.githooks/pre-commit` (fmt + clippy), Dependabot
  config, issue & PR templates, `.editorconfig`, `.gitattributes`.
