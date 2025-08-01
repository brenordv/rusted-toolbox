use crate::models::guessit_response::GuessItResponse;

pub struct GuessItClient {
    base_url: String,
}

impl GuessItClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
        }
    }
    
    pub async fn it(filename: String) -> Result<GuessItResponse> {
        
    }
}