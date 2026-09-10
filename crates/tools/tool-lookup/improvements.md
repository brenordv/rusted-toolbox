# Basic
[X] Migrate cli to the new pattern.
[X] Add more test coverage to the tool. (added clap debug assertions, matcher engine tests, binary-sniff unit tests, and end-to-end binary-skip and invalid-UTF-8 counting tests; 10 -> 25 tests)
[X] Research improvements to the tool. (including how to detect binary files, so we can avoid them in the text search) (outcome: binary detection implemented as a NUL sniff over the first 8 KiB, local to the crate; the research also surfaced a dangling `required_unless_present` on the text positional and help text advertising a fake `--regex` flag, both fixed in 3.1.0)
[ ] Promote the binary sniff to common-file-utils when a second tool needs it (planned lib idea).
