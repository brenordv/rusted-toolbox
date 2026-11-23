//! Loads serialized data (JSON) from a file into a typed object, and saves one back.
//!
//! [` load_json_file_to_object `](load_json_file_to_object::load_json_file_to_object)
//! reads a file asynchronously, strips a leading UTF-8 BOM, and deserializes the
//! JSON into any `DeserializeOwned` type.
//! [`save_object_to_json_file`](save_object_to_json_file::save_object_to_json_file)
//! writes any `Serialize` value as pretty JSON, replacing the file atomically.

pub mod load_json_file_to_object;
pub mod save_object_to_json_file;

pub use load_json_file_to_object::load_json_file_to_object;
pub use save_object_to_json_file::save_object_to_json_file;
