use crate::tools::ai::requesters::requester_implementations::OpenAiRequester;
use crate::tools::ai::requesters::requester_traits::OpenAiRequesterTraits;
use anyhow::{Context, Result};
use std::env;

pub fn build_requester_for_openai() -> Result<OpenAiRequester> {
    let request_history_path = match env::var("OPEN_AI_CHAT_REQUEST_HISTORY_PATH") {
        Ok(path) => Some(path),
        Err(_) => None,
    };

    let api_key = env::var("OPEN_AI_API_KEY").context("OPEN_AI_API_KEY must be set")?;

    let ai_model = env::var("OPEN_AI_MODEL").context("OPEN_AI_MODEL must be set")?;

    let api_url = env::var("OPEN_AI_API_URL").context("OPEN_AI_API_URL must be set")?;

    let temperature = match env::var("OPEN_AI_TEMPERATURE") {
        Ok(temperature) => Some(
            temperature
                .parse::<f32>()
                .context("OPEN_AI_TEMPERATURE must be a float")?,
        ),
        Err(_) => None,
    };

    let ai_organization =
        env::var("OPEN_AI_ORGANIZATION").context("OPEN_AI_ORGANIZATION must be set")?;

    let mut requester = OpenAiRequester::new(
        api_url,
        api_key,
        Some(ai_organization),
        temperature,
        request_history_path,
    )?;

    requester
        .set_model(ai_model.as_str())?
        .initialize_api_client()?;

    Ok(requester)
}

pub fn build_requester_for_open_router() -> Result<OpenAiRequester> {
    let request_history_path = match env::var("OPEN_ROUTER_CHAT_REQUEST_HISTORY_PATH") {
        Ok(path) => Some(path),
        Err(_) => None,
    };

    let api_key = env::var("OPEN_ROUTER_API_KEY").context("OPEN_ROUTER_API_KEY must be set")?;

    let ai_model = env::var("OPEN_ROUTER_MODEL").context("OPEN_ROUTER_MODEL must be set")?;

    let api_url = env::var("OPEN_ROUTER_API_URL").context("OPEN_ROUTER_API_URL must be set")?;

    let temperature = match env::var("OPEN_ROUTER_TEMPERATURE") {
        Ok(temperature) => Some(
            temperature
                .parse::<f32>()
                .context("OPEN_ROUTER_TEMPERATURE must be a float")?,
        ),
        Err(_) => None,
    };

    let mut requester =
        OpenAiRequester::new(api_url, api_key, None, temperature, request_history_path)?;

    requester
        .set_model(ai_model.as_str())?
        .initialize_api_client()?;

    Ok(requester)
}
