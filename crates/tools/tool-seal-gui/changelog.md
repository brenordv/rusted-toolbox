# Changelog

## 1.0.0
- Initial release: the themed shell.
- Dark theme and widget recipes from `common-gui`; Text | Files | Keys tabs
  (placeholders for now); toolbar and status bar.
- Background worker bridge: single worker thread, bounded per-frame event
  drain, repaint wake after every event, clean shutdown on window close, and
  a "worker stopped" state instead of a silent freeze.
- "Send a test toast" status-bar action proves the job pipeline end to end.
- File logging on by default at `warn` (`~/.seal-gui/logs/seal-gui.log`);
  job failures always reach the log file.
