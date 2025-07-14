#![allow(dead_code, unused_imports)] // I'll remove this later.
use anyhow::Context;
use dotenv::dotenv;
use rusted_toolbox::tools::ai::ai_agent_media_sorter_app::handle_file_created;
use rusted_toolbox::tools::ai::utils::monitor_folder::monitor_folder_for_new_files;
use std::env;
use rusted_toolbox::tools::ai::ai_functions::media_sorter_functions::{extract_episode_title_from_filename_as_string, extract_movie_title_from_filename_as_string, extract_season_episode_from_filename_as_string, extract_tv_show_title_from_filename_as_string, identify_media_format_from_filename_as_string, identify_media_type_from_filename, identify_media_type_from_filename_as_string, is_main_archive_file_as_string};
use rusted_toolbox::tools::ai::message_builders::system_message_builders::{build_rust_ai_function_system_message, build_rust_ai_function_user_message};
use rusted_toolbox::tools::ai::requesters::requester_builders::{build_requester_for_open_router, build_requester_for_openai};
use rusted_toolbox::tools::ai::requesters::requester_traits::OpenAiRequesterTraits;

fn get_test_filenames() -> Vec<String> {
    vec![
        "Brooklyn.Nine-Nine.S06E18.720p.WEBRip.x264-MIXED.mkv".to_string(),
        "Classic.Movie.Collection.part03.rar".to_string(),
        "Jurassic.Park.Rebirth.2025.1080p.BluRay.x132.YIFY.7z".to_string(),
        "No.Country.for.Old.Men.2007.1080p.BluRay.DTS.x264-Group.mkv".to_string(),
        "Black.Mirror.S05E03.1080p.WEB.H264-STRiFE.mkv".to_string(),
        "House.S04E12.avi".to_string(),
        "Fargo.S02E09.The.Castle.720p.HDTV.x264-KILLERS.mkv".to_string(),
        // "The.Matrix.1999.1080p.BluRay.x264.DTS-FGT.mkv".to_string(),
        // "Rick.and.Morty.S05E01.Mort.Dinner.Rick_Andre.720p.WEBRip.x264-ION10.mkv".to_string(),
        // "random-text-file.txt".to_string(),
        // "Better.Call.Saul.S01E07.720p.WEB-DL.x264-GROUP.part02.rar".to_string(),
        // "Inception.2010.1080p.BluRay.x264.YIFY.7z".to_string(),
        // "Breaking.Bad.S05E14.720p.HDTV.x264-IMMERSE.mkv".to_string(),
        // "Inception.2010.720p.BluRay.x264.YIFY.mp4".to_string(),
        // "Game.of.Thrones.S08E03.1080p.WEB.H264-MEMENTO.mkv".to_string(),
        // "Parasite.2019.KOREAN.1080p.BluRay.x264.DTS-FGT.mkv".to_string(),
        // "Stranger.Things.S04E01.Chapter.One.720p.NF.WEB-DL.DDP5.1.x264-NTb.mkv".to_string(),
        // "1917.2019.2160p.UHD.BluRay.X265-IAMABLE.mkv".to_string(),
        // "Friends.2x11.480p.DVD.x264-SAiNTS.mkv".to_string(),
        // "Spider-Man.Into.the.Spider-Verse.2018.1080p.BluRay.x264.YIFY.mp4".to_string(),
        // "The.Office.US.S07E17.720p.NF.WEB-DL.DDP5.1.x264-NTb.mkv".to_string(),
        // "Joker.2019.720p.BluRay.x264.YIFY.mp4".to_string(),
        // "The.Witcher.S01E01.720p.WEBRip.x264-GalaxyTV.mkv".to_string(),
        // "Gladiator.2000.1080p.BluRay.x264.YIFY.mp4".to_string(),
        // "Better.Call.Saul.S03E05.1080p.AMZN.WEBRip.DDP5.1.x264-NTb.mkv".to_string(),
        // "Mad.Max.Fury.Road.2015.720p.BluRay.x264.YIFY.mp4".to_string(),
        // "Lost.S01E01.Pilot.1080p.BluRay.x264-ROVERS.mkv".to_string(),
        // "Interstellar.2014.1080p.BluRay.x264.YIFY.mp4".to_string(),
        // "Chernobyl.2019.S01E03.720p.WEB-DL.x264-MEMENTO.mkv".to_string(),
        // "Jaws.1975.720p.BluRay.x264.YIFY.mp4".to_string(),
        // "The.Expanse.S03E08.720p.WEB-DL.x264-MEMENTO.mkv".to_string(),
        // "La.La.Land.2016.1080p.BluRay.x264.YIFY.mp4".to_string(),
        // "True.Detective.S02E01.720p.HDTV.x264-KILLERS.mkv".to_string(),
        // "Forrest.Gump.1994.720p.BluRay.x264.YIFY.mp4".to_string(),
        // "The.Mandalorian.S01E02.720p.WEBRip.x264-GalaxyTV.mkv".to_string(),
        // "Show.Name.2022.2x07.720p.WEB-DL.mkv".to_string(),
        // "Movie.2020.1080p.WEB-DL.H264-RARBG.avi".to_string(),
        // "The.Wire.03x11.avi".to_string(),
        // "Movie.Title.2019.m2ts".to_string(),
        // "Sherlock.S02.E03.1080p.BluRay.x264-SHORTCUT.mkv".to_string(),
        // "Avatar.2.2022.2160p.UHD.BluRay.x265.mkv".to_string(),
        // "Show.Name.S03E05-E06.720p.HDTV.x264-GROUP.mkv".to_string(),
        // "Dune.Part.One.2021.1080p.BluRay.x264-GROUP.mkv".to_string(),
        // "Rick.and.Morty.S05E01E02.720p.WEBRip.x264-ION10.mkv".to_string(),
        // "Edge.of.Tomorrow.2014.720p.BluRay.x264.YIFY.mkv".to_string(),
        // "The.Walking.Dead.1001.1080p.WEB.H264-STRiFE.mkv".to_string(),
        // "Show.Name.-.S04E10.-.The.Finale.mkv".to_string(),
        // "King.Kong.1933.REMASTERED.720p.BluRay.x264-GROUP.mkv".to_string(),
        // "ShowName_S06_E12_HDTV.mp4".to_string(),
        // "Blade.Runner.2049.2017.1080p.BluRay.x264-GROUP.mkv".to_string(),
        // "Movie.2001.Space.Odyssey.1968.720p.BluRay.x264.YIFY.mp4".to_string(),
        // "Seinfeld.821.720p.HDTV.x264-GROUP.mkv".to_string(),
        // "Alien3.1992.720p.BluRay.x264.YIFY.mp4".to_string(),
        // "Battlestar.Galactica.2004.S01E01.33.720p.BluRay.x264.mkv".to_string(),
        // "Up.2009.1080p.BluRay.x264.YIFY.mp4".to_string(),
        // "The.Matrix.1999.1080p.BluRay.x264.DTS-FGT.rar".to_string(),
        // "Breaking.Bad.S02E10.720p.HDTV.x264-IMMERSE.r00".to_string(),
        // "Inception.2010.1080p.BluRay.x264.YIFY.7z".to_string(),
        // "The.Office.US.S01E05.720p.WEB-DL.x264.part1.rar".to_string(),
        // "Joker.2019.BluRay.x264.YIFY.zip".to_string(),
        // "Stranger.Things.S03E01.001".to_string(),
        // "True.Detective.S02E03.720p.HDTV.x264-KILLERS.r001".to_string(),
        // "Avatar.2009.DISC2.ISO".to_string(),
        // "Lost.S04E02.PT-BR.1080p.WEB-DL.mkv".to_string(),
        // "Forrest.Gump.1994.DISC1.1080p.BluRay.iso".to_string(),
        // "Se7en.1995.avi".to_string(),
        // "24.S01E01.avi".to_string(),
        // "13.Reasons.Why.S02E01.mkv".to_string(),
        // "2012.2009.BluRay.avi".to_string(),
        // "John.Wick.Chapter.3.Parabellum.2019.720p.BluRay.mkv".to_string(),
        // "ER.101.avi".to_string(),
        // "The.Witcher.S01E01.mkv".to_string(),
        // "Fight.Club.1999.mkv".to_string(),
        // "10.Things.I.Hate.About.You.1999.mkv".to_string(),
        // "CSI.204.avi".to_string(),
        // "District.9.2009.BluRay.x264.YIFY.mp4".to_string(),
        // "Room.104.2017.1080p.WEBRip.x264-STRiFE.mkv".to_string(),
        // "The.Number.23.2007.DVDRip.x264.avi".to_string(),
        // "Friday.the.13th.2009.BluRay.avi".to_string(),
        // "readme.md".to_string(),
    ]
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();

    println!("Rusted Agents: Media sorter");

    // let folder_to_watch = env::var("AI_MEDIA_SORTER_WATCH_FOLDER")
    //     .context("AI_MEDIA_SORTER_WATCH_FOLDER is not set")?;
    //
    // monitor_folder_for_new_files(folder_to_watch.as_str(), Some(handle_file_created))?;

    let mut requester = build_requester_for_openai()?;

    requester
        .set_system_message(build_rust_ai_function_system_message())?
        .set_temperature(&0.0)?;

    for test_filename in get_test_filenames() {
        println!("-------------------------------------------------------------------------");
        println!("Processing file: {}", test_filename);

        let request = build_rust_ai_function_user_message(
            identify_media_type_from_filename_as_string,
            test_filename.as_str(),
        );

        let response = requester.send_request(request, false).await?;

        println!("Is file from a Movie or TV Show?: {:?}", response.message);

        /*----------------------------------------------------------------------------*/

        if response.message == "movie" {
            let request = build_rust_ai_function_user_message(
                extract_movie_title_from_filename_as_string,
                test_filename.as_str(),
            );

            let response = requester.send_request(request, false).await?;

            println!("Movie name?: {:?}", response.message);

        } else if response.message == "tvshow" {
            let request = build_rust_ai_function_user_message(
                extract_tv_show_title_from_filename_as_string,
                test_filename.as_str(),
            );

            let response = requester.send_request(request, false).await?;

            println!("TV Show name?: {:?}", response.message);

            /*----------------------------------------------------------------------------*/

            let request = build_rust_ai_function_user_message(
                extract_season_episode_from_filename_as_string,
                test_filename.as_str(),
            );

            let response = requester.send_request(request, false).await?;

            println!("Season and Episode numbers?: {:?}", response.message);

            /*----------------------------------------------------------------------------*/

            let request = build_rust_ai_function_user_message(
                extract_episode_title_from_filename_as_string,
                test_filename.as_str(),
            );

            let response = requester.send_request(request, false).await?;

            println!("Episode Title (if available)?: {:?}", response.message);
        }

        /*----------------------------------------------------------------------------*/

        let request = build_rust_ai_function_user_message(
            identify_media_format_from_filename_as_string,
            test_filename.as_str(),
        );

        let response = requester.send_request(request, false).await?;

        println!("Is file compressed or decompressed?: {:?}", response.message);

        /*----------------------------------------------------------------------------*/

        if response.message == "compressed" {
            let request = build_rust_ai_function_user_message(
                is_main_archive_file_as_string,
                test_filename.as_str(),
            );

            let response = requester.send_request(request, false).await?;

            println!("Is main file in the archive?: {:?}", response.message);
        }
    }

    Ok(())
}