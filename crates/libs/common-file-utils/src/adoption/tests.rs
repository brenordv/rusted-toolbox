use super::*;
use rstest::rstest;
use tempfile::{TempDir, tempdir};

const OLD_DATA: &[u8] = b"{\"from\":\"the old place\"}";
const OTHER_DATA: &[u8] = b"{\"from\":\"somewhere else\"}";

/// A temp folder with an old place and a new path whose tool folder already holds a `logs`
/// folder, as the logger leaves it.
struct Places {
    dir: TempDir,
    legacy: PathBuf,
    target: PathBuf,
}

impl Places {
    fn new(name: &str) -> Self {
        let dir = tempdir().unwrap();
        let legacy = dir.path().join("old-home").join(name);
        let tool = dir.path().join("root").join("tool");
        fs::create_dir_all(tool.join("logs")).unwrap();
        Self {
            target: tool.join(name),
            legacy,
            dir,
        }
    }

    fn file() -> Self {
        Self::new("config.json")
    }

    fn logs(&self) -> PathBuf {
        self.target.parent().unwrap().join("logs")
    }
}

fn write(path: &Path, content: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn injected() -> io::Error {
    io::Error::other("injected failure")
}

fn hard_link(from: &Path, to: &Path) -> io::Result<()> {
    fs::hard_link(from, to)
}

fn remove(path: &Path) -> io::Result<()> {
    fs::remove_file(path)
}

fn identity(one: &Path, other: &Path) -> io::Result<bool> {
    same_file::is_same_file(one, other)
}

fn failing_move(_: &Path, _: &Path) -> io::Result<()> {
    Err(injected())
}

fn failing_remove(_: &Path) -> io::Result<()> {
    Err(injected())
}

fn failing_identity(_: &Path, _: &Path) -> io::Result<bool> {
    Err(injected())
}

fn file_steps<'a>(
    mover: &'a dyn Fn(&Path, &Path) -> io::Result<()>,
    remover: &'a dyn Fn(&Path) -> io::Result<()>,
) -> Steps<'a> {
    Steps {
        mover,
        remover,
        same_file: &identity,
        same_entry: &same_entry,
    }
}

#[test]
fn a_file_only_in_the_old_place_moves_and_the_logs_folder_stays() {
    let places = Places::file();
    write(&places.legacy, OLD_DATA);

    let adoption = adopt(Some(&places.legacy), &places.target, Shape::File);

    assert!(matches!(&adoption, Adoption::Moved { from } if *from == places.legacy));
    assert_eq!(fs::read(&places.target).unwrap(), OLD_DATA);
    assert!(!places.legacy.exists());
    assert!(places.logs().is_dir());
}

#[test]
fn a_folder_only_in_the_old_place_moves_with_everything_in_it() {
    let places = Places::new("profiles");
    write(&places.legacy.join("dev").join("config.json"), OLD_DATA);
    write(
        &places.legacy.join("dev").join(".eh-data").join("secrets"),
        OTHER_DATA,
    );

    let adoption = adopt(Some(&places.legacy), &places.target, Shape::Folder);

    assert!(matches!(adoption, Adoption::Moved { .. }));
    assert_eq!(
        fs::read(places.target.join("dev").join("config.json")).unwrap(),
        OLD_DATA
    );
    assert_eq!(
        fs::read(places.target.join("dev").join(".eh-data").join("secrets")).unwrap(),
        OTHER_DATA
    );
    assert!(!places.legacy.exists());
    assert!(places.logs().is_dir());
}

#[test]
fn a_move_creates_the_missing_folders_of_the_new_path() {
    let places = Places::file();
    let target = places
        .dir
        .path()
        .join("new-root")
        .join("tool")
        .join("config.json");
    write(&places.legacy, OLD_DATA);

    let adoption = adopt(Some(&places.legacy), &target, Shape::File);

    assert!(matches!(adoption, Adoption::Moved { .. }));
    assert_eq!(fs::read(&target).unwrap(), OLD_DATA);
}

#[rstest]
#[case::no_old_place(false)]
#[case::an_empty_old_place(true)]
fn data_only_at_the_new_path_is_already_there(#[case] has_old_place: bool) {
    let places = Places::file();
    write(&places.target, OLD_DATA);
    let legacy = has_old_place.then_some(places.legacy.as_path());

    let adoption = adopt(legacy, &places.target, Shape::File);

    assert!(matches!(adoption, Adoption::AlreadyThere));
    assert_eq!(fs::read(&places.target).unwrap(), OLD_DATA);
}

