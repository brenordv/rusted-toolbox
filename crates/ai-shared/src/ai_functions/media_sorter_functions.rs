#![allow(dead_code, unused_doc_comments)]
use ai_macros::ai_function;

/*
Not in use, but keeping it to serve as a reference for future use.
*/

const OUTPUT: &str = "";

#[ai_function]
pub fn identify_media_type_from_filename(_input_filename: &str) -> &str {
    /// Input: Takes in the filename the user wants to be analyzed.
    /// Function: Analyzes the input filename and returns if it is a movie or tv-show.
    /// Important:
    /// - Work to the best of your knowledge and use filename conventions to make an informed decision.
    /// - Only if you cannot reasonably determine whether the filename represents a movie or a TV show episode, return "unknown".
    /// - "unknown" must be your last resort—try to classify as "movie" or "tvshow" whenever possible.
    /// - The output must be exactly one of: "movie", "tvshow", or "unknown". No explanation or context. No other value.
    /// - Output must be a single token: "movie", "tvshow", or "unknown". No leading or trailing spaces or newlines.
    /// - Ignore the file extension and letter case when analyzing the filename.
    /// Output:
    /// - If the filename string contains the name of a Movie, return "movie";
    /// - If the filename string contains an indication for a TV Show episode, return "tvshow".
    /// - If you cannot reasonably classify, return "unknown" as a last resort.
    /// Examples:
    /// - "The.Matrix.1999.1080p.BluRay.x264.DTS-FGT.mkv" -> movie
    /// - "Breaking.Bad.S05E14.720p.HDTV.x264-IMMERSE.mkv" -> tvshow
    /// - "Inception.2010.720p.BluRay.x264.YIFY.mp4" -> movie
    /// - "Game.of.Thrones.S08E03.1080p.WEB.H264-MEMENTO.mkv" -> tvshow
    /// - "Parasite.2019.KOREAN.1080p.BluRay.x264.DTS-FGT.mkv" -> movie
    /// - "Stranger.Things.S04E01.Chapter.One.720p.NF.WEB-DL.DDP5.1.x264-NTb.mkv" -> tvshow
    /// - "1917.2019.2160p.UHD.BluRay.X265-IAMABLE.mkv" -> movie
    /// - "Friends.2x11.480p.DVD.x264-SAiNTS.mkv" -> tvshow
    /// - "Spider-Man.Into.the.Spider-Verse.2018.1080p.BluRay.x264.YIFY.mp4" -> movie
    /// - "The.Office.US.S07E17.720p.NF.WEB-DL.DDP5.1.x264-NTb.mkv" -> tvshow
    /// - "Joker.2019.720p.BluRay.x264.YIFY.mp4" -> movie
    /// - "The.Witcher.S01E01.720p.WEBRip.x264-GalaxyTV.mkv" -> tvshow
    /// - "Gladiator.2000.1080p.BluRay.x264.YIFY.mp4" -> movie
    /// - "Better.Call.Saul.S03E05.1080p.AMZN.WEBRip.DDP5.1.x264-NTb.mkv" -> tvshow
    /// - "Mad.Max.Fury.Road.2015.720p.BluRay.x264.YIFY.mp4" -> movie
    /// - "Lost.S01E01.Pilot.1080p.BluRay.x264-ROVERS.mkv" -> tvshow
    /// - "Interstellar.2014.1080p.BluRay.x264.YIFY.mp4" -> movie
    /// - "Chernobyl.2019.S01E03.720p.WEB-DL.x264-MEMENTO.mkv" -> tvshow
    /// - "Jaws.1975.720p.BluRay.x264.YIFY.mp4" -> movie
    /// - "The.Expanse.S03E08.720p.WEB-DL.x264-MEMENTO.mkv" -> tvshow
    /// - "La.La.Land.2016.1080p.BluRay.x264.YIFY.mp4" -> movie
    /// - "True.Detective.S02E01.720p.HDTV.x264-KILLERS.mkv" -> tvshow
    /// - "Forrest.Gump.1994.720p.BluRay.x264.YIFY.mp4" -> movie
    /// - "The.Mandalorian.S01E02.720p.WEBRip.x264-GalaxyTV.mkv" -> tvshow
    /// - "Show.Name.2022.2x07.720p.WEB-DL.mkv" -> tvshow
    /// - "Movie.2020.1080p.WEB-DL.H264-RARBG.avi" -> movie
    /// - "The.Wire.03x11.avi" -> tvshow
    /// - "Movie.Title.2019.m2ts" -> movie
    /// - "Sherlock.S02.E03.1080p.BluRay.x264-SHORTCUT.mkv" -> tvshow
    /// - "Avatar.2.2022.2160p.UHD.BluRay.x265.mkv" -> movie
    /// - "Show.Name.S03E05-E06.720p.HDTV.x264-GROUP.mkv" -> tvshow
    /// - "Dune.Part.One.2021.1080p.BluRay.x264-GROUP.mkv" -> movie
    /// - "Rick.and.Morty.S05E01E02.720p.WEBRip.x264-ION10.mkv" -> tvshow
    /// - "Edge.of.Tomorrow.2014.720p.BluRay.x264.YIFY.mkv" -> movie
    /// - "The.Walking.Dead.1001.1080p.WEB.H264-STRiFE.mkv" -> tvshow
    /// - "Show.Name.-.S04E10.-.The.Finale.mkv" -> tvshow
    /// - "King.Kong.1933.REMASTERED.720p.BluRay.x264-GROUP.mkv" -> movie
    /// - "ShowName_S06_E12_HDTV.mp4" -> tvshow
    /// - "Blade.Runner.2049.2017.1080p.BluRay.x264-GROUP.mkv" -> movie
    /// - "Movie.2001.Space.Odyssey.1968.720p.BluRay.x264.YIFY.mp4" -> movie
    /// - "Seinfeld.821.720p.HDTV.x264-GROUP.mkv" -> tvshow
    /// - "Alien3.1992.720p.BluRay.x264.YIFY.mp4" -> movie
    /// - "Battlestar.Galactica.2004.S01E01.33.720p.BluRay.x264.mkv" -> tvshow
    /// - "Up.2009.1080p.BluRay.x264.YIFY.mp4" -> movie
    /// - "The.Matrix.1999.1080p.BluRay.x264.DTS-FGT.rar" -> movie
    /// - "Breaking.Bad.S02E10.720p.HDTV.x264-IMMERSE.r00" -> tvshow
    /// - "Inception.2010.1080p.BluRay.x264.YIFY.7z" -> movie
    /// - "The.Office.US.S01E05.720p.WEB-DL.x264.part1.rar" -> tvshow
    /// - "Joker.2019.BluRay.x264.YIFY.zip" -> movie
    /// - "Stranger.Things.S03E01.001" -> tvshow
    /// - "True.Detective.S02E03.720p.HDTV.x264-KILLERS.r001" -> tvshow
    /// - "Avatar.2009.DISC2.ISO" -> movie
    /// - "Lost.S04E02.PT-BR.1080p.WEB-DL.mkv" -> tvshow
    /// - "Better.Call.Saul.S01E07.720p.WEB-DL.x264-GROUP.part02.rar" -> tvshow
    /// - "Forrest.Gump.1994.DISC1.1080p.BluRay.iso" -> movie
    /// - "Se7en.1995.avi" -> movie
    /// - "24.S01E01.avi" -> tvshow
    /// - "13.Reasons.Why.S02E01.mkv" -> tvshow
    /// - "2012.2009.BluRay.avi" -> movie
    /// - "John.Wick.Chapter.3.Parabellum.2019.720p.BluRay.mkv" -> movie
    /// - "ER.101.avi" -> tvshow
    /// - "The.Witcher.S01E01.mkv" -> tvshow
    /// - "Fight.Club.1999.mkv" -> movie
    /// - "10.Things.I.Hate.About.You.1999.mkv" -> movie
    /// - "CSI.204.avi" -> tvshow
    /// - "District.9.2009.BluRay.x264.YIFY.mp4" -> movie
    /// - "Room.104.2017.1080p.WEBRip.x264-STRiFE.mkv" -> tvshow
    /// - "The.Number.23.2007.DVDRip.x264.avi" -> movie
    /// - "Friday.the.13th.2009.BluRay.avi" -> movie
    /// - "Stephen King\The.Lawnmower.Man.1992.DVDRiP.XviD.iNTERNAL-JUSTRiP\CD1\jrp-tlma.r05" -> movie
    /// - "Pulp.Fiction.1994.DVDRip.XviD.AC3\DISC2\pulpfict-ac3.r03" -> movie
    /// - "Gladiator.2000.720p.BluRay.x264-YIFY\CD2\glad-yify.mkv" -> movie
    /// - "Inception.2010.1080p.BluRay.x264.YIFY\Part2\incept-yify.avi" -> movie
    /// - "classics/Jaws.1975.720p.BluRay.x264.YIFY\CD1\jaws1975-yify.mp4" -> movie
    /// - "Blade.Runner.2049.2017.2160p.UHD.BluRay.x265\CD1\blade2049-gp.avi" -> movie
    /// - "sci-fi/Avatar.2.2022.2160p.UHD.BluRay.x265\CD2\avat2-uhd.r05" -> movie
    /// - "Mad.Max.Fury.Road.2015.720p.BluRay.x264-YIFY\CD1\madmax-yify.avi" -> movie
    /// - "best/King.Kong.1933.REMASTERED.720p.BluRay.x264\CD2\kong1933-remast.mp4" -> movie
    /// - "Breaking.Bad.S05E14.720p.HDTV.x264-IMMERSE\CD1\bb-s05e14.r11" -> tvshow
    /// - "shows/Game.of.Thrones.S08E03.1080p.WEB.H264-MEMENTO\DISC1\got-s08e03.mkv" -> tvshow
    /// - "Stranger.Things.S04E01.Chapter.One.720p.NF.WEB-DL.DDP5.1.x264-NTb\CD2\st-s04e01.r10" -> tvshow
    /// - "The.Office.US.S07E17.720p.NF.WEB-DL.DDP5.1.x264-NTb\PART1\office-s07e17.avi" -> tvshow
    /// - "favs/Friends.2x11.480p.DVD.x264-SAiNTS\CD1\friends-2x11.r06" -> tvshow
    /// - "Lost.S04E02.PT-BR.1080p.WEB-DL\CD2\lost-s04e02.mp4" -> tvshow
    /// - "sci-fi/The.Expanse.S03E08.720p.WEB-DL.x264-MEMENTO\CD1\expanse-s03e08.r14" -> tvshow
    /// - "The Shawshank Redemption 1994 1080p BluRay x264 YIFY.mp4" -> movie
    /// - "Game of Thrones S08E03 1080p WEB H264-MEMENTO.mkv" -> tvshow
    /// - "Better Call Saul S03E05 1080p AMZN WEBRip DDP5.1 x264-NTb.mkv" -> tvshow
    /// - "Forrest Gump 1994 720p BluRay x264 YIFY.mp4" -> movie
    /// - "Rick and Morty S05E01E02 720p WEBRip x264-ION10.mkv" -> tvshow
    /// - "The Mandalorian/S01E02 720p WEBRip x264-GalaxyTV.mkv" -> tvshow
    /// - "Mad Max Fury Road 2015 720p BluRay x264 YIFY.mp4" -> movie
    /// - "Stranger Things/S04E01 Chapter One 720p NF WEB-DL DDP5.1 x264-NTb.mkv" -> tvshow
    /// - "10 Things I Hate About You 1999.mkv" -> movie
    /// - "Movies Super Hero/Spider-Man Into the Spider-Verse 2018 1080p BluRay x264 YIFY.mp4" -> movie
    /// - "The Office/US S07E17 720p NF WEB-DL DDP5.1 x264-NTb.mkv" -> tvshow
    /// - "Sherlock S02 E03 1080p BluRay x264-SHORTCUT.mkv" -> tvshow
    /// - "King Kong 1933 REMASTERED 720p BluRay x264-GROUP.mkv" -> movie
    /// - "Edge of Tomorrow 2014 720p BluRay x264 YIFY.mkv" -> movie
    /// - "True Detective S02E01 720p HDTV x264-KILLERS.mkv" -> tvshow
    /// - "Show Name 2022 2x07 720p WEB-DL.mkv" -> tvshow
    /// - "Blade Runner 2049 2017 1080p BluRay x264-GROUP.mkv" -> movie
    /// - "La La Land 2016 1080p BluRay x264 YIFY.mp4" -> movie
    /// - "John Wick Chapter 3 Parabellum 2019 720p BluRay.mkv" -> movie
    /// - "readme.md" -> unknown
    OUTPUT
}

