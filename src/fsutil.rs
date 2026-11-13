//! Filesystem identity helpers shared by the CLI and the project build.
//!
//! Paths can reach the same file through relative segments, symlinks, or hard
//! links. The build and `compose --out` both need to answer "is this output
//! actually one of my inputs?" before truncating anything, so the comparison
//! lives in one place.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

/// Returns `true` when two open handles refer to the same file.
#[cfg(unix)]
pub(crate) fn same_file(input: &fs::File, output: &fs::File) -> io::Result<bool> {
    use std::os::unix::fs::MetadataExt;

    let input = input.metadata()?;
    let output = output.metadata()?;
    Ok(input.dev() == output.dev() && input.ino() == output.ino())
}

/// Returns `true` when two open handles refer to the same file.
///
/// `FileIdInfo` carries a 128-bit identifier on modern file systems; older
/// ones only support `GetFileInformationByHandle`, so that is the fallback.
#[cfg(windows)]
pub(crate) fn same_file(input: &fs::File, output: &fs::File) -> io::Result<bool> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::{
        ERROR_INVALID_FUNCTION, ERROR_INVALID_PARAMETER, ERROR_NOT_SUPPORTED,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        FileIdInfo, GetFileInformationByHandle, GetFileInformationByHandleEx,
        BY_HANDLE_FILE_INFORMATION, FILE_ID_INFO,
    };

    fn identity(file: &fs::File) -> io::Result<(u64, [u8; 16])> {
        let mut info = FILE_ID_INFO::default();
        // SAFETY: The file owns the handle, and FileIdInfo writes a FILE_ID_INFO
        // into the correctly sized and aligned buffer supplied here.
        let success = unsafe {
            GetFileInformationByHandleEx(
                file.as_raw_handle(),
                FileIdInfo,
                (&mut info as *mut FILE_ID_INFO).cast(),
                std::mem::size_of::<FILE_ID_INFO>() as u32,
            )
        };
        if success == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok((info.VolumeSerialNumber, info.FileId.Identifier))
    }

    fn legacy_identity(file: &fs::File) -> io::Result<(u32, u32, u32)> {
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: The file owns the handle and the output buffer matches the API.
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok((
            info.dwVolumeSerialNumber,
            info.nFileIndexHigh,
            info.nFileIndexLow,
        ))
    }

    match identity(input).and_then(|id| identity(output).map(|other| id == other)) {
        Err(e)
            if matches!(
                e.raw_os_error().map(|code| code as u32),
                Some(ERROR_INVALID_FUNCTION | ERROR_INVALID_PARAMETER | ERROR_NOT_SUPPORTED)
            ) =>
        {
            // Some file systems support only the older file identity API.
            Ok(legacy_identity(input)? == legacy_identity(output)?)
        }
        result => result,
    }
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn same_file(_input: &fs::File, _output: &fs::File) -> io::Result<bool> {
    Ok(false)
}

/// Resolve a path as far as the filesystem allows.
///
/// Existing paths are canonicalized. For a path that does not exist yet, the
/// nearest existing ancestor is canonicalized and the remaining components are
/// appended, so `dist/../x` and `x` compare equal before either exists.
pub(crate) fn canonical_anchor(path: &Path) -> io::Result<PathBuf> {
    canonical_anchor_inner(path, 0)
}

fn validate_symlink_traversal(path: &Path) -> io::Result<()> {
    let mut traversed = PathBuf::new();

    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => traversed.push(component.as_os_str()),
            Component::CurDir => {}
            Component::Normal(name) => traversed.push(name),
            Component::ParentDir => {
                let path_to_check = if traversed.as_os_str().is_empty() {
                    Path::new(".")
                } else {
                    &traversed
                };
                let resolved = fs::canonicalize(path_to_check).map_err(|error| {
                    io::Error::new(
                        error.kind(),
                        format!(
                            "cannot resolve '{}' before '..': {}",
                            path_to_check.display(),
                            error
                        ),
                    )
                })?;
                if !fs::metadata(&resolved)?.is_dir() {
                    return Err(io::Error::new(
                        io::ErrorKind::NotADirectory,
                        format!(
                            "cannot traverse '..' after non-directory '{}'",
                            resolved.display()
                        ),
                    ));
                }
                traversed = fs::canonicalize(resolved.join(".."))?;
            }
        }
    }

    Ok(())
}

