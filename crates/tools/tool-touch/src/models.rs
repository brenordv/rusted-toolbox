use filetime::FileTime;

/// Specifies which timestamps to modify in a touch operation.
///
/// # Variants
/// - `AccessOnly`: Update access time only
/// - `ModifyOnly`: Update modification time only  
/// - `AccessAndModify`: Update both access and modification times
pub enum TouchTimeWord {
    AccessOnly,
    ModifyOnly,
    AccessAndModify,
}

/// Configuration for touch file operations.
///
/// Contains all user-specified options controlling file creation,
/// timestamp updates, and time sources.
///
/// # Fields
/// - `access`: Update access time flag (-a)
/// - `no_create`: Skip file creation flag (-c)
/// - `date`: Specified date/time for both timestamps (-d)
/// - `ignore`: Compatibility flag (ignored, -f)
/// - `no_dereference`: Update symlink instead of target (-n)
/// - `modify`: Update modification time flag (-m)
/// - `reference`: Reference file timestamps (-r)
/// - `time_spec`: Formatted time specification (-t)
/// - `time`: Resolved timestamp update strategy (from -a, -m, --time)
/// - `files`: List of target files
pub struct TouchArgs {
    // Used during CLI parsing to resolve TouchTimeWord, not directly in runtime logic
    #[allow(dead_code)]
    pub access: bool,
    pub no_create: bool,
    pub date: Option<FileTime>,
    // This option is ignored by the original implementation of touch, so we're also ignoring it.
    #[allow(dead_code)]
    pub ignore: bool,
    pub no_dereference: bool,
    // Used during CLI parsing to resolve TouchTimeWord, not directly in runtime logic
    #[allow(dead_code)]
    pub modify: bool,
    pub reference: Option<(FileTime, FileTime)>,
    pub time_spec: Option<FileTime>,
    pub time: TouchTimeWord,
    pub files: Vec<String>,
}

impl TouchArgs {
    /// Returns the appropriate file time based on available time sources.
    ///
    /// Checks time sources in priority order: date, time_spec, reference.
    /// Returns the same time for both accesses and modify when using date or time_spec.
    ///
    /// # Returns
    /// - `Some((access_time, modify_time))`: Time source found
    /// - `None`: No time source specified, use current time
    pub fn get_current_filetime(&self) -> Option<(FileTime, FileTime)> {
        if let Some(date_filetime) = self.date {
            Some((date_filetime, date_filetime))
        } else if let Some(time_spec_filetime) = self.time_spec {
            Some((time_spec_filetime, time_spec_filetime))
        } else {
            self.reference
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args_with(
        date: Option<FileTime>,
        time_spec: Option<FileTime>,
        reference: Option<(FileTime, FileTime)>,
    ) -> TouchArgs {
        TouchArgs {
            access: false,
            no_create: false,
            date,
            ignore: false,
            no_dereference: false,
            modify: false,
            reference,
            time_spec,
            time: TouchTimeWord::AccessAndModify,
            files: vec![],
        }
    }

    #[test]
    fn get_current_filetime_is_none_without_time_sources() {
        assert!(args_with(None, None, None).get_current_filetime().is_none());
    }

    #[test]
    fn get_current_filetime_prefers_date_over_other_sources() {
        let date = FileTime::from_unix_time(100, 0);
        let time_spec = FileTime::from_unix_time(200, 0);
        let reference = (
            FileTime::from_unix_time(300, 0),
            FileTime::from_unix_time(400, 0),
        );
        let args = args_with(Some(date), Some(time_spec), Some(reference));
        assert_eq!(args.get_current_filetime(), Some((date, date)));
    }

    #[test]
    fn get_current_filetime_prefers_time_spec_over_reference() {
        let time_spec = FileTime::from_unix_time(200, 0);
        let reference = (
            FileTime::from_unix_time(300, 0),
            FileTime::from_unix_time(400, 0),
        );
        let args = args_with(None, Some(time_spec), Some(reference));
        assert_eq!(args.get_current_filetime(), Some((time_spec, time_spec)));
    }

    #[test]
    fn get_current_filetime_falls_back_to_reference_pair() {
        let reference = (
            FileTime::from_unix_time(300, 0),
            FileTime::from_unix_time(400, 0),
        );
        let args = args_with(None, None, Some(reference));
        assert_eq!(args.get_current_filetime(), Some(reference));
    }
}