#[ai_function]
pub fn identify_media_format_from_filename(_input_filename: &str) -> &str {
    /// Input: Takes in the filename the user wants to be analyzed.
    /// Function: Analyzes the input filename and returns if it is compressed or decompressed.
    /// Important:
    /// - Work to the best of your knowledge and use filename conventions to make an informed decision.
    /// - Only if you cannot reasonably determine whether the filename represents a compressed or decompressed file, return "unknown".
    /// - "unknown" must be your last resort—try to classify as "compressed" or "decompressed" whenever possible.
    /// - The output must be exactly one of: "compressed", "decompressed", or "unknown". No explanation or context. No other value.
    /// - Output must be a single token: "compressed", "decompressed", or "unknown". No leading or trailing spaces or newlines.
    /// - Ignore the letter case when analyzing the filename.
    /// Output:
    /// - If the filename string contains extensions that indicate the file is compressed, return "compressed";
    /// - If the filename string does not contain extensions that indicate the file is compressed, return "decompressed";
    /// - If you cannot reasonably classify, return "unknown" as a last resort.
    /// Examples:
    /// - "The.Matrix.1999.1080p.BluRay.x264.DTS-FGT.mkv" -> decompressed
    /// - "Inception.2010.720p.BluRay.x264.YIFY.mp4" -> decompressed
    /// - "The.Matrix.1999.1080p.BluRay.x264.DTS-FGT.rar" -> compressed
    /// - "Inception.2010.1080p.BluRay.x264.YIFY.7z" -> compressed
    /// - "Better.Call.Saul.S01E07.720p.WEB-DL.x264-GROUP.part02.rar" -> compressed
    /// - "Breaking.Bad.S02E10.720p.HDTV.x264-IMMERSE.r00" -> compressed
    /// - "Stranger.Things.S03E01.001" -> compressed
    /// - "True.Detective.S02E03.720p.HDTV.x264-KILLERS.r001" -> compressed
    /// - "Holiday.Pictures.2022.zip" -> compressed
    /// - "Forrest.Gump.1994.DISC1.1080p.BluRay.iso" -> compressed
    /// - "Show.Name.backup.tar.gz" -> compressed
    /// - "Series.S01E01.avi" -> decompressed
    /// - "Season1_Episode1_showname.mkv" -> decompressed
    /// - "home.mkv" -> decompressed
    /// - "home.rar" -> compressed
    /// - "home.r10" -> compressed
    /// - "home.r042" -> compressed
    /// - "video-0001.mkv" -> decompressed
    /// - "aa-23-1111.mkv" -> decompressed
    /// - "aa-23-1111.rar" -> compressed
    /// - "za-23-1111.r01" -> compressed
    /// - "zb-21111.r002" -> compressed
    /// - "f-23-1111.r003" -> compressed
    /// - "Stephen King\The.Lawnmower.Man.1992.DVDRiP.XviD.iNTERNAL-JUSTRiP\CD1\jrp-tlma.r05" -> compressed
    /// - "Pulp.Fiction.1994.DVDRip.XviD.AC3\DISC2\pulpfict-ac3.r03" -> compressed
    /// - "Gladiator.2000.720p.BluRay.x264-YIFY\CD2\glad-yify.mkv" -> decompressed
    /// - "Inception.2010.1080p.BluRay.x264.YIFY\Part2\incept-yify.avi" -> decompressed
    /// - "classics/Jaws.1975.720p.BluRay.x264.YIFY\CD1\jaws1975-yify.mp4" -> decompressed
    /// - "Blade.Runner.2049.2017.2160p.UHD.BluRay.x265\CD1\blade2049-gp.avi" -> decompressed
    /// - "sci-fi/Avatar.2.2022.2160p.UHD.BluRay.x265\CD2\avat2-uhd.rar" -> compressed
    /// - "Mad.Max.Fury.Road.2015.720p.BluRay.x264-YIFY\CD1\madmax-yify.avi" -> decompressed
    /// - "best/King.Kong.1933.REMASTERED.720p.BluRay.x264\CD2\kong1933-remast.mp4" -> decompressed
    /// - "Breaking.Bad.S05E14.720p.HDTV.x264-IMMERSE\CD1\bb-s05e14.r11" -> compressed
    /// - "shows/Game.of.Thrones.S08E03.1080p.WEB.H264-MEMENTO\DISC1\got-s08e03.mkv" -> decompressed
    /// - "Stranger.Things.S04E01.Chapter.One.720p.NF.WEB-DL.DDP5.1.x264-NTb\CD2\st-s04e01.r10" -> compressed
    /// - "The.Office.US.S07E17.720p.NF.WEB-DL.DDP5.1.x264-NTb\PART1\office-s07e17.avi" -> decompressed
    /// - "favs/Friends.2x11.480p.DVD.x264-SAiNTS\CD1\friends-2x11.rar" -> compressed
    /// - "Lost.S04E02.PT-BR.1080p.WEB-DL\CD2\lost-s04e02.mp4" -> decompressed
    /// - "sci-fi/The.Expanse.S03E08.720p.WEB-DL.x264-MEMENTO\CD1\expanse-s03e08.r14" -> compressed
    /// - "The Shawshank Redemption 1994 1080p BluRay x264 YIFY.mp4" -> decompressed
    /// - "Game of Thrones S08E03 1080p WEB H264-MEMENTO.mkv" -> decompressed
    /// - "Better Call Saul S03E05 1080p AMZN WEBRip DDP5.1 x264-NTb.zip" -> compressed
    /// - "Forrest Gump 1994 720p BluRay x264 YIFY.7z" -> compressed
    /// - "Rick and Morty S05E01E02 720p WEBRip x264-ION10.rar" -> compressed
    /// - "The Mandalorian/S01E02 720p WEBRip x264-GalaxyTV.part1.rar" -> compressed
    /// - "Mad Max Fury Road 2015 720p BluRay x264 YIFY.r001" -> compressed
    /// - "Stranger Things/S04E01 Chapter One 720p NF WEB-DL DDP5.1 x264-NTb.r999" -> compressed
    /// - "10 Things I Hate About You 1999.mkv" -> decompressed
    /// - "Movies Super Hero/Spider-Man Into the Spider-Verse 2018 1080p BluRay x264 YIFY.mp4" -> decompressed
    /// - "The Office/US S07E17 720p NF WEB-DL DDP5.1 x264-NTb.r15" -> compressed
    /// - "Sherlock S02 E03 1080p BluRay x264-SHORTCUT.mkv" -> decompressed
    /// - "King Kong 1933 REMASTERED 720p BluRay x264-GROUP.iso" -> compressed
    /// - "Edge of Tomorrow 2014 720p BluRay x264 YIFY.mkv" -> decompressed
    /// - "True Detective S02E01 720p HDTV x264-KILLERS.zip" -> compressed
    /// - "Show Name 2022 2x07 720p WEB-DL.rar" -> compressed
    /// - "Blade Runner 2049 2017 1080p BluRay x264-GROUP.7z" -> compressed
    /// - "La La Land 2016 1080p BluRay x264 YIFY.mp4" -> decompressed
    /// - "John Wick Chapter 3 Parabellum 2019 720p BluRay.mkv" -> decompressed
    /// - "README.txt" -> unknown
    /// - "video_without_extension" -> unknown
    OUTPUT
}

