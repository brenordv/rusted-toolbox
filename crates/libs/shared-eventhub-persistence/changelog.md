# Changelog

## 1.0.0
- Initial release: redb message archive per data folder (bound to one event
  hub) implementing `MessageSink` with a group-commit writer thread, archive
  search by time window and text, txt/csv/json exporters with an export log,
  the disk-write modes (`WritingSink`), and `config.json` with a separate
  secrets store.
- `ExportFormat::extension` is public, for naming a single-message export.
- A data folder that already holds files is used only when its `.gitignore`
  is eh's own `*`; config load errors redact the name in "unknown field" and
  "unknown variant" messages; `ExportOutcome::cancelled` says whether an
  export stopped early on its cancel flag; `resolve_target_with` and
  `needs_stored_secret` resolve a target from given secrets and read the
  secrets file only for the ones missing; `ToolConfig::engine_config` maps a
  config's capture settings and a resolved target to the engine's run
  settings.
- The archive binding records the event hub's creation time and refuses a hub
  of the same name created at another time, naming both times, or one created
  more than a minute after the archive's earliest event (a deleted and
  recreated hub). Archives bound earlier record the time on their next bind;
  older builds ignore it and bind by name.
- Dependency: `redb` 4.6.0.
