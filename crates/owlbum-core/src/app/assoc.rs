//! Owlbum as the program that opens pictures (docs/08-desktop-and-release.md,
//! "Opening pictures"; owner's feedback, issue 184).
//!
//! On Windows, Settings registers Owlbum for the current user, with no
//! administrator rights: a ProgID that opens a picture with Owlbum, an
//! "Open with" entry for each picture extension of the media registry, and
//! the registered-application entries that put Owlbum in Windows' Default
//! apps page. Windows 10 and 11 do not let a program make itself the
//! default silently, so the user confirms the choice on that page.
//! Turning the setting off removes every entry again.
//!
//! What is written is worked out here, against [`Registry`], the current
//! user's registry hive. The shell gives the real one on Windows; tests
//! give a map, so no test touches the real registry.

use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::media::registry::picture_extensions;

/// The current user's registry hive (`HKEY_CURRENT_USER`), as far as
/// associating pictures needs it. Keys are paths under the hive with `\`
/// between their parts; the value name `""` is a key's default value.
/// Each call answers why it failed, for the log.
pub trait Registry: Send + Sync {
    /// Writes a string value, making the key and its parents as needed.
    fn set(&self, key: &str, name: &str, value: &str) -> Result<(), String>;
    /// A string value, or none when the key or value is not there.
    fn get(&self, key: &str, name: &str) -> Option<String>;
    /// Removes a value. A value or key that is not there is not an error.
    fn delete_value(&self, key: &str, name: &str) -> Result<(), String>;
    /// Removes a key and everything under it. A key that is not there is
    /// not an error.
    fn delete_tree(&self, key: &str) -> Result<(), String>;
    /// Tells the file manager that file types changed, so it reads them
    /// again.
    fn changed(&self) {}
}

/// The ProgID a picture opened with Owlbum has.
pub const PROG_ID: &str = "Owlbum.Picture";
/// The name Owlbum is registered under in Default apps.
pub const APP_NAME: &str = "Owlbum";
/// Under the hive: where file types and ProgIDs live for this user.
const CLASSES: &str = r"Software\Classes";
/// Under the hive: Owlbum's capabilities, which Default apps reads.
pub const CAPABILITIES: &str = r"Software\Owlbum\Capabilities";
/// Under the hive: the list of programs Default apps offers.
const REGISTERED: &str = r"Software\RegisteredApplications";
/// Windows' Default apps page, where the user confirms the choice.
pub const DEFAULT_APPS: &str = "ms-settings:defaultapps";

/// The answer of `app_associate_status` and `app_associate`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Association {
    /// This platform can do it: Windows only for now; on Linux a
    /// `.desktop` entry's `MimeType` will do it later (Linux is paused).
    pub available: bool,
    /// Owlbum is registered to open pictures, for this copy of the program.
    pub on: bool,
}

/// The command a picture is opened with: the program and the file.
pub fn open_command(exe: &Path) -> String {
    format!("\"{}\" \"%1\"", exe.display())
}

fn prog_id_key() -> String {
    format!(r"{CLASSES}\{PROG_ID}")
}

fn application_key(exe: &Path) -> Option<String> {
    let name = exe.file_name()?.to_str()?;
    Some(format!(r"{CLASSES}\Applications\{name}"))
}

/// Whether Owlbum is registered to open pictures with the program at `exe`.
/// Registered for another copy (a development build, an older install
/// place) counts as off, so turning it on points it at this one.
pub fn is_registered(reg: &dyn Registry, exe: &Path) -> bool {
    reg.get(&format!(r"{}\shell\open\command", prog_id_key()), "")
        .is_some_and(|c| c == open_command(exe))
}