#[ai_function]
pub fn is_main_archive_file(_input_filename: &str) -> &str {
    /// Input: Takes in the filename to be analyzed.
    /// Function: Determines whether the given filename is the main file of a multi-part compressed archive.
    /// Important:
    /// - Use filename conventions and common archive patterns to make an informed decision.
    /// - Only if you cannot reasonably determine, return "unknown".
    /// - "unknown" must be your last resort—try to classify as "true" (main file) or "false" (not main file) whenever possible.
    /// - The output must be exactly one of: "true", "false", or "unknown". No explanation or context. No other value.
    /// - Output must be a single token: "true", "false", or "unknown". No leading or trailing spaces or newlines.
    /// - Ignore letter case when analyzing the filename.
    /// Output:
    /// - If the filename string indicates the file is the main archive file (e.g., ends with ".rar", ".zip", ".7z", ".tar", ".tar.gz", ".tgz" with no part/number in the name), return "true";
    /// - If the filename string indicates the file is a part of a multi-part archive (e.g., ends with ".part01.rar", ".001", ".r01", ".z01", ".part2.7z", etc.), return "false";
    /// - If you cannot reasonably classify, return "unknown" as a last resort.
    /// Examples:
    /// - "Movie.Title.2019.1080p.BluRay.x264-GROUP.rar" -> true
    /// - "Show.Name.S01E01.1080p.WEB-DL-GROUP.zip" -> true
    /// - "Big.Archive.2023.7z" -> true
    /// - "Movie.Title.2019.1080p.BluRay.x264-GROUP.part01.rar" -> false
    /// - "Movie.Title.2019.1080p.BluRay.x264-GROUP.part1.rar" -> false
    /// - "Movie.Title.2019.1080p.BluRay.x264-GROUP.part2.rar" -> false
    /// - "Movie.Title.2019.1080p.BluRay.x264-GROUP.001" -> false
    /// - "Show.Name.S01E01.1080p.WEB-DL-GROUP.002" -> false
    /// - "Big.Archive.2023.r01" -> false
    /// - "Big.Archive.2023.r00" -> false
    /// - "Big.Archive.2023.z01" -> false
    /// - "Big.Archive.2023.z02" -> false
    /// - "Video.Collection.part03.7z" -> false
    /// - "Backup.April.2022.tar.gz" -> true
    /// - "Backup.April.2022.tgz" -> true
    /// - "Backup.April.2022.tar" -> true
    /// - "Backup.April.2022.part2.tar.gz" -> false
    /// - "Backup.April.2022.part01.tgz" -> false
    /// - "Series.S01E01.mkv" -> false
    /// - "Holiday.Pictures.2022.zip" -> true
    /// - "Holiday.Pictures.2022.z01" -> false
    /// - "Stephen King\The.Lawnmower.Man.1992.DVDRiP.XviD.iNTERNAL-JUSTRiP\CD1\jrp-tlma.r05" -> false
    /// - "Pulp.Fiction.1994.DVDRip.XviD.AC3\DISC2\pulpfict-ac3.r03" -> false
    /// - "Gladiator.2000.720p.BluRay.x264-YIFY\CD2\glad-yify.mkv" -> false
    /// - "Inception.2010.1080p.BluRay.x264.YIFY\Part2\incept-yify.avi" -> false
    /// - "classics/Jaws.1975.720p.BluRay.x264.YIFY\CD1\jaws1975-yify.mp4" -> false
    /// - "Blade.Runner.2049.2017.2160p.UHD.BluRay.x265\CD1\blade2049-gp.avi" -> false
    /// - "sci-fi/Avatar.2.2022.2160p.UHD.BluRay.x265\CD2\avat2-uhd.rar" -> true
    /// - "Mad.Max.Fury.Road.2015.720p.BluRay.x264-YIFY\CD1\madmax-yify.avi" -> false
    /// - "best/King.Kong.1933.REMASTERED.720p.BluRay.x264\CD2\kong1933-remast.mp4" -> false
    /// - "Breaking.Bad.S05E14.720p.HDTV.x264-IMMERSE\CD1\bb-s05e14.r11" -> false
    /// - "shows/Game.of.Thrones.S08E03.1080p.WEB.H264-MEMENTO\DISC1\got-s08e03.mkv" -> false
    /// - "Stranger.Things.S04E01.Chapter.One.720p.NF.WEB-DL.DDP5.1.x264-NTb\CD2\st-s04e01.r10" -> false
    /// - "The.Office.US.S07E17.720p.NF.WEB-DL.DDP5.1.x264-NTb\PART1\office-s07e17.avi" -> false
    /// - "favs/Friends.2x11.480p.DVD.x264-SAiNTS\CD1\friends-2x11.rar" -> true
    /// - "Lost.S04E02.PT-BR.1080p.WEB-DL\CD2\lost-s04e02.mp4" -> false
    /// - "sci-fi/The.Expanse.S03E08.720p.WEB-DL.x264-MEMENTO\CD1\expanse-s03e08.r14" -> false
    /// - "The Shawshank Redemption 1994 1080p BluRay x264 YIFY.mp4" -> false
    /// - "Game of Thrones S08E03 1080p WEB H264-MEMENTO.mkv" -> false
    /// - "Better Call Saul S03E05 1080p AMZN WEBRip DDP5.1 x264-NTb.zip" -> true
    /// - "Forrest Gump 1994 720p BluRay x264 YIFY.7z" -> true
    /// - "Rick and Morty S05E01E02 720p WEBRip x264-ION10.rar" -> true
    /// - "The Mandalorian/S01E02 720p WEBRip x264-GalaxyTV.part1.rar" -> false
    /// - "Mad Max Fury Road 2015 720p BluRay x264 YIFY.r001" -> false
    /// - "Stranger Things/S04E01 Chapter One 720p NF WEB-DL DDP5.1 x264-NTb.r999" -> false
    /// - "10 Things I Hate About You 1999.mkv" -> false
    /// - "Movies Super Hero/Spider-Man Into the Spider-Verse 2018 1080p BluRay x264 YIFY.mp4" -> false
    /// - "The Office/US S07E17 720p NF WEB-DL DDP5.1 x264-NTb.r15" -> false
    /// - "Sherlock S02 E03 1080p BluRay x264-SHORTCUT.mkv" -> false
    /// - "King Kong 1933 REMASTERED 720p BluRay x264-GROUP.iso" -> true
    /// - "Edge of Tomorrow 2014 720p BluRay x264 YIFY.mkv" -> false
    /// - "True Detective S02E01 720p HDTV x264-KILLERS.zip" -> true
    /// - "Show Name 2022 2x07 720p WEB-DL.rar" -> true
    /// - "Blade Runner 2049 2017 1080p BluRay x264-GROUP.7z" -> true
    /// - "La La Land 2016 1080p BluRay x264 YIFY.mp4" -> false
    /// - "John Wick Chapter 3 Parabellum 2019 720p BluRay.mkv" -> false
    /// - "sample" -> unknown
    /// - "README.txt" -> unknown
    OUTPUT
}

