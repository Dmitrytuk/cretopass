//! The directory listing behind the browser and the Locations overlay rows.

use std::path::{Path, PathBuf};

use crate::file_handling::{self, FileEntry, FileHandlingError};
use crate::theming::texts;

use super::types::{FileBrowserSession, Location};

/// Lists `path` honouring the hidden-entry setting.
pub fn list_entries(path: &Path, show_hidden: bool) -> Result<Vec<FileEntry>, FileHandlingError> {
    if show_hidden {
        file_handling::list_dir_including_hidden(path)
    } else {
        file_handling::list_dir(path)
    }
}

/// Picks the initial directory and its listing for a new browser.
pub(super) fn initial_location(session: &FileBrowserSession) -> (PathBuf, Vec<FileEntry>) {
    if let Some(dir) = session.last_dir.as_deref()
        && let Ok(entries) = list_entries(dir, session.show_hidden)
    {
        return (dir.to_path_buf(), entries);
    }

    let home = dirs_home();
    let entries = list_entries(&home, session.show_hidden).unwrap_or_default();
    (home, entries)
}

/// Builds the Locations overlay rows: Home first, then mounted disks.
pub fn build_locations() -> Vec<Location> {
    let home = dirs_home();
    let mut locations = Vec::new();
    locations.push(Location {
        label: texts::LOCATION_HOME.to_string(),
        detail: home.to_string_lossy().to_string(),
        path: home,
        is_home: true,
    });

    for disk in file_handling::list_disks() {
        locations.push(Location {
            label: disk.mount_point.to_string_lossy().to_string(),
            detail: format_disk_detail(&disk),
            path: disk.mount_point,
            is_home: false,
        });
    }

    locations
}

/// Selects the most specific location containing `current_dir` (Home wins
/// when the directory is inside the home tree).  Falls back to the first row.
pub fn preselect_location(locations: &[Location], current_dir: &Path) -> usize {
    locations
        .iter()
        .enumerate()
        .filter(|(_, location)| current_dir.starts_with(&location.path))
        .max_by_key(|(_, location)| location.path.components().count())
        .map(|(index, _)| index)
        .unwrap_or(0)
}

/// Formats the secondary information for a disk row.
fn format_disk_detail(disk: &file_handling::DiskInfo) -> String {
    let mut detail = disk.file_system.clone();
    if disk.total_space > 0 {
        detail.push_str(&format!(
            " · {} free / {}",
            format_size(disk.available_space),
            format_size(disk.total_space)
        ));
    }
    if disk.is_removable {
        detail.push(' ');
        detail.push_str(texts::LOCATION_USB);
    }
    if disk.is_read_only {
        detail.push(' ');
        detail.push_str(texts::LOCATION_READ_ONLY);
    }
    detail
}

/// Formats a byte count using binary units and one decimal place.
fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];

    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Returns the user's home directory, or the current directory as fallback.
pub(super) fn dirs_home() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home)
    } else if cfg!(target_os = "windows") {
        if let Ok(home) = std::env::var("USERPROFILE") {
            PathBuf::from(home)
        } else {
            PathBuf::from(".")
        }
    } else {
        PathBuf::from(".")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disk(mount_point: &str, file_system: &str) -> file_handling::DiskInfo {
        file_handling::DiskInfo {
            mount_point: PathBuf::from(mount_point),
            file_system: file_system.to_string(),
            total_space: 500 * 1024 * 1024 * 1024,
            available_space: 120 * 1024 * 1024 * 1024,
            is_removable: false,
            is_read_only: false,
        }
    }
    #[test]
    fn preselect_prefers_most_specific_location() {
        let locations = vec![
            Location {
                label: "/".to_string(),
                path: PathBuf::from("/"),
                detail: String::new(),
                is_home: false,
            },
            Location {
                label: "Home".to_string(),
                path: PathBuf::from("/home/user"),
                detail: String::new(),
                is_home: true,
            },
        ];

        let selected = preselect_location(&locations, Path::new("/home/user/docs"));

        assert_eq!(selected, 1);
    }

    #[test]
    fn preselect_falls_back_to_first_location() {
        let locations = vec![Location {
            label: "Home".to_string(),
            path: PathBuf::from("/home/user"),
            detail: String::new(),
            is_home: true,
        }];

        let selected = preselect_location(&locations, Path::new("/mnt/other"));

        assert_eq!(selected, 0);
    }

    #[test]
    fn build_locations_pins_home_first() {
        let locations = build_locations();

        assert!(locations[0].is_home);
        assert_eq!(locations[0].label, texts::LOCATION_HOME);
    }

    #[test]
    fn format_size_uses_binary_units() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.0 KiB");
        assert_eq!(format_size(1024 * 1024), "1.0 MiB");
    }

    #[test]
    fn format_disk_detail_includes_tags() {
        let mut entry = disk("/mnt/usb", "vfat");
        entry.is_removable = true;
        entry.is_read_only = true;

        let detail = format_disk_detail(&entry);

        assert!(detail.contains("vfat"));
        assert!(detail.contains(texts::LOCATION_USB));
        assert!(detail.contains(texts::LOCATION_READ_ONLY));
    }

    #[test]
    fn format_disk_detail_omits_unknown_size() {
        let mut entry = disk("/mnt/net", "nfs");
        entry.total_space = 0;
        entry.available_space = 0;

        let detail = format_disk_detail(&entry);

        assert_eq!(detail, "nfs");
    }
}
