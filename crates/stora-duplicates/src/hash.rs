use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

use stora_core::{Result, StoraError};

/// How many bytes are read from each end during the sampling stage.
pub const SAMPLE_BYTES: usize = 8 * 1024;

/// Read buffer for full hashing. Large enough to keep syscall overhead low
/// without holding a whole file in memory.
const READ_BUFFER: usize = 128 * 1024;

/// Hashes the first and last [`SAMPLE_BYTES`] of a file.
///
/// This is the cheap stage: it eliminates the overwhelming majority of
/// same-size files that are not actually identical, without reading them in
/// full. A match here is a candidate, never a conclusion.
pub fn sample_hash(path: &str, size: u64) -> Result<u64> {
    let extended = stora_security::to_extended_length(path);
    let mut file = File::open(&extended).map_err(|err| StoraError::from_io(&err, path))?;

    let mut hasher = xxhash_rust::xxh3::Xxh3::new();
    // The size participates so two files that share their ends but differ in
    // length can never collide at this stage.
    hasher.update(&size.to_le_bytes());

    // Read up to the sample length rather than demanding it. The size was
    // recorded when the file was enumerated and the file may have shrunk
    // since; a short read is normal, not an error.
    let mut head = Vec::with_capacity(SAMPLE_BYTES.min(size as usize));
    file.by_ref()
        .take(SAMPLE_BYTES as u64)
        .read_to_end(&mut head)
        .map_err(|err| StoraError::from_io(&err, path))?;
    hasher.update(&head);

    // Only read a tail when the file is long enough for it to be distinct
    // from the head.
    let actual = file
        .metadata()
        .map(|metadata| metadata.len())
        .unwrap_or(size);

    if actual > SAMPLE_BYTES as u64 * 2 {
        file.seek(SeekFrom::End(-(SAMPLE_BYTES as i64)))
            .map_err(|err| StoraError::from_io(&err, path))?;

        let mut tail = Vec::with_capacity(SAMPLE_BYTES);
        file.by_ref()
            .take(SAMPLE_BYTES as u64)
            .read_to_end(&mut tail)
            .map_err(|err| StoraError::from_io(&err, path))?;
        hasher.update(&tail);
    }

    Ok(hasher.digest())
}

/// Computes the full SHA-256 of a file.
///
/// This is the verification stage. Nothing is presented as an exact duplicate
/// until two files agree here.
pub fn full_hash(path: &str) -> Result<String> {
    use sha2::{Digest, Sha256};

    let extended = stora_security::to_extended_length(path);
    let mut file = File::open(&extended).map_err(|err| StoraError::from_io(&err, path))?;

    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; READ_BUFFER];

    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|err| StoraError::from_io(&err, path))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

/// A file's identity on the volume, used to detect hard links.
///
/// Two paths sharing a volume serial and file index are the *same* file with
/// two names. Reporting them as duplicates would be wrong, and "removing" one
/// frees nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileIdentity {
    pub volume_serial: u64,
    pub file_index: u64,
}

