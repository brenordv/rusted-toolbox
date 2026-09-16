use std::io::Write;

/// Flushes stdout and stderr, ignoring flush failures (a closed pipe must not
/// prevent the exit). `std::process::exit` skips `Drop`, so Rust's buffered
/// stdout would otherwise lose output printed just before an exit helper runs.
fn flush_stdio() {
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
}

/// Exits the current process with a status code indicating success.
///
/// This function will immediately terminate the program and return a
/// status code of `0` to the operating system. A status code of `0`
/// conventionally indicates that the program completed successfully.
///
/// # Important
/// - stdout and stderr are flushed before exiting, so buffered `print!` output
///   is not lost.
/// - This function does not run any `Drop` implementations of active variables or resources.
///   Therefore, resources such as open files or sockets may not be properly cleaned up.
/// - Use this function only when an immediate and clean exit is necessary.
pub fn exit_success() -> ! {
    flush_stdio();
    std::process::exit(0);
}

/// Exits the current process with a status code indicating an error.
///
/// This function will immediately terminate the program and return a
/// status code of `1` to the operating system. A status code of `1`
/// conventionally indicates that the program terminated with an error.
///
/// # Important
/// - stdout and stderr are flushed before exiting, so buffered `print!` output
///   is not lost.
/// - This function does not run any `Drop` implementations of active variables or resources.
///   Therefore, resources such as open files or sockets may not be properly cleaned up.
/// - Use this function only when an immediate and clean exit is necessary.
pub fn exit_error() -> ! {
    flush_stdio();
    std::process::exit(1);
}

/// Exits the current process with the specified exit code.
/// For consistency, if the code passed is 0 or 1, will call the corresponding methods.
///
/// # Parameters
/// - `code`: An integer representing the exit code to terminate the process with.
///   By convention, a code of `0` typically indicates successful execution, while a non-zero code
///   signals an error or abnormal termination.
///
/// # Behavior
/// This function terminates the current process immediately, skipping any remaining code execution,
/// including destructors for local variables (i.e., Drop implementations). Because of this, it is
/// advised to use this function cautiously and only in scenarios where an immediate exit is
/// required.
///
/// # Important
/// - stdout and stderr are flushed before exiting, so buffered `print!` output
///   is not lost.
/// - This function does not run any `Drop` implementations of active variables or resources.
///   Therefore, resources such as open files or sockets may not be properly cleaned up.
/// - Use this function only when an immediate and clean exit is necessary.
pub fn exit_with_code(code: i32) -> ! {
    if code == 0 {
        exit_success();
    }

    if code == 1 {
        exit_error();
    }

    flush_stdio();
    std::process::exit(code);
}
