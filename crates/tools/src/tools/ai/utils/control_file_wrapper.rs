use crate::shared::sqlite::dictionary_db::DictionaryDb;
use crate::tools::ai::models::file_process_item_model::FileProcessItem;
use crate::tools::ai::models::file_process_item_traits::FileProcessItemTraits;
use crate::tools::ai::models::models::{FileProcessResult, MediaType, TvShowSeasonEpisodeInfo};
use anyhow::Result;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use log::{debug, info};

pub struct ControlFileWrapper {
    db: Arc<DictionaryDb>,
    key: Arc<str>,
    item: Mutex<FileProcessItem>,
}

impl ControlFileWrapper {
    pub fn new(db: Arc<DictionaryDb>, item: FileProcessItem) -> Result<Self> {
        let ref_file = &item.file_path.clone();

        Ok(ControlFileWrapper {
            db,
            key: Arc::from(ref_file.as_str()),
            item: Mutex::new(item),
        })
    }

    fn save(&self) -> Result<()> {
        info!("Saving changes to control file: {:?}", self.key);
        self.db.update(self.key.as_ref(), &self.item)?;
        
        info!("Changes saved to control file: {:?}", self.key);
        Ok(())
    }

    pub fn get_current_attempts(&self) -> usize {
        let item = self.item.lock().unwrap();
        item.attempt.clone()
    }

    pub fn get_current_status(&self) -> FileProcessResult {
        let item = self.item.lock().unwrap();
        item.status.clone()
    }

    pub fn get_is_archived(&self) -> bool {
        let item = self.item.lock().unwrap();
        item.is_archive.unwrap_or(false)
    }

    pub fn get_is_main_archive_file(&self) -> bool {
        let item = self.item.lock().unwrap();
        item.is_main_archive_file.unwrap_or(false)
    }

    pub fn get_file(&self) -> PathBuf {
        let item = self.item.lock().unwrap();
        item.file.clone()
    }

    pub fn get_media_type(&self) -> MediaType {
        let item = self.item.lock().unwrap();
        item.media_type.clone().unwrap_or(MediaType::Unknown)
    }

    pub fn get_title(&self) -> Option<String> {
        let item = self.item.lock().unwrap();
        item.title.clone()
    }

    pub fn get_episode_info(&self) -> Option<TvShowSeasonEpisodeInfo> {
        let item = self.item.lock().unwrap();
        item.season_episode_info.clone()
    }
}

impl FileProcessItemTraits for ControlFileWrapper {
    fn update_status(&self, status: FileProcessResult) -> Result<()> {
        {
            debug!("Getting access to control file to update status to: {:?}", status);
            let mut item = self.item.lock().unwrap();
            debug!("Got access to control file to update status to: {:?}", status);
            item.status = status;
            debug!("Status changed in the control file object...");
        }

        debug!("Saving changes to control file: {:?}", self.key);
        self.save()
    }

    fn update_attempt(&self) -> Result<()> {
        {
            let mut item = self.item.lock().unwrap();
            item.attempt += 1;
        }
        self.save()
    }

    fn update_media_type(&self, media_type: MediaType) -> Result<()> {
        {
            let mut item = self.item.lock().unwrap();
            item.media_type = Some(media_type);
        }
        self.save()
    }

    fn update_title(&self, title: String) -> Result<()> {
        {
            let mut item = self.item.lock().unwrap();
            item.title = Some(title);
        }
        self.save()
    }

    fn update_is_archived(&self, is_archived: bool) -> Result<()> {
        {
            let mut item = self.item.lock().unwrap();
            item.is_archive = Some(is_archived);
        }
        self.save()
    }

    fn update_is_main_archive_file(&self, is_main_archive_file: bool) -> Result<()> {
        {
            let mut item = self.item.lock().unwrap();
            item.is_main_archive_file = Some(is_main_archive_file);
        }
        self.save()
    }

    fn update_season_episode_info(
        &self,
        season_episode_info: TvShowSeasonEpisodeInfo,
    ) -> Result<()> {
        {
            let mut item = self.item.lock().unwrap();
            item.season_episode_info = Some(season_episode_info);
        }
        self.save()
    }

    fn update_target_path(&self, target_path: PathBuf) -> Result<()> {
        {
            let mut item = self.item.lock().unwrap();
            item.target_path = Some(target_path);
        }
        self.save()
    }
}