#[ai_function]
pub fn extract_movie_title_from_filename(_input_filename: &str) -> &str {
    /// Input: Takes in a filename string that is known to represent a movie (not a TV episode, not a compressed archive part).
    /// Function: Extracts and returns only the title of the movie, cleaned and as close as possible to the original release name, with spaces and proper capitalization.
    /// Important:
    /// - Ignore resolution, codecs, year, quality, group tags, file extension, and any extra descriptors.
    /// - Return only the movie title—no year, no quality, no tags, no extension, no explanation, no context.
    /// - Format the title with spaces and proper capitalization (e.g., "The Lord of the Rings - The Return of the King").
    /// - Remove dots, dashes, and underscores that separate title words.
    /// - If you cannot reasonably extract a movie title, return "unknown" (but this must be your last resort).
    /// - The output must be a single line, with no extra spaces at the start or end.
    /// Examples:
    /// - "The.Matrix.1999.1080p.BluRay.x264.DTS-FGT.mkv" -> The Matrix
    /// - "Inception.2010.720p.BluRay.x264.YIFY.mp4" -> Inception
    /// - "Joker.2019.BluRay.x264.YIFY.zip" -> Joker
    /// - "Parasite.2019.KOREAN.1080p.BluRay.x264.DTS-FGT.mkv" -> Parasite
    /// - "1917.2019.2160p.UHD.BluRay.X265-IAMABLE.mkv" -> 1917
    /// - "Spider-Man.Into.the.Spider-Verse.2018.1080p.BluRay.x264.YIFY.mp4" -> Spider-Man - Into the Spider Verse
    /// - "Gladiator.2000.1080p.BluRay.x264.YIFY.mp4" -> Gladiator
    /// - "Mad.Max.Fury.Road.2015.720p.BluRay.x264.YIFY.mp4" -> Mad Max - Fury Road
    /// - "Interstellar.2014.1080p.BluRay.x264.YIFY.mp4" -> Interstellar
    /// - "Jaws.1975.720p.BluRay.x264.YIFY.mp4" -> Jaws
    /// - "La.La.Land.2016.1080p.BluRay.x264.YIFY.mp4" -> La La Land
    /// - "Forrest.Gump.1994.720p.BluRay.x264.YIFY.mp4" -> Forrest Gump
    /// - "Dune.Part.One.2021.1080p.BluRay.x264-GROUP.mkv" -> Dune Part One
    /// - "Avatar.2.2022.2160p.UHD.BluRay.x265.mkv" -> Avatar 2
    /// - "Movie.2001.Space.Odyssey.1968.720p.BluRay.x264.YIFY.mp4" -> 2001 Space Odyssey
    /// - "Alien3.1992.720p.BluRay.x264.YIFY.mp4" -> Alien 3
    /// - "Up.2009.1080p.BluRay.x264.YIFY.mp4" -> Up
    /// - "10.Things.I.Hate.About.You.1999.mkv" -> 10 Things I Hate About You
    /// - "District.9.2009.BluRay.x264.YIFY.mp4" -> District 9
    /// - "The.Number.23.2007.DVDRip.x264.avi" -> The Number 23
    /// - "Friday.the.13th.2009.BluRay.avi" -> Friday the 13th
    /// - "Edge.of.Tomorrow.2014.720p.BluRay.x264.YIFY.mkv" -> Edge of Tomorrow
    /// - "King.Kong.1933.REMASTERED.720p.BluRay.x264-GROUP.mkv" -> King Kong
    /// - "Se7en.1995.avi" -> Se7en
    /// - "John.Wick.Chapter.3.Parabellum.2019.720p.BluRay.mkv" -> John Wick - Chapter 3 Parabellum
    /// - "Fight.Club.1999.mkv" -> Fight Club
    /// - "Stephen King\The.Lawnmower.Man.1992.DVDRiP.XviD.iNTERNAL-JUSTRiP\CD1\jrp-tlma.r05" -> The Lawnmower Man
    /// - "Pulp.Fiction.1994.DVDRip.XviD.AC3\DISC2\pulpfict-ac3.r03" -> Pulp Fiction
    /// - "Gladiator.2000.720p.BluRay.x264-YIFY\CD2\glad-yify.mkv" -> Gladiator
    /// - "Inception.2010.1080p.BluRay.x264.YIFY\Part2\incept-yify.avi" -> Inception
    /// - "classics/Jaws.1975.720p.BluRay.x264.YIFY\CD1\jaws1975-yify.mp4" -> Jaws
    /// - "Blade.Runner.2049.2017.2160p.UHD.BluRay.x265\CD1\blade2049-gp.avi" -> Blade Runner 2049
    /// - "sci-fi/Avatar.2.2022.2160p.UHD.BluRay.x265\CD2\avat2-uhd.rar" -> Avatar 2
    /// - "Mad.Max.Fury.Road.2015.720p.BluRay.x264-YIFY\CD1\madmax-yify.avi" -> Mad Max Fury Road
    /// - "best/King.Kong.1933.REMASTERED.720p.BluRay.x264\CD2\kong1933-remast.mp4" -> King Kong
    /// - "The Shawshank Redemption 1994 1080p BluRay x264 YIFY.mp4" -> The Shawshank Redemption
    /// - "Forrest Gump 1994 720p BluRay x264 YIFY.7z" -> Forrest Gump
    /// - "Mad Max Fury Road 2015 720p BluRay x264 YIFY.r001" -> Mad Max Fury Road
    /// - "10 Things I Hate About You 1999.mkv" -> 10 Things I Hate About You
    /// - "Movies Super Hero/Spider-Man Into the Spider-Verse 2018 1080p BluRay x264 YIFY.mp4" -> Spider-Man Into the Spider-Verse
    /// - "King Kong 1933 REMASTERED 720p BluRay x264-GROUP.iso" -> King Kong
    /// - "Edge of Tomorrow 2014 720p BluRay x264 YIFY.mkv" -> Edge of Tomorrow
    /// - "Blade Runner 2049 2017 1080p BluRay x264-GROUP.7z" -> Blade Runner 2049
    /// - "La La Land 2016 1080p BluRay x264 YIFY.mp4" -> La La Land
    /// - "John Wick Chapter 3 Parabellum 2019 720p BluRay.mkv" -> John Wick Chapter 3 Parabellum
    /// - "Show.Name.S01E01.1080p.WEB-DL-GROUP.mkv" -> unknown
    /// - "README.txt" -> unknown
    OUTPUT
}

