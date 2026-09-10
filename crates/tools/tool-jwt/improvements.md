# Basic
[X] Migrate cli to the new pattern.
[X] Add more test coverage to the tool. Added tests for token normalization, CSV formatting, clipboard claim resolution, and expiration edge cases.
[X] Research improvements to the tool. Found and fixed the `exp` out-of-range fallback that reported bad input as expired-now; the error!-then-exit-success path on empty claims is documented behavior and was left as is.
[X] Map an exp claim that is present but not an integer (RFC 7519 NumericDate allows fractions) to InvalidExpiration or truncate it, instead of reporting 'No expiration claim'. Outcome (2.0.2): fractional NumericDate truncates toward zero and evaluates normally; non-numeric exp values report InvalidExpiration.
