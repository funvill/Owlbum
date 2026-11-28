//! Owlbum as the program that opens pictures, on the real registry
//! (docs/08-desktop-and-release.md, "Opening pictures"). What is written is
//! [`owlbum_core::app::assoc`]'s; this is where it is written.

use std::path::PathBuf;
use std::sync::Arc;

use owlbum_core::app::assoc::Registry;

/// The registry and the program file that `app_associate` uses, managed as
/// state. `run` gives the current user's hive on Windows; shell tests give
/// a map. Without it (Linux, for now) the setting is not offered.
#[derive(Clone)]
pub struct FileTypes {
    pub registry: Arc<dyn Registry>,
    /// The program a picture is opened with: this copy's own file.
    pub exe: PathBuf,
}

/// `HKEY_CURRENT_USER`. Nothing here needs administrator rights.
#[cfg(windows)]
pub struct CurrentUser;

/// ERROR_FILE_NOT_FOUND, as an HRESULT: the key or value is not there.
#[cfg(windows)]
const NOT_FOUND: i32 = 0x8007_0002_u32 as i32;

#[cfg(windows)]
impl Registry for CurrentUser {
    fn set(&self, key: &str, name: &str, value: &str) -> Result<(), String> {
        windows_registry::CURRENT_USER
            .create(key)
            .and_then(|k| k.set_string(name, value))
            .map_err(|e| format!("{key} {name}: {e}"))
    }

    fn get(&self, key: &str, name: &str) -> Option<String> {
        windows_registry::CURRENT_USER
            .open(key)
            .and_then(|k| k.get_string(name))
            .ok()
    }

    fn delete_value(&self, key: &str, name: &str) -> Result<(), String> {
        let k = match windows_registry::CURRENT_USER
            .options()
            .read()
            .write()
            .open(key)
        {
            Ok(k) => k,
            Err(e) if e.code().0 == NOT_FOUND => return Ok(()),
            Err(e) => return Err(format!("{key}: {e}")),
        };
        match k.remove_value(name) {
            Err(e) if e.code().0 != NOT_FOUND => Err(format!("{key} {name}: {e}")),
            _ => Ok(()),
        }
    }

    fn delete_tree(&self, key: &str) -> Result<(), String> {
        match windows_registry::CURRENT_USER.remove_tree(key) {
            Err(e) if e.code().0 != NOT_FOUND => Err(format!("{key}: {e}")),
            _ => Ok(()),
        }
    }

    #[allow(unsafe_code)]
    fn changed(&self) {
        use windows_sys::Win32::UI::Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify};
        // Explorer reads the file types again.
        // SAFETY: no item pointers are passed with SHCNE_ASSOCCHANGED.
        unsafe {
            SHChangeNotify(
                SHCNE_ASSOCCHANGED as i32,
                SHCNF_IDLIST,
                std::ptr::null(),
                std::ptr::null(),
            );
        }
    }
}
