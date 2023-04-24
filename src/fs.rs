use anyhow::{anyhow, Context, Result};
use log::debug;

use std::fs::{copy, metadata, remove_file, rename, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

pub use std::fs::read;

use crate::path::file_name_display;

/// Retrieves the file mode/permissions of a file
pub fn get_file_mode(path: &Path) -> Result<u32> {
    let meta = metadata(path)?;
    let perm = meta.permissions();
    Ok(perm.mode())
}

/// Creates and writes and a file
///
/// Invokes the `unclean` version of the function and removes any left over file content
/// in case of failure
pub fn safe_create_file(path: &Path, mode: Option<u32>, content: &[u8]) -> Result<()> {
    if let Err(e) = safe_create_file_unclean(path, mode, content) {
        safe_remove_file(path).ok();
        return Err(e);
    }

    Ok(())
}

/// Creates and writes and a file (unclean)
///
/// Both the file and its parent dir are fsynced.
///
/// The `unclean` version of the function may leave incomplete left over file in case
/// of failure.
fn safe_create_file_unclean(path: &Path, mode: Option<u32>, content: &[u8]) -> Result<()> {
    let mut file = open_with_mode(path, mode)?;

    file.write_all(content)?;
    file.sync_all()?;

    debug!("Created {}", file_name_display(path));

    fsync_parent_dir(path)?;

    Ok(())
}

/// Copies a file
///
/// Invokes the `unclean` version of the function and removes any left over file content
/// in case of failure
pub fn safe_copy(from: &Path, to: &Path) -> Result<()> {
    match safe_copy_unclean(from, to) {
        Ok(_) => Ok(()),
        Err(err) => {
            safe_remove_file(to).ok();
            Err(err).context("Failed to copy file")
        }
    }
}

/// Copies a file (unclean)
///
/// Both the file and its parent dir are fsynced.
///
/// The `unclean` version of the function may leave incomplete left over file in case
/// of failure.
fn safe_copy_unclean(from: &Path, to: &Path) -> Result<u64> {
    let bytes = copy(from, to)?;

    fsync_file_and_parent_dir(to)?;

    Ok(bytes)
}

/// Removes a file
///
/// The parent dir is fsynced.
pub fn safe_remove_file(path: &Path) -> Result<()> {
    remove_file(path)?;

    fsync_parent_dir(path)?;

    Ok(())
}

/// Renames a file
///
/// Both the file and its parent dir are fsynced.
pub fn safe_rename(from: &Path, to: &Path) -> Result<()> {
    rename(from, to)?;

    fsync_file_and_parent_dir(to)?;

    Ok(())
}

/// Fsyncs a file and its parent directory
fn fsync_file_and_parent_dir(path: &Path) -> Result<()> {
    fsync_path(path)?;
    fsync_parent_dir(path)?;

    Ok(())
}

/// Fsyncs the parent directory of a file
fn fsync_parent_dir(path: &Path) -> Result<()> {
    let parent_dir = path
        .parent()
        .ok_or_else(|| anyhow!("Cannot evaluate the parent directory of {}", path.display()))?;

    fsync_path(parent_dir)
}

/// Fsyncs a path that can be a file or directory
fn fsync_path(path: &Path) -> Result<()> {
    let f = File::open(path).context(format!("Failed to open path {}", path.display()))?;
    f.sync_all()
        .context(format!("Failed to sync path {}", path.display()))?;

    debug!("Fsynced {}", path.display());

    Ok(())
}

/// Opens a file specifying file mode
fn open_with_mode(path: &Path, mode: Option<u32>) -> Result<File> {
    let mut open_options = OpenOptions::new();

    open_options.create(true).write(true);

    if let Some(octal_mode) = mode {
        open_options.mode(octal_mode);
    }

    Ok(open_options.open(path)?)
}

/// Checks whether a passed Error is an out-of-space error
///
/// This is `No space left on device (os error 28)` error.
pub fn is_storage_full_error(err: &anyhow::Error) -> bool {
    // TODO: Use io::ErrorKind::StorageFull when stabilized
    if let Some(e) = err.downcast_ref::<std::io::Error>() {
        e.raw_os_error() == Some(28_i32)
    } else {
        false
    }
}
