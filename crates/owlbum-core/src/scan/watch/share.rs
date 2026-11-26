//! Network shares, where watching cannot be trusted
//! (docs/03-media-and-scanning.md, "When watching cannot be trusted").
//!
//! Windows: a UNC path, or a drive letter of type remote. Linux: the mount
//! that holds the folder has a network file system.

use std::path::Path;

/// File systems that are reached over a network (Linux mount types).
pub const NETWORK_FILE_SYSTEMS: [&str; 7] =
    ["nfs", "nfs4", "cifs", "smb3", "fuse.sshfs", "davfs", "9p"];

/// True when `folder` is on a network share, so no watch is set.
pub fn is_network_share(folder: &Path) -> bool {
    #[cfg(windows)]
    {
        windows::is_remote(folder)
    }
    #[cfg(not(windows))]
    {
        let folder = std::fs::canonicalize(folder).unwrap_or_else(|_| folder.to_path_buf());
        std::fs::read_to_string("/proc/self/mountinfo")
            .ok()
            .and_then(|info| file_system_of(&info, &folder.to_string_lossy()).map(str::to_owned))
            .is_some_and(|fs| NETWORK_FILE_SYSTEMS.contains(&fs.as_str()))
    }
}

/// The file system type of the mount that holds `path`, from the text of
/// `/proc/self/mountinfo`: the longest mount point that `path` lies in.
pub fn file_system_of<'i>(mountinfo: &'i str, path: &str) -> Option<&'i str> {
    let mut best: Option<(usize, &str)> = None;
    for line in mountinfo.lines() {
        // `36 35 98:0 /mnt1 /mnt/parent rw,noatime master:1 - ext3 /dev/root rw`:
        // the mount point is the fifth field; the type follows the " - ".
        let Some((left, right)) = line.split_once(" - ") else {
            continue;
        };
        let (Some(point), Some(fs)) = (left.split(' ').nth(4), right.split(' ').next()) else {
            continue;
        };
        let point = unescape(point);
        let inside = point == "/"
            || path == point
            || (path.starts_with(&point) && path.as_bytes().get(point.len()) == Some(&b'/'));
        if inside && best.is_none_or(|(len, _)| point.len() >= len) {
            best = Some((point.len(), fs));
        }
    }
    best.map(|(_, fs)| fs)
}

/// Mount points in `mountinfo` write a space, tab, newline and backslash as
/// octal escapes (`\040`).
fn unescape(field: &str) -> String {
    let mut out = String::with_capacity(field.len());
    let mut rest = field;
    while let Some(i) = rest.find('\\') {
        out.push_str(&rest[..i]);
        let code = rest
            .get(i + 1..i + 4)
            .and_then(|o| u8::from_str_radix(o, 8).ok());
        match code {
            Some(c) => {
                out.push(char::from(c));
                rest = &rest[i + 4..];
            }
            None => {
                out.push('\\');
                rest = &rest[i + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(windows)]
mod windows {
    use std::path::{Component, Path, Prefix};

    /// `GetDriveTypeW` answers this for a network drive.
    const DRIVE_REMOTE: u32 = 4;

    pub fn is_remote(folder: &Path) -> bool {
        match folder.components().next() {
            Some(Component::Prefix(p)) => match p.kind() {
                Prefix::UNC(..) | Prefix::VerbatimUNC(..) => true,
                Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => drive_is_remote(letter),
                _ => false,
            },
            _ => false,
        }
    }

    #[allow(unsafe_code)]
    fn drive_is_remote(letter: u8) -> bool {
        use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
        let root: Vec<u16> = format!("{}:\\", char::from(letter))
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: `root` is a NUL-terminated wide string that outlives the
        // call; nothing is kept.
        unsafe { GetDriveTypeW(root.as_ptr()) == DRIVE_REMOTE }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOUNTINFO: &str = "\
22 1 8:2 / / rw,relatime shared:1 - ext4 /dev/sda2 rw
25 22 0:22 / /proc rw,nosuid - proc proc rw
40 22 0:35 / /mnt/photos rw,relatime shared:20 - nfs4 server:/export rw,vers=4.2
41 40 8:3 / /mnt/photos/local rw,relatime - ext4 /dev/sdb1 rw
42 22 0:36 / /mnt/c rw,noatime - 9p drvfs rw,dirsync
43 22 0:37 / /media/me/My\\040Share rw - cifs //nas/share rw
44 22 0:38 / /home/me/remote rw - fuse.sshfs me@host:/ rw
";

    #[test]
    fn the_longest_mount_point_holding_the_path_gives_its_type() {
        let cases = [
            ("/home/me/Pictures", Some("ext4")),
            ("/mnt/photos", Some("nfs4")),
            ("/mnt/photos/2024/trip", Some("nfs4")),
            ("/mnt/photos/local/a", Some("ext4")),
            ("/mnt/photosx", Some("ext4")),
            ("/mnt/c/Users/me", Some("9p")),
            ("/media/me/My Share/2024", Some("cifs")),
            ("/home/me/remote/x", Some("fuse.sshfs")),
        ];
        for (path, fs) in cases {
            assert_eq!(file_system_of(MOUNTINFO, path), fs, "{path}");
        }
        assert_eq!(file_system_of("", "/x"), None);
        assert_eq!(file_system_of("garbage line\n", "/x"), None);
    }

    #[test]
    fn network_types_are_the_documented_ones() {
        for fs in ["nfs", "nfs4", "cifs", "smb3", "fuse.sshfs", "davfs", "9p"] {
            assert!(NETWORK_FILE_SYSTEMS.contains(&fs));
        }
        assert!(!NETWORK_FILE_SYSTEMS.contains(&"ext4"));
    }

    #[test]
    fn a_local_temporary_folder_is_not_a_share() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(!is_network_share(tmp.path()));
    }

    #[cfg(windows)]
    #[test]
    fn a_unc_path_is_a_share() {
        assert!(is_network_share(Path::new(r"\\server\photos\2024")));
        assert!(is_network_share(Path::new(r"\\?\UNC\server\photos")));
        assert!(!is_network_share(Path::new(r"C:\Users")));
    }
}
