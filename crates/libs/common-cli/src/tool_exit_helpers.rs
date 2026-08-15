/// Exits the current process with a status code indicating success.
///
/// This function will immediately terminate the program and return a
/// status code of `0` to the operating system. A status code of `0`
/// conventionally indicates that the program completed successfully.
///
/// # Important
/// - This function does not run any `Drop` implementations of active variables or resources.
///   Therefore, resources such as open files or sockets may not be properly cleaned up.
/// - Use this function only when an immediate and clean exit is necessary.
pub fn exit_success() {
    std::process::exit(0);
}

/// Exits the current process with a status code indicating an error.
///
/// This function will immediately terminate the program and return a
/// status code of `1` to the operating system. A status code of `1`
/// conventionally indicates that the program completed successfully.
///
/// # Important
/// - This function does not run any `Drop` implementations of active variables or resources.
///   Therefore, resources such as open files or sockets may not be properly cleaned up.
/// - Use this function only when an immediate and clean exit is necessary.
pub fn exit_error() {
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
/// - This function does not run any `Drop` implementations of active variables or resources.
///   Therefore, resources such as open files or sockets may not be properly cleaned up.
/// - Use this function only when an immediate and clean exit is necessary.
pub fn exit_with_code(code: i32) {
    if code == 0 {
        exit_success();
        return;
    }

    if code == 1 {
        exit_error();
        return;
    }

    std::process::exit(code);
}
