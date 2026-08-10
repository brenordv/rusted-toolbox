# Workspace Changelog

## v1

First release after the major refactor. This version reorganizes the whole
workspace, modernizes the dependency tree, and adds CI/CD.

### Repository structure
- Split the flat `crates/<name>` layout into `crates/libs/*` for the library
  crates (`ai-macros`, `ai-shared`, `shared`, `shared-eventhub`) and
  `crates/tools/*` for the 27 binary tools.
- Rewrote the root workspace manifest and every per-crate `Cargo.toml` to point
  at the new paths.
- Updated `build.sh`, `build.bat`, and the macOS/Ubuntu convenience build
  scripts for the new layout.
- Revised `README.md` and `CONTRIBUTING.md` to match the new structure.

### CI/CD
- Added GitHub Actions workflows for CI (`ci.yml`) and release (`release.yml`).
- Pinned the toolchain with `rust-toolchain.toml`.

### Dependencies
- Bumped the workspace dependency set to current versions, including the major
  upgrades `fake` 4 -> 5, `hurl`/`hurl_core` 7 -> 8, `jsonwebtoken` 9 -> 11,
  `rand` 0.9 -> 0.10, `ratatui` 0.29 -> 0.30, `reqwest` 0.12 -> 0.13, `syn`
  2 -> 3, `rusqlite` 0.38 -> 0.40, `string-interner` 0.19 -> 0.20, and
  `surge-ping` 0.8 -> 0.9, plus many minor and patch bumps across the tree.
- Made `reqwest` 0.13 the workspace default. `tool-netquality` pins `reqwest`
  0.12 to stay compatible with `cfspeedtest`.

### Code updated for the new dependency APIs
- `tool-jwt`: decode via `jsonwebtoken`'s `dangerous::insecure_decode`, dropping
  the manual validation and algorithm-list setup.
- `tool-mock` and `tool-whurl`: moved from `rand::Rng` to `rand::RngExt` for the
  `rand` 0.10 API.
- `tool-whurl`: switched `follow_location` to the `hurl` 8
  `FollowLocation`/`CredentialForwarding` API.