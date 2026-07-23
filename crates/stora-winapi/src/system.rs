//! Windows system integration: accent color and well-known folders.

/// Reads the current Windows accent color as a CSS `#rrggbb` string.
///
/// Returns `None` when it cannot be determined, letting the UI fall back to
/// the Memora default rather than guessing.
pub fn accent_color() -> Option<String> {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::BOOL;
        use windows::Win32::Graphics::Dwm::DwmGetColorizationColor;

        let mut colorization: u32 = 0;
        let mut opaque_blend = BOOL(0);

        // SAFETY: both out-params are valid locals.
        let hr = unsafe { DwmGetColorizationColor(&mut colorization, &mut opaque_blend) };
        if hr.is_err() {
            return None;
        }

        // Value is 0xAARRGGBB; the alpha channel is not useful as a CSS color.
        let r = (colorization >> 16) & 0xFF;
        let g = (colorization >> 8) & 0xFF;
        let b = colorization & 0xFF;
        Some(format!("#{r:02x}{g:02x}{b:02x}"))
    }

    #[cfg(not(windows))]
    {
        None
    }
}

/// Expands a `%VAR%`-style Windows path.
pub fn expand_environment(path: &str) -> Option<String> {
    let mut result = String::with_capacity(path.len());
    let mut rest = path;

    while let Some(start) = rest.find('%') {
        result.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let end = after.find('%')?;
        let name = &after[..end];
        let value = std::env::var(name).ok()?;
        result.push_str(&value);
        rest = &after[end + 1..];
    }

    result.push_str(rest);
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_a_known_variable() {
        std::env::set_var("STORA_TEST_DIR", "C:\\Test");
        assert_eq!(
            expand_environment("%STORA_TEST_DIR%\\cache"),
            Some("C:\\Test\\cache".into())
        );
    }

    #[test]
    fn returns_none_for_an_undefined_variable() {
        assert_eq!(expand_environment("%STORA_NOT_SET_ANYWHERE%\\x"), None);
    }

    #[test]
    fn passes_through_paths_without_variables() {
        assert_eq!(
            expand_environment("C:\\Users\\Test"),
            Some("C:\\Users\\Test".into())
        );
    }

    #[test]
    fn accent_color_is_a_css_hex_value_when_available() {
        if let Some(color) = accent_color() {
            assert!(color.starts_with('#'), "got {color}");
            assert_eq!(color.len(), 7, "got {color}");
            assert!(color[1..].chars().all(|c| c.is_ascii_hexdigit()));
        }
    }
}
