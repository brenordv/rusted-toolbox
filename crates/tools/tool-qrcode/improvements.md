# Basic
[X] Migrate cli to the new pattern.
[X] Add more test coverage to the tool. Outcome (2.0.1): added tests for wifi payload escaping, the `--wifi-auth` allowlist, extension-mismatch detection, and oversized-input errors.
[X] Research improvements to the tool. Outcome (2.0.1): found and fixed the unescaped wifi payload and the cleartext password in the header and debug log; the extension-behavior surprise (`-o x.svg` producing `x.svg.png`) is now mitigated with a warning.
[ ] Infer the output image format from the -o extension when -f is not given (explicit -f wins); replaces the mismatch warning with the expected behavior.
