//! What the File Browser was opened for, and what a selection therefore means.

use crate::file_handling::export::ExportFormat;

/// Why the file browser was opened, which decides what a selection means.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileBrowserMode {
    OpenFile,
    CreateFile,
    ImportFile,
    /// Naming the plain-text file an export is written to, in `format`.
    ExportFile {
        format: ExportFormat,
    },
    /// Naming the folder a share bundle is written into.
    ShareDestination,
}

impl FileBrowserMode {
    /// The export format this mode is naming a file for, if any.
    pub fn export_format(self) -> Option<ExportFormat> {
        match self {
            Self::ExportFile { format } => Some(format),
            _ => None,
        }
    }

    /// Whether this mode names a destination the user types — the export file or
    /// the share bundle folder — rather than picking an existing entry.
    pub fn names_destination(self) -> bool {
        self.export_format().is_some() || self.is_share_destination()
    }

    /// Whether this mode names the folder a share bundle is written into.
    pub fn is_share_destination(self) -> bool {
        matches!(self, Self::ShareDestination)
    }

    /// Whether the browser may rename or delete the secret files it lists.
    ///
    /// Import, export and share only read the filesystem through the browser:
    /// editing a vault is not what the user opened it for.
    pub(crate) fn allows_entry_actions(self) -> bool {
        !matches!(
            self,
            Self::ImportFile | Self::ExportFile { .. } | Self::ShareDestination
        )
    }
}