#[ai_function]
pub fn extract_tv_show_title_from_filename(_input_filename: &str) -> &str {
    /// Input: Takes in a filename string that is known to represent a TV show episode (not a movie, not a compressed archive part).
    /// Function: Extracts and returns only the title of the TV show, cleaned and as close as possible to the original show name, with spaces and proper capitalization.
    /// Important:
    /// - Ignore season/episode markers, year, quality, codecs, group tags, file extension, and any extra descriptors.
    /// - Return only the show title—no year, no S01E01, no group tags, no explanation, no context.
    /// - Format the title with spaces and proper capitalization (e.g., "Game of Thrones").
    /// - Remove dots, dashes, and underscores that separate title words.
    /// - If you cannot reasonably extract a TV show title, return "unknown" (but this must be your last resort).
    /// - The output must be a single line, with no extra spaces at the start or end.
    /// Examples:
    /// - "Breaking.Bad.S05E14.720p.HDTV.x264-IMMERSE.mkv" -> Breaking Bad
    /// - "Game.of.Thrones.S08E03.1080p.WEB.H264-MEMENTO.mkv" -> Game of Thrones
    /// - "Stranger.Things.S04E01.Chapter.One.720p.NF.WEB-DL.DDP5.1.x264-NTb.mkv" -> Stranger Things
    /// - "Friends.2x11.480p.DVD.x264-SAiNTS.mkv" -> Friends
    /// - "The.Office.US.S07E17.720p.NF.WEB-DL.DDP5.1.x264-NTb.mkv" -> The Office US
    /// - "The.Witcher.S01E01.720p.WEBRip.x264-GalaxyTV.mkv" -> The Witcher
    /// - "Better.Call.Saul.S03E05.1080p.AMZN.WEBRip.DDP5.1.x264-NTb.mkv" -> Better Call Saul
    /// - "Lost.S01E01.Pilot.1080p.BluRay.x264-ROVERS.mkv" -> Lost
    /// - "Chernobyl.2019.S01E03.720p.WEB-DL.x264-MEMENTO.mkv" -> Chernobyl
    /// - "The.Expanse.S03E08.720p.WEB-DL.x264-MEMENTO.mkv" -> The Expanse
    /// - "True.Detective.S02E01.720p.HDTV.x264-KILLERS.mkv" -> True Detective
    /// - "The.Mandalorian.S01E02.720p.WEBRip.x264-GalaxyTV.mkv" -> The Mandalorian
    /// - "ShowName_S06_E12_HDTV.mp4" -> Show Name
    /// - "Rick.and.Morty.S05E01E02.720p.WEBRip.x264-ION10.mkv" -> Rick and Morty
    /// - "Seinfeld.821.720p.HDTV.x264-GROUP.mkv" -> Seinfeld
    /// - "Battlestar.Galactica.2004.S01E01.33.720p.BluRay.x264.mkv" -> Battlestar Galactica 2004
    /// - "13.Reasons.Why.S02E01.mkv" -> 13 Reasons Why
    /// - "24.S01E01.avi" -> 24
    /// - "CSI.204.avi" -> CSI
    /// - "ER.101.avi" -> ER
    /// - "Room.104.2017.1080p.WEBRip.x264-STRiFE.mkv" -> Room 104
    /// - "Breaking.Bad.S05E14.720p.HDTV.x264-IMMERSE\CD1\bb-s05e14.r11" -> Breaking Bad
    /// - "shows/Game.of.Thrones.S08E03.1080p.WEB.H264-MEMENTO\DISC1\got-s08e03.mkv" -> Game of Thrones
    /// - "Stranger.Things.S04E01.Chapter.One.720p.NF.WEB-DL.DDP5.1.x264-NTb\CD2\st-s04e01.r10" -> Stranger Things
    /// - "The.Office.US.S07E17.720p.NF.WEB-DL.DDP5.1.x264-NTb\PART1\office-s07e17.avi" -> The Office
    /// - "favs/Friends.2x11.480p.DVD.x264-SAiNTS\CD1\friends-2x11.rar" -> Friends
    /// - "Lost.S04E02.PT-BR.1080p.WEB-DL\CD2\lost-s04e02.mp4" -> Lost
    /// - "sci-fi/The.Expanse.S03E08.720p.WEB-DL.x264-MEMENTO\CD1\expanse-s03e08.r14" -> The Expanse
    /// - "Game of Thrones S08E03 1080p WEB H264-MEMENTO.mkv" -> Game of Thrones
    /// - "Better Call Saul S03E05 1080p AMZN WEBRip DDP5.1 x264-NTb.zip" -> Better Call Saul
    /// - "Rick and Morty S05E01E02 720p WEBRip x264-ION10.rar" -> Rick and Morty
    /// - "The Mandalorian/S01E02 720p WEBRip x264-GalaxyTV.part1.rar" -> The Mandalorian
    /// - "Stranger Things/S04E01 Chapter One 720p NF WEB-DL DDP5.1 x264-NTb.r999" -> Stranger Things
    /// - "The Office/US S07E17 720p NF WEB-DL DDP5.1 x264-NTb.r15" -> The Office US
    /// - "Sherlock S02 E03 1080p BluRay x264-SHORTCUT.mkv" -> Sherlock
    /// - "True Detective S02E01 720p HDTV x264-KILLERS.zip" -> True Detective
    /// - "README.txt" -> unknown
    OUTPUT
}

