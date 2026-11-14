# Changelog

## 1.0.0
- Initial release: redb message archive per data folder (bound to one event
  hub) implementing `MessageSink` with a group-commit writer thread, archive
  search by time window and text, txt/csv/json exporters with an export log,
  the disk-write modes (`WritingSink`), and `config.json` with a separate
  secrets store.
- A data folder that already holds files is used only when its `.gitignore`
  is eh's own `*`; config load errors redact the name in "unknown field" and
  "unknown variant" messages; `ExportOutcome::cancelled` says whether an
  export stopped early on its cancel flag; `resolve_target_with` and
  `needs_stored_secret` resolve a target from given secrets and read the
  secrets file only for the ones missing; `ToolConfig::engine_config` maps a
  config's capture settings and a resolved target to the engine's run
  settings.
- Dependency: `redb` 4.6.0.
