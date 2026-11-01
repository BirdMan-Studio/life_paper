use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

pub const ENTRY_FILE: &str = "main.py";
const DEFAULT_CODE: &str = include_str!("../../../python/main.py");

/// Content-based versions detect same-size edits and helper/resource changes too.
pub fn fingerprint(root: &Path) -> io::Result<String> {
    use io::Read;
    use sha2::{Digest, Sha256};
    checked_path(root, "", true)?;
    let mut hash = Sha256::new();
    let mut pending = vec![PathBuf::new()];
    let mut count = 0;
    let mut total = 0u64;
    while let Some(relative) = pending.pop() {
        let mut entries = fs::read_dir(root.join(&relative))?.collect::<io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            count += 1;
            if count > 1024 {
                return Err(invalid("program has too many entries"));
            }
            let relative = relative.join(entry.file_name());
            let relative_text = relative.to_string_lossy().replace('\\', "/");
            let path = checked_path(root, &relative_text, false)?;
            let metadata = fs::metadata(&path)?;
            hash.update((relative_text.len() as u64).to_le_bytes());
            hash.update(relative_text.as_bytes());
            if metadata.is_dir() {
                hash.update(b"directory");
                pending.push(relative);
            } else if metadata.is_file() {
                let mut file = fs::File::open(path)?.take(10 * 1024 * 1024 + 1);
                let mut size = 0u64;
                let mut buffer = [0u8; 8192];
                hash.update(b"file");
                hash.update(metadata.len().to_le_bytes());
                loop {
                    let read = file.read(&mut buffer)?;
                    if read == 0 {
                        break;
                    }
                    size += read as u64;
                    total += read as u64;
                    if size > 10 * 1024 * 1024 || total > 50 * 1024 * 1024 {
                        return Err(invalid("program exceeds copy size limit"));
                    }
                    hash.update(&buffer[..read]);
                }
                if size != metadata.len() {
                    return Err(invalid("program changed while reading"));
                }
            } else {
                return Err(invalid("unsupported program entry"));
            }
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn reject_link(path: &Path) -> io::Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let mut linked = metadata.file_type().is_symlink();
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        linked |= metadata.file_attributes() & 0x400 != 0;
    }
    if linked {
        return Err(invalid("program links are not supported"));
    }
    Ok(())
}

pub fn checked_path(root: &Path, path: &str, allow_root: bool) -> io::Result<PathBuf> {
    let normalized = path.replace('\\', "/");
    let relative = Path::new(&normalized);
    if (!allow_root && normalized.is_empty())
        || normalized.contains(':')
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(invalid("expected a relative program path"));
    }
    reject_link(root)?;
    let mut target = root.to_path_buf();
    for part in relative.components() {
        if part.as_os_str().to_string_lossy().ends_with(['.', ' ']) {
            return Err(invalid("ambiguous program path"));
        }
        target.push(part);
        reject_link(&target)?;
    }
    Ok(target)
}

pub fn ensure_entry(root: &Path) -> io::Result<()> {
    let entry = checked_path(root, ENTRY_FILE, false)?;
    fs::create_dir_all(root)?;
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&entry)
    {
        Ok(mut file) => {
            use io::Write;
            file.write_all(DEFAULT_CODE.as_bytes())?;
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists && entry.is_file() => {}
        Err(error) => return Err(error),
    }
    Ok(())
}

/// Copy into an unpublished instance directory. Any failure rolls back the copy.
pub fn clone_program(source: &Path, target: &Path) -> io::Result<()> {
    reject_link(source)?;
    fs::create_dir(target)?;
    let result = (|| {
        let mut pending = vec![PathBuf::new()];
        let mut entries = 0usize;
        let mut bytes = 0u64;
        while let Some(relative) = pending.pop() {
            for entry in fs::read_dir(source.join(&relative))? {
                let entry = entry?;
                reject_link(&entry.path())?;
                entries += 1;
                if entries > 1024 {
                    return Err(invalid("program has too many entries"));
                }
                let metadata = entry.metadata()?;
                let next = relative.join(entry.file_name());
                if metadata.is_dir() {
                    fs::create_dir(target.join(&next))?;
                    pending.push(next);
                } else if metadata.is_file() {
                    bytes = bytes.saturating_add(metadata.len());
                    if metadata.len() > 10 * 1024 * 1024 || bytes > 50 * 1024 * 1024 {
                        return Err(invalid("program exceeds copy size limit"));
                    }
                    fs::copy(entry.path(), target.join(next))?;
                } else {
                    return Err(invalid("unsupported program entry"));
                }
            }
        }
        ensure_entry(target)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(target);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clones_entire_program_without_sharing_files() {
        let root = std::env::temp_dir().join(format!("program-test-{}", uuid::Uuid::new_v4()));
        let model = root.join("model");
        fs::create_dir_all(model.join("lib")).unwrap();
        fs::write(model.join("main.py"), "import lib.brain\n").unwrap();
        fs::write(model.join("lib/brain.py"), "speed = 1\n").unwrap();
        let first = root.join("first");
        let second = root.join("second");
        clone_program(&model, &first).unwrap();
        clone_program(&model, &second).unwrap();
        fs::write(first.join("lib/brain.py"), "speed = 2\n").unwrap();
        fs::remove_dir_all(&model).unwrap();
        assert_eq!(
            fs::read_to_string(second.join("lib/brain.py")).unwrap(),
            "speed = 1\n"
        );
        assert_eq!(
            fs::read_to_string(first.join("main.py")).unwrap(),
            "import lib.brain\n"
        );
        assert!(checked_path(&first, "../second/main.py", false).is_err());
        assert!(checked_path(&first, "C:/secret", false).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn supplies_entry_and_rolls_back_invalid_copy() {
        let root = std::env::temp_dir().join(format!("program-test-{}", uuid::Uuid::new_v4()));
        let model = root.join("model");
        fs::create_dir_all(&model).unwrap();
        let target = root.join("instance");
        clone_program(&model, &target).unwrap();
        assert!(target.join(ENTRY_FILE).is_file());
        fs::create_dir(model.join(ENTRY_FILE)).unwrap();
        let failed = root.join("failed");
        assert!(clone_program(&model, &failed).is_err());
        assert!(!failed.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_oversized_files_and_windows_path_aliases() {
        let root = std::env::temp_dir().join(format!("program-test-{}", uuid::Uuid::new_v4()));
        let source = root.join("model");
        fs::create_dir_all(&source).unwrap();
        fs::File::create(source.join("huge.py"))
            .unwrap()
            .set_len(10 * 1024 * 1024 + 1)
            .unwrap();
        let target = root.join("instance");
        assert!(clone_program(&source, &target).is_err());
        assert!(!target.exists());
        for path in [
            "main.py.",
            "main.py ",
            "main.py:stream",
            "../outside",
            "lib/../../outside",
        ] {
            assert!(checked_path(&source, path, false).is_err(), "{path}");
        }
        fs::remove_dir_all(root).unwrap();
    }
}
