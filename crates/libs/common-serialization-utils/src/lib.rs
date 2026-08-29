//! Loads serialized data (JSON) from a file into a typed object.
//!
//! [` load_json_file_to_object `](load_json_file_to_object::load_json_file_to_object)
//! reads a file asynchronously, strips a leading UTF-8 BOM, and deserializes the
//! JSON into any `DeserializeOwned` type.

pub mod load_json_file_to_object;

pub use load_json_file_to_object::load_json_file_to_object;
