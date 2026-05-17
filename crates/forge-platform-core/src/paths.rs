use std::path::PathBuf;

use directories::ProjectDirs;

fn project_dirs() -> ProjectDirs {
    #[allow(clippy::expect_used)]
    ProjectDirs::from("com", "icesqueez", "forge")
        .expect("home directory must be discoverable on supported platforms")
}

pub fn config_dir() -> PathBuf {
    project_dirs().config_dir().to_path_buf()
}

pub fn data_dir() -> PathBuf {
    project_dirs().data_dir().to_path_buf()
}

pub fn cache_dir() -> PathBuf {
    project_dirs().cache_dir().to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_dir_is_non_empty() {
        assert!(!data_dir().to_string_lossy().is_empty());
    }

    #[test]
    fn config_dir_is_non_empty() {
        assert!(!config_dir().to_string_lossy().is_empty());
    }

    #[test]
    fn cache_dir_is_non_empty() {
        assert!(!cache_dir().to_string_lossy().is_empty());
    }
}
