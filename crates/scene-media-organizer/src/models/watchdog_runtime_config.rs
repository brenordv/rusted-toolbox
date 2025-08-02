use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct WatchdogRuntimeConfig {
    pub watch_folder: PathBuf,
    pub target_base_movie_folder: PathBuf,
    pub target_base_series_folder: PathBuf,
    pub db_data_file: String,
    pub guess_it_api_base_url: String,
    pub unrar_bin_path: String,
    watch_folder_string: String,
    target_base_movie_folder_string: String,
    target_base_series_folder_string: String,
}

impl WatchdogRuntimeConfig {
    pub fn new(
        watch_folder: PathBuf,
        target_base_movie_folder: PathBuf,
        target_base_series_folder: PathBuf,
        db_data_file: String,
        guess_it_api_base_url: String,
        unrar_bin_path: String,
    ) -> Self {
        let watch_folder_string = watch_folder.to_str().unwrap().to_string();
        let target_base_movie_folder_string =
            target_base_movie_folder.to_str().unwrap().to_string();
        let target_base_series_folder_string =
            target_base_series_folder.to_str().unwrap().to_string();

        Self {
            watch_folder,
            target_base_movie_folder,
            target_base_series_folder,
            watch_folder_string,
            target_base_movie_folder_string,
            target_base_series_folder_string,
            db_data_file,
            guess_it_api_base_url,
            unrar_bin_path,
        }
    }

    pub fn get_watch_folder(&self) -> String {
        self.watch_folder_string.clone()
    }

    pub fn get_target_base_movie_folder(&self) -> String {
        self.target_base_movie_folder_string.clone()
    }

    pub fn get_target_base_series_folder(&self) -> String {
        self.target_base_series_folder_string.clone()
    }
}