/// Registers the program at `exe` to open every picture extension of the
/// media registry, for the current user. It does not make it the default:
/// the user does that on Windows' Default apps page.
///
/// # Errors
///
/// The first write that failed, with why. What was written before it stays;
/// [`unregister`] removes it.
pub fn register(reg: &dyn Registry, exe: &Path) -> Result<(), String> {
    let command = open_command(exe);
    let icon = format!("\"{}\",0", exe.display());
    let prog = prog_id_key();
    reg.set(&prog, "", crate::text::appdata::PICTURE_TYPE)?;
    reg.set(&format!(r"{prog}\DefaultIcon"), "", &icon)?;
    reg.set(&format!(r"{prog}\shell\open\command"), "", &command)?;
    let app = application_key(exe);
    if let Some(app) = &app {
        reg.set(&format!(r"{app}\shell\open\command"), "", &command)?;
        reg.set(app, "FriendlyAppName", APP_NAME)?;
    }
    reg.set(CAPABILITIES, "ApplicationName", APP_NAME)?;
    reg.set(
        CAPABILITIES,
        "ApplicationDescription",
        crate::text::appdata::APP_DESCRIPTION,
    )?;
    for ext in picture_extensions() {
        reg.set(&format!(r"{CLASSES}\{ext}\OpenWithProgids"), PROG_ID, "")?;
        reg.set(&format!(r"{CAPABILITIES}\FileAssociations"), ext, PROG_ID)?;
        if let Some(app) = &app {
            reg.set(&format!(r"{app}\SupportedTypes"), ext, "")?;
        }
    }
    reg.set(REGISTERED, APP_NAME, CAPABILITIES)?;
    reg.changed();
    Ok(())
}

/// Removes everything [`register`] wrote. The extensions' own keys stay,
/// with whatever else is in them; only Owlbum's entries go. A default the
/// user chose for Owlbum falls back to Windows' choice.
///
/// # Errors
///
/// The first removal that failed, with why.
pub fn unregister(reg: &dyn Registry, exe: &Path) -> Result<(), String> {
    reg.delete_value(REGISTERED, APP_NAME)?;
    for ext in picture_extensions() {
        reg.delete_value(&format!(r"{CLASSES}\{ext}\OpenWithProgids"), PROG_ID)?;
    }
    reg.delete_tree(CAPABILITIES)?;
    if let Some(app) = application_key(exe) {
        reg.delete_tree(&app)?;
    }
    reg.delete_tree(&prog_id_key())?;
    reg.changed();
    Ok(())
}

/// The installer's no-window modes (issue 193): the NSIS installer runs
/// `owlbum --associate-pictures` when its unticked-by-default checkbox
/// was ticked, and the uninstaller runs `owlbum --forget-pictures`, so the
/// entries are written and removed by the same code as Settings'.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallerMode {
    /// [`register`], then exit.
    Associate,
    /// [`unregister`], then exit.
    Forget,
}

/// The installer's argument for [`InstallerMode::Associate`].
pub const ASSOCIATE_ARG: &str = "--associate-pictures";
/// The uninstaller's argument for [`InstallerMode::Forget`].
pub const FORGET_ARG: &str = "--forget-pictures";

/// The installer mode a command line asks for: [`ASSOCIATE_ARG`] or
/// [`FORGET_ARG`] as the first argument after the program's own name.
/// Anything else starts the program as usual.
pub fn installer_mode<I, S>(args: I) -> Option<InstallerMode>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let first = args.into_iter().nth(1)?;
    match first.as_ref().to_str()? {
        ASSOCIATE_ARG => Some(InstallerMode::Associate),
        FORGET_ARG => Some(InstallerMode::Forget),
        _ => None,
    }
}

