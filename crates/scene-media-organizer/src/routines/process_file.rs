use crate::utils::file_status_controller::FileStatusController;
use crate::utils::guessit_client::GuessItClient;
use anyhow::Result;
use notify::Event;
use shared::system::monitor_folder::EventType;
use std::path::PathBuf;
use tracing_log::log::debug;

pub struct ProcessFileRoutine {
    file_status_controller: FileStatusController,
    guess: GuessItClient,
}

impl ProcessFileRoutine {
    pub fn new(db_path: String, guess_it_base_url: String) -> Result<Self> {
        Ok(Self {
            file_status_controller: FileStatusController::new(db_path)?,
            guess: GuessItClient::new(guess_it_base_url),
        })
    }

    pub async fn handle_on_file_created(
        &self,
        event: Event,
        event_type: EventType,
        _: PathBuf,
    ) -> Result<()> {
        let created_entries = &event.paths;
        debug!(
            "[{:?}] On Created Event (count: {}): [{:?}]",
            event_type,
            created_entries.len(),
            event
        );

        for entry in created_entries {
            debug!("Processing file: {:?}", entry);

            let response = self.guess.it(entry.to_str().unwrap().to_string()).await?;

            debug!("Response: {:?}", response);
        }

        Ok(())
    }
}
