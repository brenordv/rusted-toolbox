#[macro_export]
macro_rules! app_name {
    () => {
        env!("CARGO_PKG_NAME")
    };
}
