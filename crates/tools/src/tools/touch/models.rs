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
/// - `access`: Update access time flag
/// - `no_create`: Skip file creation flag
/// - `date`: Specified date/time for both timestamps
/// - `ignore`: Compatibility flag (ignored)
/// - `no_dereference`: Update symlink instead of target
/// - `modify`: Update modification time flag
/// - `reference`: Reference file timestamps (access, modify)
/// - `time_spec`: Formatted time specification
/// - `time`: Which timestamps to update
/// - `files`: List of target files
pub struct TouchArgs {
    pub access: bool,
    pub no_create: bool,
    pub date: Option<FileTime>,
    pub ignore: bool,
    pub no_dereference: bool,
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
    /// Returns the same time for both access and modify when using date or time_spec.
    ///
    /// # Returns
    /// - `Some((access_time, modify_time))`: Time source found
    /// - `None`: No time source specified, use current time
    pub fn get_current_filetime(&self) -> Option<(FileTime, FileTime)> {
        if self.date.is_some() {
            let date_filetime = self.date.unwrap();
            Some((date_filetime, date_filetime))
        } else if self.time_spec.is_some() {
            let time_spec_filetime = self.time_spec.unwrap();
            Some((time_spec_filetime, time_spec_filetime))
        } else if self.reference.is_some() {
            Some(self.reference.unwrap())
        } else {
            None
        }
    }
}
