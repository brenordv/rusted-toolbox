# shared-eventhub

> **On hold:** this crate is frozen until the official Microsoft Event Hubs crate for Rust ships
> connection-string support; the planned refactor (shared with `eh-read` and `eh-export`) moves it
> onto that SDK. It keeps known warnings and an empty changelog until then.

Shared library machinery behind the `eh-read` and `eh-export` tools: Event Hub config models and
traits, CLI argument presets, logging setup, and utilities (connection-string parsing, database
paths, message filtering).