#[rstest]
#[case::no_old_place(false)]
#[case::an_empty_old_place(true)]
fn neither_place_holding_data_is_nothing(#[case] has_old_place: bool) {
    let places = Places::file();
    let legacy = has_old_place.then_some(places.legacy.as_path());

    let adoption = adopt(legacy, &places.target, Shape::File);

    assert!(matches!(adoption, Adoption::Nothing));
    assert!(!places.target.exists());
}

#[test]
fn a_new_path_folder_that_cannot_be_created_keeps_the_old_place_in_use() {
    let dir = tempdir().unwrap();
    let legacy = dir.path().join("old").join("config.json");
    let blocker = dir.path().join("tool");
    let target = blocker.join("config.json");
    write(&legacy, OLD_DATA);
    fs::write(&blocker, b"a file where the tool folder goes").unwrap();

    let adoption = adopt(Some(&legacy), &target, Shape::File);

    assert!(matches!(
        &adoption,
        Adoption::KeptLegacy {
            reason: KeptReason::Failed(_),
            ..
        }
    ));
    assert_eq!(adoption.path_in_use(&target), legacy);
    assert_eq!(fs::read(&legacy).unwrap(), OLD_DATA);
}

#[test]
fn a_failed_move_with_the_new_path_still_absent_keeps_the_old_place_in_use() {
    let places = Places::file();
    write(&places.legacy, OLD_DATA);

    let adoption = adopt_with(
        Some(&places.legacy),
        &places.target,
        Shape::File,
        &file_steps(&failing_move, &remove),
    );

    assert!(matches!(
        &adoption,
        Adoption::KeptLegacy {
            reason: KeptReason::Failed(_),
            ..
        }
    ));
    assert_eq!(adoption.path_in_use(&places.target), places.legacy);
    assert_eq!(fs::read(&places.legacy).unwrap(), OLD_DATA);
    assert!(!places.target.exists());
}

#[test]
fn a_failed_move_after_a_rival_moved_the_data_is_already_there() {
    let places = Places::new("profiles");
    write(&places.legacy.join("config.json"), OLD_DATA);
    let rival_then_fail = |from: &Path, to: &Path| {
        fs::rename(from, to).unwrap();
        Err(injected())
    };

    let adoption = adopt_with(
        Some(&places.legacy),
        &places.target,
        Shape::Folder,
        &file_steps(&rival_then_fail, &remove),
    );

    assert!(matches!(adoption, Adoption::AlreadyThere));
    assert_eq!(
        fs::read(places.target.join("config.json")).unwrap(),
        OLD_DATA
    );
}

#[test]
fn a_failed_move_after_the_old_data_vanished_is_nothing() {
    let places = Places::file();
    write(&places.legacy, OLD_DATA);
    let deleted_then_fail = |from: &Path, _: &Path| {
        fs::remove_file(from).unwrap();
        Err(injected())
    };

    let adoption = adopt_with(
        Some(&places.legacy),
        &places.target,
        Shape::File,
        &file_steps(&deleted_then_fail, &remove),
    );

    assert!(matches!(adoption, Adoption::Nothing));
    assert_eq!(adoption.path_in_use(&places.target), places.target);
}

#[test]
fn a_file_move_never_overwrites_a_new_path_created_after_the_look() {
    let places = Places::file();
    write(&places.legacy, OLD_DATA);
    let rival_writes_first = |from: &Path, to: &Path| {
        fs::write(to, OTHER_DATA).unwrap();
        fs::hard_link(from, to)
    };

    let adoption = adopt_with(
        Some(&places.legacy),
        &places.target,
        Shape::File,
        &file_steps(&rival_writes_first, &remove),
    );

    assert!(matches!(&adoption, Adoption::BothPresent { legacy } if *legacy == places.legacy));
    assert_eq!(fs::read(&places.target).unwrap(), OTHER_DATA);
    assert_eq!(fs::read(&places.legacy).unwrap(), OLD_DATA);
}

#[test]
fn one_file_under_two_names_has_its_old_name_removed() {
    let places = Places::file();
    write(&places.legacy, OLD_DATA);
    fs::hard_link(&places.legacy, &places.target).unwrap();

    let adoption = adopt(Some(&places.legacy), &places.target, Shape::File);

    assert!(matches!(adoption, Adoption::Moved { .. }));
    assert!(!places.legacy.exists());
    assert_eq!(fs::read(&places.target).unwrap(), OLD_DATA);
}

