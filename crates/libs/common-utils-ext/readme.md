# common-utils-ext

Shared helpers that pull in heavier dependencies than `common-utils` allows.
The split exists so dependency-light tools do not pay for `arboard`, `uuid`, or
`regex` when all they need is a string formatter.

- `copy_to_clipboard(&str)`: writes to the system clipboard via `arboard`.
  Fails in headless environments; the error carries context.
- `new_guid()`: a random (v4) UUID as a hyphenated, lowercase string.
- `clean_str_regex(&str)`: strips non-printable characters (Unicode category
  `C`: controls, zero-widths, BOM) with a precompiled regex; returns a
  `Cow<str>` so clean input is not reallocated.
