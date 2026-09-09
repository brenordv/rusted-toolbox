# Basic
[x] Add more test coverage to the tool, if reasonable. Also let's check we if we can group similar tests and group them using rstest.
    Outcome (2.0.1): grouped the sanitize/duration/bytes example tables with rstest; added cases for negative-duration magnitude and TB as the largest unit.
[x] Research improvements to the app.
    Outcome (2.0.1): reviewed all four modules; no defects found. Documented the real guarantees instead (substitution-only sanitize, TB cap in format_bytes_to_string).
[x] Create the `readme` file.
    Outcome: already existed and still accurate; unchanged.
[x] Add summaries to public helpers that have none.
    Outcome (2.0.1): documented the constants, get_current_dir, sanitize_string_for_filename, and both datetime traits.
[ ] Promote a shared escape-for-terminal-display helper (control characters plus Unicode bidi controls) to string_utils: tool-mqtt (format_payload_for_display) and tool-pingx (sanitize_display) hand-roll the same escaping.
