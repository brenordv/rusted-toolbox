use std::path::PathBuf;

pub struct WatchdogRuntimeConfig {
    pub watch_folder: PathBuf,
    pub target_base_movie_folder: PathBuf,
    pub target_base_series_folder: PathBuf,
    watch_folder_string: String,
    target_base_movie_folder_string: String,
    target_base_series_folder_string: String,
}

impl WatchdogRuntimeConfig {
    pub fn new(
        watch_folder: PathBuf,
        target_base_movie_folder: PathBuf,
        target_base_series_folder: PathBuf,
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
