use crate::model::AppType;

/// Publisher or name fragments that mark a runtime, redistributable, or
/// driver.
///
/// These are the entries that look abandoned — nothing launches them
/// directly, so no activity is ever observed — while other software depends
/// on them. Removing one can break an unrelated application, so Stora never
/// suggests it.
const SYSTEM_FRAGMENTS: &[&str] = &[
    "redistributable",
    "runtime",
    "driver",
    "sdk",
    ".net framework",
    "visual c++",
    "directx",
    "vulkan",
    "windows software development kit",
    "update for windows",
    "security update",
    "hotfix",
    "service pack",
    "webview2",
    "microsoft edge update",
    "intel(r) ",
    "nvidia ",
    "amd ",
    "realtek",
    "qualcomm",
];

/// Fragments that identify a game or game launcher's content.
const GAME_FRAGMENTS: &[&str] = &[
    "steam",
    "epic games",
    "gog galaxy",
    "riot",
    "battle.net",
    "origin",
    "ubisoft connect",
    "ea app",
    "rockstar games",
    "roblox",
];

/// Fragments suggesting a small always-running helper.
const UTILITY_FRAGMENTS: &[&str] = &["tray", "agent", "helper", "updater", "notifier", "sync"];

/// Infers what kind of thing an installed entry is.
///
/// Errs toward `DriverOrSystemComponent` when a name looks like a runtime,
/// because that classification is the one that *prevents* a suggestion. A
/// false positive here costs the user nothing; a false negative could cost
/// them a working application.
pub fn infer_type(name: &str, publisher: &str, install_location: Option<&str>) -> AppType {
    let haystack = format!("{name} {publisher}").to_ascii_lowercase();

    if SYSTEM_FRAGMENTS
        .iter()
        .any(|fragment| haystack.contains(fragment))
    {
        return AppType::DriverOrSystemComponent;
    }

    if let Some(location) = install_location {
        let lowered = location.to_ascii_lowercase();
        if lowered.contains("\\steamapps\\")
            || lowered.contains("\\epic games\\")
            || lowered.contains("\\gog galaxy\\")
        {
            return AppType::Game;
        }
        if lowered.contains("\\windowsapps\\") {
            return AppType::StoreApplication;
        }
    }

    if GAME_FRAGMENTS
        .iter()
        .any(|fragment| haystack.contains(fragment))
    {
        return AppType::Game;
    }

    if UTILITY_FRAGMENTS
        .iter()
        .any(|fragment| haystack.contains(fragment))
    {
        return AppType::BackgroundUtility;
    }

    if name.trim().is_empty() {
        return AppType::Unknown;
    }

    AppType::DesktopApplication
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtimes_and_redistributables_are_system_components() {
        for (name, publisher) in [
            (
                "Microsoft Visual C++ 2015-2022 Redistributable (x64)",
                "Microsoft",
            ),
            ("Microsoft .NET Framework 4.8", "Microsoft"),
            ("Microsoft Edge WebView2 Runtime", "Microsoft"),
            ("Windows Software Development Kit", "Microsoft"),
        ] {
            assert_eq!(
                infer_type(name, publisher, None),
                AppType::DriverOrSystemComponent,
                "{name} must never be suggested for removal"
            );
        }
    }

    #[test]
    fn hardware_vendors_are_treated_as_system_components() {
        assert_eq!(
            infer_type("Intel(R) Chipset Device Software", "Intel", None),
            AppType::DriverOrSystemComponent
        );
        assert_eq!(
            infer_type("NVIDIA Graphics Driver", "NVIDIA", None),
            AppType::DriverOrSystemComponent
        );
        assert_eq!(
            infer_type("Realtek High Definition Audio", "Realtek", None),
            AppType::DriverOrSystemComponent
        );
    }

    #[test]
    fn system_components_are_excluded_from_suggestions() {
        let inferred = infer_type("Vulkan Run Time Libraries", "LunarG", None);
        assert!(!inferred.is_suggestable());
    }

    #[test]
    fn install_location_identifies_games() {
        assert_eq!(
            infer_type(
                "Some Game",
                "A Studio",
                Some("D:\\SteamLibrary\\steamapps\\common\\Some Game")
            ),
            AppType::Game
        );
    }

    #[test]
    fn install_location_identifies_store_apps() {
        assert_eq!(
            infer_type(
                "Notes",
                "Contoso",
                Some("C:\\Program Files\\WindowsApps\\Contoso.Notes_1.0")
            ),
            AppType::StoreApplication
        );
    }

    #[test]
    fn launchers_are_classified_as_games() {
        assert_eq!(infer_type("Steam", "Valve", None), AppType::Game);
        assert_eq!(
            infer_type("Roblox Player", "Roblox Corporation", None),
            AppType::Game
        );
    }

    #[test]
    fn helpers_are_background_utilities() {
        assert_eq!(
            infer_type("Dropbox Update Helper", "Dropbox", None),
            AppType::BackgroundUtility
        );
    }

    #[test]
    fn ordinary_software_is_a_desktop_application() {
        assert_eq!(
            infer_type("Visual Studio Code", "Microsoft Corporation", None),
            AppType::DesktopApplication
        );
        assert_eq!(
            infer_type("Blender", "Blender Foundation", None),
            AppType::DesktopApplication
        );
    }

    #[test]
    fn an_empty_name_is_unknown_rather_than_a_guess() {
        assert_eq!(infer_type("", "", None), AppType::Unknown);
    }

    #[test]
    fn classification_is_case_insensitive() {
        assert_eq!(
            infer_type("MICROSOFT VISUAL C++ REDISTRIBUTABLE", "MICROSOFT", None),
            AppType::DriverOrSystemComponent
        );
    }

    #[test]
    fn a_system_match_beats_a_game_match() {
        // "NVIDIA GeForce Experience" contains a vendor fragment; the safer
        // classification must win.
        assert_eq!(
            infer_type("NVIDIA GeForce Experience", "NVIDIA", None),
            AppType::DriverOrSystemComponent
        );
    }
}
