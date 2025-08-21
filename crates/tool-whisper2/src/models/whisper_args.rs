use crate::models::shared_types::RuntimeType;

pub struct WhisperArgs {
    pub host: String,
    pub runtime: RuntimeType,
    pub role: String,
}