fn canonical_anchor_inner(path: &Path, symlink_depth: usize) -> io::Result<PathBuf> {
    if let Ok(canonical) = fs::canonicalize(path) {
        return Ok(canonical);
    }

    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() {
            if symlink_depth >= 40 {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "too many symbolic links while resolving path",
                ));
            }

            let target = fs::read_link(path)?;
            let target = if target.is_absolute() {
                target
            } else {
                let parent = path
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                    .unwrap_or_else(|| Path::new("."));
                parent.join(target)
            };
            validate_symlink_traversal(&target)?;
            return canonical_anchor_inner(&target, symlink_depth + 1);
        }
    }

    match path.file_name() {
        Some(name) => {
            let parent = match path.parent() {
                Some(parent) if !parent.as_os_str().is_empty() => parent,
                _ => Path::new("."),
            };
            Ok(canonical_anchor_inner(parent, symlink_depth)?.join(name))
        }
        None => {
            // The path ends in `..` (or is a bare root), so it has no file
            // name. Resolve its parent and step up one directory.
            let parent = path.parent().unwrap_or_else(|| Path::new("."));
            let mut anchor = canonical_anchor_inner(parent, symlink_depth)?;
            if path
                .components()
                .next_back()
                .is_some_and(|component| component == Component::ParentDir)
            {
                anchor.pop();
            }
            Ok(anchor)
        }
    }
}

/// Returns `true` when two paths reach the same file.
///
/// Existing files are compared by handle identity (catching hard links) and
/// then by canonical path. Otherwise canonical anchors are compared.
pub(crate) fn paths_alias(a: &Path, b: &Path) -> io::Result<bool> {
    let (Ok(a_meta), Ok(b_meta)) = (fs::metadata(a), fs::metadata(b)) else {
        return Ok(canonical_anchor(a)? == canonical_anchor(b)?);
    };

    if a_meta.is_file() && b_meta.is_file() {
        let a_file = fs::File::open(a)?;
        let b_file = fs::File::open(b)?;
        if same_file(&a_file, &b_file)? {
            return Ok(true);
        }
    }

    Ok(fs::canonicalize(a)? == fs::canonicalize(b)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_anchor_normalizes_missing_paths() {
        let temp = tempfile::tempdir().expect("tempdir");
        let direct = temp.path().join("x.md");
        let indirect = temp.path().join("sub/..").join("x.md");
        assert_eq!(
            canonical_anchor(&direct).unwrap(),
            canonical_anchor(&indirect).unwrap()
        );
    }

    #[cfg(unix)]
    #[test]
    fn canonical_anchor_resolves_dangling_symlinks() {
        let temp = tempfile::tempdir().expect("tempdir");
        let target = temp.path().join("target.md");
        let link = temp.path().join("link.md");
        std::os::unix::fs::symlink("target.md", &link).expect("symlink created");

        assert_eq!(
            canonical_anchor(&target).unwrap(),
            canonical_anchor(&link).unwrap()
        );
    }

    #[test]
    fn paths_alias_detects_hard_links() {
        let temp = tempfile::tempdir().expect("tempdir");
        let first = temp.path().join("first.vdl");
        let second = temp.path().join("second.vdl");
        fs::write(&first, "schema \"s\" { }").unwrap();
        fs::hard_link(&first, &second).unwrap();
        assert!(paths_alias(&first, &second).unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn paths_alias_detects_symlinks() {
        let temp = tempfile::tempdir().expect("tempdir");
        let first = temp.path().join("first.vdl");
        let second = temp.path().join("second.vdl");
        fs::write(&first, "schema \"s\" { }").unwrap();
        std::os::unix::fs::symlink(&first, &second).unwrap();
        assert!(paths_alias(&first, &second).unwrap());
    }
}