#[test]
fn one_file_under_two_names_whose_old_name_cannot_be_removed_keeps_the_old_name_in_use() {
    let places = Places::file();
    write(&places.legacy, OLD_DATA);
    fs::hard_link(&places.legacy, &places.target).unwrap();

    let adoption = adopt_with(
        Some(&places.legacy),
        &places.target,
        Shape::File,
        &file_steps(&hard_link, &failing_remove),
    );

    assert!(matches!(
        &adoption,
        Adoption::KeptLegacy {
            reason: KeptReason::Failed(_),
            ..
        }
    ));
    assert_eq!(adoption.path_in_use(&places.target), places.legacy);
    assert_eq!(fs::read(&places.target).unwrap(), OLD_DATA);
}

#[test]
fn a_file_move_whose_old_name_cannot_be_removed_keeps_the_old_name_in_use() {
    let places = Places::file();
    write(&places.legacy, OLD_DATA);

    let adoption = adopt_with(
        Some(&places.legacy),
        &places.target,
        Shape::File,
        &file_steps(&hard_link, &failing_remove),
    );

    assert!(matches!(
        &adoption,
        Adoption::KeptLegacy {
            reason: KeptReason::Failed(_),
            ..
        }
    ));
    assert_eq!(adoption.path_in_use(&places.target), places.legacy);
    assert_eq!(fs::read(&places.legacy).unwrap(), OLD_DATA);
    assert!(places.target.exists());
}

#[test]
fn two_different_files_with_equal_contents_are_both_present() {
    let places = Places::file();
    write(&places.legacy, OLD_DATA);
    write(&places.target, OLD_DATA);

    let adoption = adopt(Some(&places.legacy), &places.target, Shape::File);

    assert!(matches!(adoption, Adoption::BothPresent { .. }));
    assert!(places.legacy.exists());
    assert!(places.target.exists());
}

#[test]
fn a_failing_identity_check_is_both_present_and_removes_nothing() {
    let places = Places::file();
    write(&places.legacy, OLD_DATA);
    fs::hard_link(&places.legacy, &places.target).unwrap();
    let steps = Steps {
        mover: &hard_link,
        remover: &remove,
        same_file: &failing_identity,
        same_entry: &same_entry,
    };

    let adoption = adopt_with(Some(&places.legacy), &places.target, Shape::File, &steps);

    assert!(matches!(adoption, Adoption::BothPresent { .. }));
    assert!(places.legacy.exists());
    assert!(places.target.exists());
}

#[rstest]
#[case::file(Shape::File)]
#[case::folder(Shape::Folder)]
fn the_same_path_as_old_and_new_place_is_already_there_and_keeps_the_data(#[case] shape: Shape) {
    let places = Places::file();
    write(&places.target, OLD_DATA);

    let adoption = adopt(Some(&places.target), &places.target, shape);

    assert!(matches!(adoption, Adoption::AlreadyThere));
    assert_eq!(fs::read(&places.target).unwrap(), OLD_DATA);
}

#[test]
fn a_failing_entry_check_on_one_file_under_two_names_removes_nothing() {
    let places = Places::file();
    write(&places.legacy, OLD_DATA);
    fs::hard_link(&places.legacy, &places.target).unwrap();
    let steps = Steps {
        mover: &hard_link,
        remover: &remove,
        same_file: &identity,
        same_entry: &failing_identity,
    };

    let adoption = adopt_with(Some(&places.legacy), &places.target, Shape::File, &steps);

    assert!(matches!(adoption, Adoption::BothPresent { .. }));
    assert!(places.legacy.exists());
    assert!(places.target.exists());
}

#[rstest]
#[case::nothing(Adoption::Nothing, "new")]
#[case::already_there(Adoption::AlreadyThere, "new")]
#[case::moved(Adoption::Moved { from: PathBuf::from("old") }, "new")]
#[case::both_present(Adoption::BothPresent { legacy: PathBuf::from("old") }, "new")]
#[case::kept_legacy(
    Adoption::KeptLegacy { legacy: PathBuf::from("old"), reason: KeptReason::SymbolicLink },
    "old"
)]
fn the_path_in_use_is_the_old_place_only_when_the_data_was_kept_there(
    #[case] adoption: Adoption,
    #[case] expected: &str,
) {
    let in_use = adoption.path_in_use(Path::new("new"));

    assert_eq!(in_use, PathBuf::from(expected));
}

