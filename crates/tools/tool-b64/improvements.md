# Basic
[X] Migrate cli to the new pattern.
[X] Add more test coverage to the tool. Added WrapWriter unit tests (wrap boundaries, trailing newline deferred to finish, no-wrap passthrough), directory-path inference, empty-input encode, and ignore-garbage decode of whitespace-only and padding-only input.
[X] Research improvements to the app.
[ ] Adopt a shared broken-pipe-as-clean-exit helper from common-cli when one exists (planned lib idea; b64 hand-rolls AppError::broken_pipe today).
