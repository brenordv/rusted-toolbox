use crate::models::open_ai::ChatCompletion;
use anyhow::{Context, Result};
use chrono::Local;
use shared::system::ensure_directory_exists::EnsureDirectoryExists;
use shared::system::get_current_working_dir::get_current_working_dir_str;
use shared::system::resolve_path_with_base::resolve_path_with_base;
use shared::utils::datetime_utc_utils::DateTimeUtilsExt;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

pub struct RequestLogger {
    runtime_history_path: PathBuf,
    request_timestamp: String,
}

impl RequestLogger {
    pub fn new(request_history_path: String) -> Result<Self> {
        let cwd = get_current_working_dir_str()?;

        let runtime_history_path = resolve_path_with_base(&cwd, request_history_path.as_str());

        runtime_history_path.ensure_directory_exists()?;

        Ok(Self {
            runtime_history_path,
            request_timestamp: String::new(),
        })
    }

    pub fn set_request_timestamp_local(&mut self) {
        self.request_timestamp = Local::now().get_datetime_as_filename_safe_string();
    }

    pub fn save_request(&self, new_request: &ChatCompletion) -> Result<()> {
        let filename = self.get_new_request_file();

        // Since this is used only for error logging, no need to bail out in case of error.
        let filename_str = filename.to_str().unwrap_or("<non-utf8 path>");

        let json = serde_json::to_string_pretty(&new_request).context(format!(
            "Failed to serialize request to json: {}",
            filename_str
        ))?;

        self.write_to_file(&filename, &json)?;

        Ok(())
    }

    pub fn save_response(&self, response: &String, status_code: u16) -> Result<()> {
        let filename = self.get_new_response_file(status_code);

        match serde_json::from_str::<serde_json::Value>(response.as_str()) {
            Ok(value) => {
                // The value was parseable to JSON. We're doing this round-trip to save it in a
                // pretty and readable format.
                let pretty =
                    serde_json::to_string_pretty(&value).unwrap_or_else(|_| response.clone());

                self.write_to_file(&filename, &pretty)?;
            }
            Err(_) => {
                // The response cannot be a JSON. Probably the request failed, so we'll save the
                // response as normal text.
                self.write_to_file(&filename, &response)?;
            }
        }

        Ok(())
    }

    fn write_to_file(&self, filename: &PathBuf, content: &str) -> Result<()> {
        // Since this is used only for error logging, no need to bail out in case of error.
        let filename_str = filename.to_str().unwrap_or("<non-utf8 path>");

        let mut file =
            File::create(&filename).context(format!("Failed to create file: {}", filename_str))?;

        file.write_all(content.as_bytes())
            .context(format!("Failed to write to file: {}", filename_str))?;

        Ok(())
    }

    fn get_new_request_file(&self) -> PathBuf {
        let filename = format!("{}.request.json", self.request_timestamp);

        self.runtime_history_path.join(filename)
    }

    fn get_new_response_file(&self, status_code: u16) -> PathBuf {
        let file_ext = if status_code == 200 { "json" } else { "txt" };

        let filename = format!(
            "{}.response.{}.{}",
            self.request_timestamp, status_code, file_ext
        );

        self.runtime_history_path.join(filename)
    }
}
