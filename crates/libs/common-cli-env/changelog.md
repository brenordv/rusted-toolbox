# Changelog

## 1.1.1
- Created the readme: search order, "already-set process variables win"
  precedence, and the trust caveats (working-directory fallback, dotenvy's
  raw-line echo in parse errors).
- Added a test pinning that the process environment wins over a `.env` value.

## 1.1.0
- Swapped the unmaintained `dotenv` dependency for `dotenvy` (RUSTSEC-2021-0141
  lists it as the maintained alternative).
- The "no .env found" error now lists every directory that was checked, instead
  of a generic failure message; failures resolving the executable or working
  directory no longer vanish silently.
- Documented the search order, the working-directory trust caveat, and that
  `dotenvy` echoes a malformed line back inside its parse error.
- Added the first unit tests.
- Registered the crate in the workspace dependencies so tools can adopt it.

## 1.0.0
- Initial release: `.env` loader searching the executable directory then the
  current working directory.