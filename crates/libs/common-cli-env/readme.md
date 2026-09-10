# common-cli-env

Loads a tool's `.env` file into the process environment. Call
`load_env_variables()` early in `main`, before argument parsing.

## Search order and precedence

1. The executable's directory.
2. The current working directory.

The first directory that contains a `.env` wins; the other is not read.
Variables already present in the process environment are never overridden, so a
loaded `.env` can only fill in unset variables. When no `.env` is found, the
error lists every directory that was checked.

## Trust caveats

- The working-directory fallback means running a tool inside an untrusted
  directory imports that directory's `.env`. The risk only applies when no
  `.env` sits next to the binary, since the executable's directory is checked
  first.
- dotenvy echoes a malformed line back verbatim inside its parse error, so a
  mistyped secret can surface on stderr or in a log file.
