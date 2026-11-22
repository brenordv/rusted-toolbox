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
- `ToolConfig::missing_client_secret` names the client whose secret a resolved target lacks,
  `ToolConfig::disk_write_config` builds a capture run's disk-write settings for a data folder,
  and `DataPaths::export_dir` places the export folder in the data folder (an absolute one as
  given).
- `workspace`: the folder of saved connection profiles (`Workspace::locate` reads `EH_WORKSPACE`
  or uses `<home>/.eh/profiles`), `ProfileName` (a name safe as a folder on every platform),
  finding a profile ignoring case, listing profiles without a log line per profile, and
  `ConfigHome`, whose `data_paths` places a relative data folder inside a profile folder.
- `ConfigHome::uses_recalled_folder` says whether a home uses a folder its `config.json` names:
  any folder under a profile, only a plain relative one under the current folder.
- `MessageStore::write_failure` returns why a handle's writes fail (the failed batch's cause or
  the writer thread's), set before the failed batch is answered, and
  `MessageStore::is_last_handle` says whether a close on the handle would release the archive.
  The dev-only `test-support` feature makes `store::test_support` public (`failing_store`,
  `store_with_dead_writer`) for other crates' tests.
- Exports with metadata carry the event's content type, correlation id, and application
  properties: after the capture time in the txt details block (line breaks, the other line
  separators, and backslashes written as escapes), and the columns `content_type`,
  `correlation_id`, and `properties` (a JSON object) before `message_content` in csv and json. The csv header changed with it, so a condensed csv
  that has the earlier five-column header is refused on append by the header check; a condensed
  json file takes the new rows. `WritingSink`'s message files use the same details block and
  gain the three entries. Exports without metadata are unchanged.
- Dependency: `redb` 5.0.0.
