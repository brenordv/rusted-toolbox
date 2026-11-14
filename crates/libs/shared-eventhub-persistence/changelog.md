# Changelog

## 1.0.0
- Initial release: redb message archive per data folder (bound to one event
  hub) implementing `MessageSink` with a group-commit writer thread, archive
  search by time window and text, txt/csv/json exporters with an export log,
  the disk-write modes (`WritingSink`), and `config.json` with a separate
  secrets store.
- Dependency: `redb` 4.6.0.
