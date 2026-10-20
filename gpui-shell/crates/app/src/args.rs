//! Command-line argument parsing for GPUi Shell.

use std::path::PathBuf;

/// Command-line arguments.
#[derive(Debug, Default)]
pub struct Args {
    /// Optional input to prefill in the launcher.
    pub input: Option<String>,
    /// Parse the config files, report, and exit without starting the shell.
    pub validate: bool,
    /// Explicit CyberManju cosmetic snapshot; never an auto-discovered secret store.
    pub appearance_profile: Option<PathBuf>,
    /// Watch only the explicitly supplied cosmetic file; never discover app stores.
    pub watch_appearance_profile: bool,
}

impl Args {
    /// Parse command-line arguments from `std::env::args()`.
    pub fn parse() -> Self {
        Self::parse_from(std::env::args().skip(1)).unwrap_or_else(|error| {
            eprintln!("Error: {error}");
            std::process::exit(1)
        })
    }

    fn parse_from(args: impl IntoIterator<Item = String>) -> Result<Self, &'static str> {
        let mut parsed = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--input" | "-i" => {
                    parsed.input = Some(args.next().ok_or("--input requires a value")?);
                }
                "--validate" => parsed.validate = true,
                "--watch-appearance-profile" => {
                    if parsed.watch_appearance_profile {
                        return Err("--watch-appearance-profile may only be supplied once");
                    }
                    parsed.watch_appearance_profile = true;
                }
                "--appearance-profile" => {
                    if parsed.appearance_profile.is_some() {
                        return Err("--appearance-profile may only be supplied once");
                    }
                    let path = args.next().ok_or("--appearance-profile requires a path")?;
                    if path.is_empty() || path.starts_with("--") {
                        return Err("--appearance-profile requires a path");
                    }
                    parsed.appearance_profile = Some(path.into());
                }
                // Preserve the existing handling of unrelated/legacy options.
                _ => {}
            }
        }
        if parsed.watch_appearance_profile && parsed.appearance_profile.is_none() {
            return Err("--watch-appearance-profile requires --appearance-profile FILE");
        }
        Ok(parsed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Args, &'static str> {
        Args::parse_from(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn explicit_profile_and_validation_do_not_steal_launcher_input() {
        let args = parse(&[
            "--validate",
            "--appearance-profile",
            "/tmp/my profile.json",
            "-i",
            "hello",
        ])
        .unwrap();
        assert!(args.validate);
        assert_eq!(args.input.as_deref(), Some("hello"));
        assert_eq!(
            args.appearance_profile,
            Some(PathBuf::from("/tmp/my profile.json"))
        );
        assert!(parse(&[]).unwrap().appearance_profile.is_none());
    }

    #[test]
    fn missing_and_ambiguous_profile_paths_fail_closed() {
        for args in [
            vec!["--appearance-profile"],
            vec!["--appearance-profile", ""],
            vec!["--appearance-profile", "--validate"],
            vec!["--appearance-profile", "a", "--appearance-profile", "b"],
        ] {
            assert!(parse(&args).is_err());
        }
        assert!(parse(&["--input"]).is_err());
    }

    #[test]
    fn live_import_requires_an_explicit_file_and_does_not_change_default_mode() {
        assert!(parse(&["--watch-appearance-profile"]).is_err());
        assert!(parse(&["--appearance-profile", "a"]).is_ok_and(|a| !a.watch_appearance_profile));
        let args = parse(&[
            "--watch-appearance-profile",
            "--appearance-profile",
            "a",
            "--validate",
        ])
        .unwrap();
        assert!(args.watch_appearance_profile && args.validate);
        assert!(
            parse(&[
                "--watch-appearance-profile",
                "--appearance-profile",
                "a",
                "--watch-appearance-profile"
            ])
            .is_err()
        );
    }
}
