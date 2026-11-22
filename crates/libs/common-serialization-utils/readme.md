# common-serialization-utils

Loads a JSON file into a typed object and saves one back. Two helpers:

- `load_json_file_to_object::<T>(&Path)`: reads the file with tokio, strips a
  leading UTF-8 BOM, and deserializes into any `DeserializeOwned` type. Async.
- `save_object_to_json_file(&Path, &T)`: writes any `Serialize` value as
  pretty JSON with a final newline. Synchronous.

## Contract

- Read failures carry the context `failed to read file <path>`; parse failures
  (invalid JSON, or valid JSON that does not match `T`'s shape) carry
  `failed to parse JSON from <path>`.
- Only a leading UTF-8 BOM is handled; UTF-16/32 files are not supported.
- A save creates the parent folder when it is missing and replaces the file
  atomically (a temporary file beside it, then a rename), so a failed save
  leaves an existing file as it was. Its failures carry
  `failed to serialize JSON for <path>`, `failed to create folder <parent>`, or
  `failed to write file <path>`.
