use stora_core::model::StorageCategory;

/// Classifies a path into a storage category using observable evidence only.
///
/// This never guesses from file size or age — only from location and, as a
/// last resort, extension. Anything unrecognized stays `Other` rather than
/// being forced into a category to make the chart look tidy.
pub fn classify(path: &str) -> StorageCategory {
    let lowered = path.to_ascii_lowercase();

    // Temporary data first: a temp folder inside a dev project is still temp.
    if contains_segment(&lowered, "\\temp\\")
        || contains_segment(&lowered, "\\tmp\\")
        || lowered.contains("\\windows\\temp\\")
        || lowered.contains("\\inetcache\\")
        || lowered.contains("\\crashdumps\\")
        || lowered.contains("\\$recycle.bin\\")
        || lowered.contains("\\microsoft\\windows\\explorer\\thumbcache")
        || lowered.ends_with(".tmp")
    {
        return StorageCategory::TemporaryFiles;
    }

    if lowered.contains("\\node_modules\\")
        || lowered.contains("\\.cargo\\")
        || lowered.contains("\\.nuget\\")
        || lowered.contains("\\.gradle\\")
        || lowered.contains("\\__pycache__\\")
        || lowered.contains("\\.venv\\")
        || lowered.contains("\\site-packages\\")
        || contains_segment(&lowered, "\\.git\\")
        || lowered.contains("\\appdata\\local\\programs\\microsoft vs code")
        || lowered.contains("\\.vscode\\")
        || lowered.contains("\\docker\\")
        || lowered.contains("\\wsl\\")
    {
        return StorageCategory::Development;
    }

    if lowered.contains("\\steamapps\\")
        || lowered.contains("\\epic games\\")
        || lowered.contains("\\gog galaxy\\")
        || lowered.contains("\\riot games\\")
        || lowered.contains("\\battle.net\\")
        || lowered.contains("\\origin games\\")
    {
        return StorageCategory::Games;
    }

    if lowered.starts_with("c:\\windows")
        || lowered.starts_with("c:\\programdata")
        || lowered.starts_with("c:\\system volume information")
        || lowered.starts_with("c:\\recovery")
        || lowered.starts_with("c:\\perflogs")
    {
        return StorageCategory::System;
    }

    if lowered.starts_with("c:\\program files") || lowered.contains("\\appdata\\") {
        return StorageCategory::Applications;
    }

    if contains_segment(&lowered, "\\documents\\")
        || contains_segment(&lowered, "\\pictures\\")
        || contains_segment(&lowered, "\\videos\\")
        || contains_segment(&lowered, "\\music\\")
        || contains_segment(&lowered, "\\desktop\\")
        || contains_segment(&lowered, "\\downloads\\")
        || contains_segment(&lowered, "\\onedrive\\")
    {
        return StorageCategory::Documents;
    }

    StorageCategory::Other
}

fn contains_segment(lowered_path: &str, segment: &str) -> bool {
    lowered_path.contains(segment)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_windows_as_system() {
        assert_eq!(
            classify("C:\\Windows\\System32\\driver.sys"),
            StorageCategory::System
        );
    }

    #[test]
    fn classifies_program_files_as_applications() {
        assert_eq!(
            classify("C:\\Program Files\\App\\app.exe"),
            StorageCategory::Applications
        );
    }

    #[test]
    fn classifies_development_folders() {
        assert_eq!(
            classify("D:\\Dev\\project\\node_modules\\react\\index.js"),
            StorageCategory::Development
        );
        assert_eq!(
            classify("C:\\Users\\Test\\.cargo\\registry\\cache.crate"),
            StorageCategory::Development
        );
    }

    #[test]
    fn classifies_game_libraries() {
        assert_eq!(
            classify("D:\\SteamLibrary\\steamapps\\common\\Game\\game.pak"),
            StorageCategory::Games
        );
    }

    #[test]
    fn classifies_user_documents() {
        assert_eq!(
            classify("C:\\Users\\Test\\Documents\\report.docx"),
            StorageCategory::Documents
        );
        assert_eq!(
            classify("C:\\Users\\Test\\Downloads\\setup.exe"),
            StorageCategory::Documents
        );
    }

    #[test]
    fn temporary_data_wins_over_its_surroundings() {
        // A temp folder inside a project is still temporary data.
        assert_eq!(
            classify("D:\\Dev\\project\\node_modules\\temp\\build.tmp"),
            StorageCategory::TemporaryFiles
        );
        assert_eq!(
            classify("C:\\Windows\\Temp\\install.log"),
            StorageCategory::TemporaryFiles
        );
    }

    #[test]
    fn unrecognized_paths_stay_other() {
        assert_eq!(
            classify("D:\\Archive\\misc\\thing.bin"),
            StorageCategory::Other
        );
    }

    #[test]
    fn classification_is_case_insensitive() {
        assert_eq!(
            classify("c:\\PROGRAM FILES\\App\\app.exe"),
            StorageCategory::Applications
        );
    }
}
