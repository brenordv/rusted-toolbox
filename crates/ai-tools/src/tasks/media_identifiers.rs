use crate::ai_functions::media_sorter_functions::{
    extract_movie_title_from_filename_as_string, extract_season_episode_from_filename_as_string,
    extract_tv_show_title_from_filename_as_string, identify_media_format_from_filename_as_string,
    identify_media_type_from_filename_as_string, is_main_archive_file_as_string,
};
use crate::message_builders::system_message_builders::build_rust_ai_function_user_message;
use crate::models::file_process_item_traits::FileProcessItemTraits;
use crate::models::models::FileProcessResult::{IdentifiedFailed, Ignored};
use crate::models::models::MediaType::{Movie, TvShow};
use crate::models::models::{IdentificationResult, TvShowSeasonEpisodeInfo};
use crate::requesters::requester_implementations::OpenAiRequester;
use crate::requesters::requester_traits::OpenAiRequesterTraits;
use crate::utils::control_file_wrapper::ControlFileWrapper;
use shared::system::pathbuf_extensions::PathBufExtensions;
use std::sync::Arc;
use tracing::info;

/// Identifies basic media data from a file using AI assistance.
///
/// This function uses an AI model to identify whether a given file corresponds
/// to a movie or a TV show based on its filename. Depending on the identified
/// media type, it further extracts and updates metadata such as the title, and
/// for TV shows, it also extracts season and episode information.
///
/// # Arguments
///
/// * `control` - A thread-safe wrapper around the ControlFile instance, which
///   contains file metadata and provides methods to update it.
/// * `ai_requester` - A mutable reference to an instance of `OpenAiRequester`,
///   responsible for sending requests to the AI model.
///
/// # Returns
///
/// Returns `Ok(())` if the media identification and metadata extraction
/// processes succeed. Otherwise, it returns an `anyhow::Error` describing what
/// went wrong.
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

/// Asynchronously identifies a file's type and processes it based on its characteristics.
///
/// This function performs the following operations:
/// - Updates the file status to "Identifying".
/// - Examines the file to determine its type (e.g., image or compressed).
/// - Applies appropriate handling based on the file type:
///   - If the file is an image, it logs that no further analysis is needed and updates the status to "Ignored".
///   - If the file is compressed, it validates whether the file is the primary archive file:
///     - If not the main archive file, logs the information, updates its properties, marks it as "Ignored", and stops further processing.
///     - If it is the main archive file, logs the information, marks it as such, updates the status to "Ignored", and stops further processing.
///   - If the file is neither an image nor compressed, further analysis is conducted using AI to identify the media's basic properties.
///
/// # Remarks
/// The difference between this function and `identify_file_only_with_ai` is that this one identifies if the file is an archive,
/// and if it is the main file in the archive using simple local methods. This greatly reduces processing time and costs when dealing
/// with multipart compressed files.
///
/// # Arguments
///
/// * `control` - An `Arc` wrapped instance of `ControlFileWrapper` that provides methods to access and update the file's metadata.
/// * `ai_requester` - A mutable reference to an `OpenAiRequester`, which is used for AI-based analysis of the file.
///
/// # Returns
///
/// Returns an `anyhow::Result<()>`, where:
/// - `Ok(())` indicates successful completion of the file identification and processing.
/// - `Err` contains an error in case of failure during file analysis, updates, or AI processing.
pub async fn identify_file_hybrid(
    control: Arc<ControlFileWrapper>,
    ai_requester: &mut OpenAiRequester,
) -> anyhow::Result<IdentificationResult> {
    let file = control.get_file();

    if file.is_image() {
        info!("File is an image, no need to analyze it further...");
        return Ok(IdentificationResult::Ignored);
    }

    if file.is_compressed() {
        info!("File is compressed, checking if it is the main archive file...");
        control.update_is_archive(true)?;

        if !file.is_main_file_multi_part_compression() {
            info!("File is not the main archive file, no point in keeping analyzing it.");
            control.update_is_main_archive_file(false)?;
            return Ok(IdentificationResult::Ignored);
        }

        info!("File is the main archive file, proceeding with analysis...");
        control.update_is_main_archive_file(true)?;
        return Ok(IdentificationResult::Ignored);
    }

    control.update_is_archive(false)?;

    identify_media_basic_data_using_ai(control.clone(), ai_requester).await?;

    Ok(IdentificationResult::Success)
}

/// Asynchronously identifies a file's type and processes it based on its characteristics.
///
/// This function performs the following operations:
/// - Updates the file status to "Identifying".
/// - Examines the file to determine its type (e.g., image or compressed).
/// - Applies appropriate handling based on the file type:
///   - If the file is an image, it logs that no further analysis is needed and updates the status to "Ignored".
///   - If the file is compressed, it validates whether the file is the primary archive file:
///     - If not the main archive file, logs the information, updates its properties, marks it as "Ignored", and stops further processing.
///     - If it is the main archive file, logs the information, marks it as such, updates the status to "Ignored", and stops further processing.
///   - If the file is neither an image nor compressed, further analysis is conducted using AI to identify the media's basic properties.
///
/// # Remarks
/// The difference between this function and `identify_file_hybrid` is that this one identifies if the file is an archive,
/// and if it is the main file in the archive using AI. This makes things slower and a bit more expensive, but it is way cooler.
///
/// # Arguments
///
/// * `control` - An `Arc` wrapped instance of `ControlFileWrapper` that provides methods to access and update the file's metadata.
/// * `ai_requester` - A mutable reference to an `OpenAiRequester`, which is used for AI-based analysis of the file.
///
/// # Returns
///
/// Returns an `anyhow::Result<()>`, where:
/// - `Ok(())` indicates successful completion of the file identification and processing.
/// - `Err` contains an error in case of failure during file analysis, updates, or AI processing.
pub async fn identify_file_only_with_ai(
    control: Arc<ControlFileWrapper>,
    ai_requester: &mut OpenAiRequester,
) -> anyhow::Result<()> {
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

    Ok(())
}
