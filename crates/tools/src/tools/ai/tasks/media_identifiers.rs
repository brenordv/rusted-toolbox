use std::sync::Arc;
use tracing::{debug, info};
use crate::shared::system::pathbuf_extensions::PathBufExtensions;
use crate::tools::ai::ai_functions::media_sorter_functions::{extract_movie_title_from_filename_as_string, extract_season_episode_from_filename_as_string, extract_tv_show_title_from_filename_as_string, identify_media_format_from_filename_as_string, identify_media_type_from_filename_as_string, is_main_archive_file_as_string};
use crate::tools::ai::message_builders::system_message_builders::build_rust_ai_function_user_message;
use crate::tools::ai::models::file_process_item_traits::FileProcessItemTraits;
use crate::tools::ai::models::models::FileProcessResult::{IdentifiedFailed, IdentifiedOk, Identifying, Ignored};
use crate::tools::ai::models::models::MediaType::{Movie, TvShow};
use crate::tools::ai::models::models::TvShowSeasonEpisodeInfo;
use crate::tools::ai::requesters::requester_implementations::OpenAiRequester;
use crate::tools::ai::requesters::requester_traits::OpenAiRequesterTraits;
use crate::tools::ai::utils::control_file_wrapper::ControlFileWrapper;

async fn identify_media_basic_data_using_ai(
    control: Arc<ControlFileWrapper>,
    ai_requester: &mut OpenAiRequester,
) -> anyhow::Result<()> {
    let file_name = control.get_file_path();

    info!("Guessing media type...");

    let response = ai_requester
        .send_request(
            build_rust_ai_function_user_message(
                identify_media_type_from_filename_as_string,
                file_name.as_str(),
            ),
            false,
        )
        .await?;

    let ai_media_type = response.message.as_str();

    info!("Is file from a Movie or TV Show?: {:?}", ai_media_type);

    if ai_media_type == "movie" {
        control.update_media_type(Movie)?;

        let response = ai_requester
            .send_request(
                build_rust_ai_function_user_message(
                    extract_movie_title_from_filename_as_string,
                    file_name.as_str(),
                ),
                false,
            )
            .await?;

        let movie_title = response.message;

        info!("Movie title: {:?}", movie_title);

        control.update_title(movie_title)?;
    } else if ai_media_type == "tvshow" {
        control.update_media_type(TvShow)?;

        info!("Extracting title of the TV Show...");

        let response = ai_requester
            .send_request(
                build_rust_ai_function_user_message(
                    extract_tv_show_title_from_filename_as_string,
                    file_name.as_str(),
                ),
                false,
            )
            .await?;

        let tv_show_title = response.message;

        info!("TV Show name?: {:?}", tv_show_title);

        control.update_title(tv_show_title)?;

        info!("Extracting season and episode numbers of the TV Show...");

        let response = ai_requester
            .send_request(
                build_rust_ai_function_user_message(
                    extract_season_episode_from_filename_as_string,
                    file_name.as_str(),
                ),
                false,
            )
            .await?;

        info!("Season and Episode numbers?: {:?}", response.message);

        let season_episode_info = TvShowSeasonEpisodeInfo::new(response.message)?;

        control.update_season_episode_info(season_episode_info)?;
    } else {
        control.update_status(IdentifiedFailed)?;

        anyhow::bail!("Failed to identify media type for file: {}", file_name);
    };

    Ok(())
}

pub async fn identify_file_hybrid(
    control: Arc<ControlFileWrapper>,
    ai_requester: &mut OpenAiRequester,
) -> anyhow::Result<()> {
    control.update_status(Identifying)?;

    let file = control.get_file();

    if file.is_image() {
        info!("File is an image, no need to analyze it further...");
        control.update_status(Ignored)?;
        return Ok(());
    }
    
    if file.is_compressed() {
        info!("File is compressed, checking if it is the main archive file...");
        control.update_is_archive(true)?;
        
        if !file.is_main_file_multi_part_compression() {
            info!("File is not the main archive file, no point in keeping analyzing it.");
            control.update_is_main_archive_file(false)?;
            control.update_status(Ignored)?;
            return Ok(());
        }

        info!("File is the main archive file, proceeding with analysis...");
        control.update_is_main_archive_file(true)?;
        control.update_status(Ignored)?;
        return Ok(());
    }

    control.update_is_archive(false)?;

    identify_media_basic_data_using_ai(control.clone(), ai_requester).await?;

    control.update_status(IdentifiedOk)?;

    Ok(())
}

pub async fn identify_file_only_with_ai(
    control: Arc<ControlFileWrapper>,
    ai_requester: &mut OpenAiRequester,
) -> anyhow::Result<()> {
    control.update_status(Identifying)?;

    let file = control.get_file();

    if file.is_image() {
        info!("File is an image, no need to analyze it further...");
        control.update_status(Ignored)?;
        return Ok(());
    }

    let file_name = control.get_file_path();

    info!("Checking if file is an archive...");
    let response = ai_requester
        .send_request(
            build_rust_ai_function_user_message(
                identify_media_format_from_filename_as_string,
                file_name.as_str(),
            ),
            false,
        )
        .await?;

    let file_type = response.message.as_str();
    info!("Is file compressed or decompressed?: {:?}", file_type);

    if file_type == "compressed" {
        control.update_is_archive(true)?;

        info!("Identifying if it is the main archive file...");

        let request =
            build_rust_ai_function_user_message(is_main_archive_file_as_string, file_name.as_str());

        let response = ai_requester.send_request(request, false).await?;

        let is_main_file = response.message.as_str();

        info!("Is file the main archive file?: {:?}", is_main_file);

        match is_main_file.parse::<bool>() {
            Ok(b) => {
                control.update_is_main_archive_file(b)?;
                if !b {
                    // If the file is compressed, and it's not the main one, no point in
                    // keeping analyzing it.
                    control.update_status(Ignored)?;
                    return Ok(());
                }
            }
            Err(_) => {
                anyhow::bail!(
                    "Failed to identify if file is the main archive file: {}",
                    file_name
                );
            }
        };
    } else if file_type == "decompressed" {
        control.update_is_archive(false)?;
    }

    identify_media_basic_data_using_ai(control.clone(), ai_requester).await?;

    control.update_status(IdentifiedOk)?;

    Ok(())
}