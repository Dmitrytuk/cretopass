use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use sysinfo::{DiskRefreshKind, Disks};

/// A mounted filesystem offered as a selectable location by the file browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskInfo {
    /// Directory the filesystem is mounted at (for example `/mnt/usb`).
    pub mount_point: PathBuf,
    /// Filesystem type reported by the OS (for example `ext4`).
    pub file_system: String,
    /// Total size of the filesystem in bytes.
    pub total_space: u64,
    /// Space currently available to the user in bytes.
    pub available_space: u64,
    /// Whether the OS reports the device as removable.
    pub is_removable: bool,
    /// Whether the filesystem is mounted read-only.
    pub is_read_only: bool,
}

/// Returns the mounted disks visible to the current user, sorted by mount point.
///
/// `sysinfo` already excludes pseudo filesystems, tmpfs, optical media,
/// network shares, and loop-backed `squashfs` mounts (snaps).  A small
/// additional filter drops any remaining loop-backed devices so the list
/// stays focused on user-facing storage.  Returns an empty vector when the
/// platform reports no disks.
pub fn list_disks() -> Vec<DiskInfo> {
    let disks = Disks::new_with_refreshed_list_specifics(DiskRefreshKind::nothing().with_storage());

    let mut entries: Vec<DiskInfo> = disks
        .list()
        .iter()
        .filter(|disk| is_user_facing_disk(disk.name(), disk.file_system(), disk.mount_point()))
        .map(|disk| DiskInfo {
            mount_point: disk.mount_point().to_path_buf(),
            file_system: disk.file_system().to_string_lossy().to_string(),
            total_space: disk.total_space(),
            available_space: disk.available_space(),
            is_removable: disk.is_removable(),
            is_read_only: disk.is_read_only(),
        })
        .collect();

    entries.sort_by(|a, b| a.mount_point.cmp(&b.mount_point));
    entries.dedup_by(|a, b| a.mount_point == b.mount_point);
    entries
}

/// Returns `false` for devices and mounts the file browser should not offer.
fn is_user_facing_disk(name: &OsStr, file_system: &OsStr, mount_point: &Path) -> bool {
    let name = name.to_string_lossy();
    if name.starts_with("/dev/loop") {
        return false;
    }

    let file_system = file_system.to_string_lossy().to_ascii_lowercase();
    if file_system == "squashfs" {
        return false;
    }

    !mount_point.starts_with("/snap") && !mount_point.starts_with("/var/lib/snapd/snap")
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn filter_rejects_loop_devices() {
        assert!(!is_user_facing_disk(
            OsStr::new("/dev/loop0"),
            OsStr::new("ext4"),
            Path::new("/mnt/data"),
        ));
    }

    #[test]
    fn filter_rejects_squashfs() {
        assert!(!is_user_facing_disk(
            OsStr::new("/dev/sdb1"),
            OsStr::new("squashfs"),
            Path::new("/mnt/data"),
        ));
    }

    #[test]
    fn filter_rejects_snap_mounts() {
        assert!(!is_user_facing_disk(
            OsStr::new("/dev/sdb1"),
            OsStr::new("ext4"),
            Path::new("/snap/core20/1974"),
        ));
    }

    #[test]
    fn filter_accepts_regular_mounts() {
        assert!(is_user_facing_disk(
            OsStr::new("/dev/nvme0n1p2"),
            OsStr::new("ext4"),
            Path::new("/"),
        ));
        assert!(is_user_facing_disk(
            OsStr::new("/dev/sdb1"),
            OsStr::new("vfat"),
            Path::new("/run/media/user/USB"),
        ));
    }

    #[test]
    fn list_disks_mount_points_are_unique_and_absolute() {
        let disks = list_disks();
        let mut seen: HashSet<PathBuf> = HashSet::new();
        for disk in &disks {
            assert!(
                disk.mount_point.is_absolute(),
                "mount point should be absolute: {:?}",
                disk.mount_point
            );
            assert!(
                seen.insert(disk.mount_point.clone()),
                "duplicate mount point: {:?}",
                disk.mount_point
            );
        }
    }
}
