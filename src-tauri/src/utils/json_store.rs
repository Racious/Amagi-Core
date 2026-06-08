use std::path::Path;
use serde::{Deserialize, Serialize};
use crate::AppError;

pub fn write_json<T: Serialize>(path: &Path, data: &T) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::Io(e.to_string()))?;
    }
    let json = serde_json::to_string_pretty(data)
        .map_err(|e| AppError::Serialization(e.to_string()))?;
    std::fs::write(path, json)
        .map_err(|e| AppError::Io(e.to_string()))
}

pub fn read_json_or_default<T>(path: &Path) -> T
where
    T: for<'de> Deserialize<'de> + Default,
{
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}
