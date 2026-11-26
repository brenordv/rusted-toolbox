# Library crates

Every crate in `crates/libs` is a library that the tools in `crates/tools` build on. None is
published to crates.io. A lib's name says which of two kinds it is.

## General libs: `common-*`

A general lib holds code any tool can use as it is: CLI boot and logging, file and string helpers,
the Rune design system, Azure sign-in, Ctrl+C handling. Any tool may depend on one.

Name it `common-<what it holds>`, as in `common-cli`, `common-file-utils`, or `common-azure`.

## Tool-owned libs: `<tool>-*`

A tool-owned lib holds one tool's own logic, split out of the tool crate so every binary of that
tool can share it. A tool's CLI and its GUI are one tool here: `eh` and `eh-gui` are the eh tool,
`seal` and `seal-gui` the seal tool. Two tools built on one engine are one owner too: `head` and
`tail` share `head-tail-core`.

Name it `<owner>-<part>`. The owner is the tool's package name (`eh`, `seal`), or both names
joined when two tools share the engine (`head-tail`). A lib that holds the tool's engine ends in
`-core`.

Only the owner depends on a tool-owned lib: the owner's binaries and the owner's other libs.

## Which kind a lib is

Decide by what the code is for, not by how many tools use it today. `common-azure` and
`common-logging-otel` each have one user now and are still general, because nothing in them
belongs to that tool. `head-tail-core` has two users and is still tool-owned, because its count
grammar and backward scan exist for head and tail.

The quick test: if another tool could use the lib without changing it, the lib is general. If its
types or rules only make sense inside one tool (eh's message archive, seal's output naming), the
lib belongs to that tool.

## Dependency direction

- A general lib depends only on other general libs.
- A tool-owned lib depends on general libs and on libs with the same owner (`eh-persistence` uses
  `eh-core`). It never depends on another tool's lib.
- When a second tool needs code that sits in a tool-owned lib, move that code into a general lib
  and have both tools depend on it there. The second tool does not become a user of the
  tool-owned lib.

## The libs today

General: `common-audio`, `common-azure`, `common-cli`, `common-file-utils`, `common-gui-rune`,
`common-logging-otel`, `common-mock-data`, `common-serialization-utils`, `common-signal-monitor`,
`common-utils`, `common-utils-ext`.

Tool-owned:

| Crate            | Owner          |
|------------------|----------------|
| `eh-core`        | eh, eh-gui     |
| `eh-persistence` | eh, eh-gui     |
| `head-tail-core` | head, tail     |
| `seal-crypto`    | seal, seal-gui |

What each lib holds is listed in `CONTRIBUTING.md` under "Project structure", and each crate's
`readme.md` covers its API.

## Adding a lib

1. Pick the kind and the name with the rules above.
2. Create `crates/libs/<name>` with a `readme.md` and a `changelog.md`. The `crates/libs/*` glob
   in the root `Cargo.toml` makes it a workspace member.
3. Add it to the root `[workspace.dependencies]`, in the general or the tool-owned group, and
   depend on it with `<name> = { workspace = true }`.
4. Add it to the list in `CONTRIBUTING.md` and to "The libs today" above.