#[ai_function]
pub fn extract_season_episode_from_filename(_input_filename: &str) -> &str {
    /// Input: Takes in a filename string that is known to represent a TV show episode.
    /// Function: Extracts and returns the season and episode number in the format: "season:X, episode:Y" (e.g., "season:1, episode:2").
    /// Important:
    /// - Only return the season and episode numbers, not titles, quality, or any other info.
    /// - Detect SxxEyy, 1x02, or similar patterns.
    /// - For double-episode files, return the first episode (e.g., S01E01E02 = episode 1).
    /// - If you cannot reasonably extract both season and episode, return "unknown" (this must be your last resort).
    /// - Output must match exactly: "season:X, episode:Y" (no leading zeros, no explanation).
    /// Examples:
    /// - "Breaking.Bad.S05E14.720p.HDTV.x264-IMMERSE.mkv" -> season:5, episode:14
    /// - "Game.of.Thrones.S08E03.1080p.WEB.H264-MEMENTO.mkv" -> season:8, episode:3
    /// - "Stranger.Things.S04E01.Chapter.One.720p.NF.WEB-DL.DDP5.1.x264-NTb.mkv" -> season:4, episode:1
    /// - "Friends.2x11.480p.DVD.x264-SAiNTS.mkv" -> season:2, episode:11
    /// - "The.Office.US.S07E17.720p.NF.WEB-DL.DDP5.1.x264-NTb.mkv" -> season:7, episode:17
    /// - "The.Witcher.S01E01.720p.WEBRip.x264-GalaxyTV.mkv" -> season:1, episode:1
    /// - "Lost.S01E01.Pilot.1080p.BluRay.x264-ROVERS.mkv" -> season:1, episode:1
    /// - "Chernobyl.2019.S01E03.720p.WEB-DL.x264-MEMENTO.mkv" -> season:1, episode:3
    /// - "The.Expanse.S03E08.720p.WEB-DL.x264-MEMENTO.mkv" -> season:3, episode:8
    /// - "True.Detective.S02E01.720p.HDTV.x264-KILLERS.mkv" -> season:2, episode:1
    /// - "ShowName_S06_E12_HDTV.mp4" -> season:6, episode:12
    /// - "Rick.and.Morty.S05E01E02.720p.WEBRip.x264-ION10.mkv" -> season:5, episode:1
    /// - "Seinfeld.821.720p.HDTV.x264-GROUP.mkv" -> season:8, episode:21
    /// - "13.Reasons.Why.S02E01.mkv" -> season:2, episode:1
    /// - "24.S01E01.avi" -> season:1, episode:1
    /// - "CSI.204.avi" -> season:2, episode:4
    /// - "ER.101.avi" -> season:1, episode:1
    /// - "Battlestar.Galactica.2004.S01E01.33.720p.BluRay.x264.mkv" -> season:1, episode:1
    /// - "Breaking.Bad.S05E14.720p.HDTV.x264-IMMERSE\CD1\bb-s05e14.r11" -> season:5, episode:14
    /// - "shows/Game.of.Thrones.S08E03.1080p.WEB.H264-MEMENTO\DISC1\got-s08e03.mkv" -> season:8, episode:3
    /// - "Stranger.Things.S04E01.Chapter.One.720p.NF.WEB-DL.DDP5.1.x264-NTb\CD2\st-s04e01.r10" -> season:4, episode:1
    /// - "The.Office.US.S07E17.720p.NF.WEB-DL.DDP5.1.x264-NTb\PART1\office-s07e17.avi" -> season:7, episode:17
    /// - "favs/Friends.2x11.480p.DVD.x264-SAiNTS\CD1\friends-2x11.rar" -> season:2, episode:11
    /// - "Lost.S04E02.PT-BR.1080p.WEB-DL\CD2\lost-s04e02.mp4" -> season:4, episode:2
    /// - "sci-fi/The.Expanse.S03E08.720p.WEB-DL.x264-MEMENTO\CD1\expanse-s03e08.r14" -> season:3, episode:8
    /// - "Game of Thrones S08E03 1080p WEB H264-MEMENTO.mkv" ->  season:8, episode:3
    /// - "Better Call Saul S03E05 1080p AMZN WEBRip DDP5.1 x264-NTb.zip" -> season:3, episode:5
    /// - "Rick and Morty S05E01E02 720p WEBRip x264-ION10.rar" -> season:5, episode:1
    /// - "The Mandalorian/S01E02 720p WEBRip x264-GalaxyTV.part1.rar" -> season:1, episode:2
    /// - "Stranger Things/S04E01 Chapter One 720p NF WEB-DL DDP5.1 x264-NTb.r999" -> season:4, episode:1
    /// - "The Office/US S07E17 720p NF WEB-DL DDP5.1 x264-NTb.r15" -> season:7, episode:17
    /// - "Sherlock S02 E03 1080p BluRay x264-SHORTCUT.mkv" -> season:2, episode:3
    /// - "True Detective S02E01 720p HDTV x264-KILLERS.zip" -> season:2, episode:1
    /// - "README.txt" -> unknown
    OUTPUT
}