#[cfg(windows)]
#[test]
fn a_file_held_open_without_delete_sharing_keeps_its_old_name_in_use() {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_SHARE_READ_WRITE: u32 = 0x1 | 0x2;
    let places = Places::file();
    write(&places.legacy, OLD_DATA);
    let _held = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(FILE_SHARE_READ_WRITE)
        .open(&places.legacy)
        .unwrap();

    let adoption = adopt(Some(&places.legacy), &places.target, Shape::File);

    assert!(matches!(
        &adoption,
        Adoption::KeptLegacy {
            reason: KeptReason::Failed(_),
            ..
        }
    ));
    assert_eq!(adoption.path_in_use(&places.target), places.legacy);
    assert_eq!(fs::read(&places.legacy).unwrap(), OLD_DATA);
}

#[cfg(unix)]
#[test]
fn an_old_place_that_cannot_be_looked_at_is_kept_never_taken_for_empty() {
    let places = Places::file();
    let blocker = places.dir.path().join("blocker");
    fs::write(&blocker, b"a file where a folder goes").unwrap();
    let legacy = blocker.join("config.json");

    let adoption = adopt(Some(&legacy), &places.target, Shape::File);

    assert!(matches!(
        &adoption,
        Adoption::KeptLegacy {
            reason: KeptReason::Failed(_),
            ..
        }
    ));
    assert_eq!(adoption.path_in_use(&places.target), legacy);
    assert!(!places.target.exists());
}

#[cfg(unix)]
mod links {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn an_old_place_that_is_a_symbolic_link_is_kept_in_use() {
        let places = Places::file();
        let real = places.dir.path().join("elsewhere").join("config.json");
        write(&real, OLD_DATA);
        fs::create_dir_all(places.legacy.parent().unwrap()).unwrap();
        symlink(&real, &places.legacy).unwrap();

        let adoption = adopt(Some(&places.legacy), &places.target, Shape::File);

        assert!(matches!(
            &adoption,
            Adoption::KeptLegacy {
                reason: KeptReason::SymbolicLink,
                ..
            }
        ));
        assert_eq!(adoption.path_in_use(&places.target), places.legacy);
        assert!(
            fs::symlink_metadata(&places.legacy)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(!places.target.exists());
    }

    #[rstest]
    #[case::new_path_links_to_the_old(true)]
    #[case::old_place_links_to_the_new(false)]
    fn a_link_between_the_two_places_is_already_there_and_keeps_both_names(
        #[case] new_links_to_old: bool,
    ) {
        let places = Places::file();
        let (real, link) = if new_links_to_old {
            (&places.legacy, &places.target)
        } else {
            (&places.target, &places.legacy)
        };
        write(real, OLD_DATA);
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        symlink(real, link).unwrap();

        let adoption = adopt(Some(&places.legacy), &places.target, Shape::File);

        assert!(matches!(adoption, Adoption::AlreadyThere));
        assert_eq!(fs::read(&places.legacy).unwrap(), OLD_DATA);
        assert_eq!(fs::read(&places.target).unwrap(), OLD_DATA);
    }

    #[test]
    fn a_link_from_the_new_tool_folder_to_the_old_folder_is_already_there_and_keeps_the_data() {
        let places = Places::file();
        let old_folder = places.dir.path().join("old");
        let old_file = old_folder.join("config.json");
        write(&old_file, OLD_DATA);
        let root = places.dir.path().join("linked-root");
        fs::create_dir_all(&root).unwrap();
        symlink(&old_folder, root.join("tool")).unwrap();

        let adoption = adopt(
            Some(&old_file),
            &root.join("tool").join("config.json"),
            Shape::File,
        );

        assert!(matches!(adoption, Adoption::AlreadyThere));
        assert_eq!(fs::read(&old_file).unwrap(), OLD_DATA);
    }

    #[test]
    fn a_dangling_link_beside_data_at_the_new_path_is_both_present() {
        let places = Places::file();
        write(&places.target, OLD_DATA);
        fs::create_dir_all(places.legacy.parent().unwrap()).unwrap();
        symlink(places.dir.path().join("gone"), &places.legacy).unwrap();

        let adoption = adopt(Some(&places.legacy), &places.target, Shape::File);

        assert!(matches!(adoption, Adoption::BothPresent { .. }));
        assert!(fs::symlink_metadata(&places.legacy).is_ok());
        assert_eq!(fs::read(&places.target).unwrap(), OLD_DATA);
    }
}