/// Reads a file's volume serial and index.
pub fn file_identity(path: &str) -> Option<FileIdentity> {
    #[cfg(windows)]
    {
        use windows::core::PCWSTR;
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::Storage::FileSystem::{
            CreateFileW, GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
            FILE_ATTRIBUTE_NORMAL, FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE, FILE_SHARE_READ,
            FILE_SHARE_WRITE, OPEN_EXISTING,
        };

        let extended = stora_security::to_extended_length(path);
        let wide: Vec<u16> = extended.encode_utf16().chain(std::iter::once(0)).collect();

        // SAFETY: opening for metadata only, with full sharing so an open file
        // can still be inspected.
        let handle = unsafe {
            CreateFileW(
                PCWSTR(wide.as_ptr()),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL | FILE_FLAG_BACKUP_SEMANTICS,
                None,
            )
        }
        .ok()?;

        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: `handle` is valid and `info` is a correctly sized out-param.
        let ok = unsafe { GetFileInformationByHandle(handle, &mut info) };

        // SAFETY: `handle` came from a successful CreateFileW.
        unsafe {
            let _ = CloseHandle(handle);
        }

        ok.ok()?;

        Some(FileIdentity {
            volume_serial: info.dwVolumeSerialNumber as u64,
            file_index: ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64,
        })
    }

    #[cfg(not(windows))]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = std::fs::metadata(path).ok()?;
        Some(FileIdentity {
            volume_serial: metadata.dev(),
            file_index: metadata.ino(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(dir: &std::path::Path, name: &str, contents: &[u8]) -> String {
        let path = dir.join(name);
        let mut file = File::create(&path).unwrap();
        file.write_all(contents).unwrap();
        path.to_string_lossy().replace('/', "\\")
    }

    #[test]
    fn identical_content_produces_identical_sample_hashes() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(dir.path(), "a.bin", b"hello world");
        let b = write(dir.path(), "b.bin", b"hello world");

        assert_eq!(sample_hash(&a, 11).unwrap(), sample_hash(&b, 11).unwrap());
    }

    #[test]
    fn different_content_produces_different_sample_hashes() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(dir.path(), "a.bin", b"hello world");
        let b = write(dir.path(), "b.bin", b"hello there");

        assert_ne!(sample_hash(&a, 11).unwrap(), sample_hash(&b, 11).unwrap());
    }

    #[test]
    fn files_differing_only_in_the_middle_share_a_sample_hash() {
        // The documented weakness of sampling, and exactly why the full
        // verification stage exists.
        let dir = tempfile::tempdir().unwrap();
        let mut first = vec![b'a'; SAMPLE_BYTES];
        let mut second = first.clone();
        first.extend(vec![b'X'; 1000]);
        second.extend(vec![b'Y'; 1000]);
        first.extend(vec![b'z'; SAMPLE_BYTES]);
        second.extend(vec![b'z'; SAMPLE_BYTES]);

        let a = write(dir.path(), "a.bin", &first);
        let b = write(dir.path(), "b.bin", &second);
        let size = first.len() as u64;

        assert_eq!(
            sample_hash(&a, size).unwrap(),
            sample_hash(&b, size).unwrap(),
            "sampling cannot see the middle"
        );
        assert_ne!(
            full_hash(&a).unwrap(),
            full_hash(&b).unwrap(),
            "full verification must catch it"
        );
    }

    #[test]
    fn size_participates_in_the_sample_hash() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(dir.path(), "a.bin", b"abc");
        // The declared size is folded into the digest, so two files that share
        // their sampled bytes but not their length cannot collide.
        assert_ne!(sample_hash(&a, 3).unwrap(), sample_hash(&a, 4).unwrap());
    }

    #[test]
    fn a_file_shorter_than_its_recorded_size_still_hashes() {
        // The file shrank between enumeration and hashing. A short read is a
        // normal race, not a failure.
        let dir = tempfile::tempdir().unwrap();
        let a = write(dir.path(), "a.bin", b"abc");
        assert!(sample_hash(&a, 1_000_000).is_ok());
    }

    #[test]
    fn full_hashes_match_for_identical_files() {
        let dir = tempfile::tempdir().unwrap();
        let payload = vec![7u8; 300_000];
        let a = write(dir.path(), "a.bin", &payload);
        let b = write(dir.path(), "b.bin", &payload);

        let hash = full_hash(&a).unwrap();
        assert_eq!(hash, full_hash(&b).unwrap());
        assert_eq!(hash.len(), 64, "SHA-256 is 32 bytes of hex");
    }

    #[test]
    fn an_empty_file_hashes_to_the_known_sha256() {
        let dir = tempfile::tempdir().unwrap();
        let empty = write(dir.path(), "empty.bin", b"");
        assert_eq!(
            full_hash(&empty).unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn hashing_a_missing_file_reports_not_found() {
        let err = full_hash("C:\\definitely\\missing.bin").unwrap_err();
        assert_eq!(err.code(), "PathNotFound");
    }

    #[test]
    fn a_file_shares_its_identity_with_itself() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(dir.path(), "a.bin", b"data");
        let first = file_identity(&a).expect("identity available");
        assert_eq!(Some(first), file_identity(&a));
    }

    #[test]
    fn distinct_files_have_distinct_identities() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(dir.path(), "a.bin", b"data");
        let b = write(dir.path(), "b.bin", b"data");

        let (Some(first), Some(second)) = (file_identity(&a), file_identity(&b)) else {
            return; // Identity is unavailable on this filesystem.
        };
        assert_ne!(first, second, "two real files are not the same file");
    }

    #[test]
    fn identity_of_a_missing_file_is_none() {
        assert!(file_identity("C:\\definitely\\missing.bin").is_none());
    }
}