#[ai_function]
pub fn extract_episode_title_from_filename(_input_filename: &str) -> &str {
    /// IMPORTANT: You must extract the episode title *only* if it is present in the input filename itself.
    /// Do not infer, deduce, or guess the episode title based on the show name, episode number, or outside information.
    /// You are strictly forbidden from using external knowledge, prior memory, or any context other than the filename string itself.
    /// If no explicit episode title is present as a distinct segment in the filename, you must return "unknown".
    /// Input: Takes in a filename string that is known to represent a TV show episode.
    /// Function: If an explicit episode title is present in the input filename, extract and return it, cleaned and formatted with spaces and proper capitalization. Otherwise, return "unknown".
    /// - Return only the episode title, not the show name, season/episode numbers, quality, or any extra info.
    /// - Remove dots, dashes, and underscores that separate title words.
    /// - Episode title is often found after season/episode info, or before quality tags (e.g., S01E01.Pilot, .Chapter.One., .The.Finale., etc.).
    /// - If you cannot reasonably extract a clear episode title, return "unknown" (this must be your last resort).
    /// - Output must be a single line, with no extra spaces at the start or end.
    /// Examples:
    /// - "Stranger.Things.S04E01.Chapter.One.720p.NF.WEB-DL.DDP5.1.x264-NTb.mkv" -> Chapter One
    /// - "Lost.S01E01.Pilot.1080p.BluRay.x264-ROVERS.mkv" -> Pilot
    /// - "Show.Name.-.S04E10.-.The.Finale.mkv" -> The Finale
    /// - "Battlestar.Galactica.2004.S01E01.33.720p.BluRay.x264.mkv" -> 33
    /// - "Game.of.Thrones.S01E09.Baelor.1080p.BluRay.x264-ROVERS.mkv" -> Baelor
    /// - "ShowName_S06_E12_The_Surprise_HDTV.mp4" -> The Surprise
    /// - "Rick.and.Morty.S05E01.Mort.Dinner.Rick_Andre.720p.WEBRip.x264-ION10.mkv" -> Mort Dinner Rick Andre
    /// - "Friends.S05E14.720p.HDTV.x264-IMMERSE.mkv" -> unknown
    /// - "Game.of.Thrones.S01E09.1080p.BluRay.x264-ROVERS.mkv" -> unknown
    /// - "Lost.S01E04.avi" -> unknown
    /// - "The.Office.US.S07E17.720p.NF.WEB-DL.DDP5.1.x264-NTb.mkv" -> unknown
    /// - "Better.Call.Saul.S03E05.1080p.AMZN.WEBRip.DDP5.1.x264-NTb.mkv" -> unknown
    /// - "Seinfeld.821.720p.HDTV.x264-GROUP.mkv" -> unknown
    /// - "README.txt" -> unknown
    OUTPUT
}