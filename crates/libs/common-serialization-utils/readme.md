# common-serialization-utils

Loads a JSON file into a typed object. One async helper:

- `load_json_file_to_object::<T>(&Path)`: reads the file with tokio, strips a
  leading UTF-8 BOM, and deserializes into any `DeserializeOwned` type.

## Contract

- Read failures carry the context `failed to read file <path>`; parse failures
  (invalid JSON, or valid JSON that does not match `T`'s shape) carry
  `failed to parse JSON from <path>`.
- Only a leading UTF-8 BOM is handled; UTF-16/32 files are not supported.

The crate name reserves room for a serialize half; today it only loads.