/// Runs an installer mode for the program at `exe`.
///
/// # Errors
///
/// As [`register`] and [`unregister`].
pub fn run_installer_mode(
    mode: InstallerMode,
    reg: &dyn Registry,
    exe: &Path,
) -> Result<(), String> {
    match mode {
        InstallerMode::Associate => register(reg, exe),
        InstallerMode::Forget => unregister(reg, exe),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    /// A registry hive in memory: key to value name to value.
    #[derive(Default)]
    pub struct FakeRegistry {
        pub keys: Mutex<BTreeMap<String, BTreeMap<String, String>>>,
    }

    impl Registry for FakeRegistry {
        fn set(&self, key: &str, name: &str, value: &str) -> Result<(), String> {
            let mut keys = self.keys.lock().unwrap();
            keys.entry(key.to_owned())
                .or_default()
                .insert(name.to_owned(), value.to_owned());
            Ok(())
        }
        fn get(&self, key: &str, name: &str) -> Option<String> {
            self.keys.lock().unwrap().get(key)?.get(name).cloned()
        }
        fn delete_value(&self, key: &str, name: &str) -> Result<(), String> {
            if let Some(values) = self.keys.lock().unwrap().get_mut(key) {
                values.remove(name);
            }
            Ok(())
        }
        fn delete_tree(&self, key: &str) -> Result<(), String> {
            let below = format!(r"{key}\");
            self.keys
                .lock()
                .unwrap()
                .retain(|k, _| k != key && !k.starts_with(&below));
            Ok(())
        }
    }

    fn exe() -> &'static Path {
        Path::new(r"C:\Users\me\AppData\Local\Owlbum\owlbum.exe")
    }

    #[test]
    fn registering_offers_owlbum_for_every_picture_type_and_names_it_in_default_apps() {
        let reg = FakeRegistry::default();
        assert!(!is_registered(&reg, exe()));
        register(&reg, exe()).unwrap();
        assert!(is_registered(&reg, exe()));
        let command = r#""C:\Users\me\AppData\Local\Owlbum\owlbum.exe" "%1""#;
        assert_eq!(
            reg.get(r"Software\Classes\Owlbum.Picture\shell\open\command", "")
                .as_deref(),
            Some(command)
        );
        assert_eq!(
            reg.get(r"Software\RegisteredApplications", "Owlbum")
                .as_deref(),
            Some(r"Software\Owlbum\Capabilities")
        );
        let exts = picture_extensions();
        assert!(exts.contains(&".jpg"));
        for ext in &exts {
            assert_eq!(
                reg.get(&format!(r"Software\Classes\{ext}\OpenWithProgids"), PROG_ID)
                    .as_deref(),
                Some(""),
                "{ext} offers Owlbum in Open with"
            );
            assert_eq!(
                reg.get(r"Software\Owlbum\Capabilities\FileAssociations", ext)
                    .as_deref(),
                Some(PROG_ID)
            );
        }
        // No video type is taken.
        for format in crate::media::registry::FORMATS {
            if format.kind == crate::media::MediaKind::Video {
                for ext in format.extensions {
                    assert!(!exts.contains(ext), "{ext}");
                }
            }
        }
    }

    #[test]
    fn registered_for_another_copy_counts_as_off() {
        let reg = FakeRegistry::default();
        register(&reg, Path::new(r"C:\dev\owlbum.exe")).unwrap();
        assert!(!is_registered(&reg, exe()));
    }

    #[test]
    fn unregistering_removes_only_owlbums_entries() {
        let reg = FakeRegistry::default();
        // Another program's entry under a picture type, and the type's own default.
        reg.set(r"Software\Classes\.jpg\OpenWithProgids", "Other.Viewer", "")
            .unwrap();
        reg.set(r"Software\Classes\.jpg", "", "Other.Viewer")
            .unwrap();
        reg.set(
            r"Software\RegisteredApplications",
            "Other",
            r"Software\Other",
        )
        .unwrap();
        register(&reg, exe()).unwrap();
        unregister(&reg, exe()).unwrap();
        assert!(!is_registered(&reg, exe()));
        let keys = reg.keys.lock().unwrap();
        let left: Vec<String> = keys
            .iter()
            .flat_map(|(k, v)| v.keys().map(move |n| format!("{k}|{n}")))
            .collect();
        assert_eq!(
            left,
            [
                r"Software\Classes\.jpg|",
                r"Software\Classes\.jpg\OpenWithProgids|Other.Viewer",
                r"Software\RegisteredApplications|Other",
            ]
        );
    }

    #[test]
    fn unregistering_when_nothing_was_registered_is_fine() {
        let reg = FakeRegistry::default();
        unregister(&reg, exe()).unwrap();
        assert!(reg.keys.lock().unwrap().is_empty());
    }

    #[test]
    fn the_installer_modes_are_read_from_the_first_argument_only() {
        let mode = |args: &[&str]| installer_mode(args.iter().copied());
        assert_eq!(
            mode(&["owlbum.exe", "--associate-pictures"]),
            Some(InstallerMode::Associate)
        );
        assert_eq!(
            mode(&["owlbum.exe", "--forget-pictures"]),
            Some(InstallerMode::Forget)
        );
        assert_eq!(mode(&["owlbum.exe"]), None);
        assert_eq!(mode(&["owlbum.exe", "a.jpg"]), None);
        // A picture named like the flag, after the program's first
        // argument, still opens as a picture.
        assert_eq!(mode(&["owlbum.exe", "x.jpg", "--forget-pictures"]), None);
        assert_eq!(mode(&["owlbum.exe", "--associate"]), None);
    }

    #[test]
    fn the_installer_modes_register_and_unregister() {
        let reg = FakeRegistry::default();
        run_installer_mode(InstallerMode::Associate, &reg, exe()).unwrap();
        assert!(is_registered(&reg, exe()));
        run_installer_mode(InstallerMode::Forget, &reg, exe()).unwrap();
        assert!(!is_registered(&reg, exe()));
        assert!(reg.keys.lock().unwrap().values().all(BTreeMap::is_empty));
    }
}
