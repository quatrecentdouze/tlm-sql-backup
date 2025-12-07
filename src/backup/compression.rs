use crate::error::{BackupError, Result};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use tracing::{debug, info};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

pub fn compress_to_zip(source_path: &Path, dest_path: &Path, archive_filename: &str) -> Result<()> {
    info!("Compressing {} to {}", source_path.display(), dest_path.display());

    if let Some(parent) = dest_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let dest_file = File::create(dest_path)?;
    let buffered_writer = BufWriter::new(dest_file);
    let mut zip = ZipWriter::new(buffered_writer);

    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(6));
    zip.start_file(archive_filename, options)?;
    let source_file = File::open(source_path)?;
    let mut reader = BufReader::new(source_file);
    let mut buffer = vec![0u8; 64 * 1024];

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        zip.write_all(&buffer[..bytes_read])?;
    }

    zip.finish()?;

    let dest_size = std::fs::metadata(dest_path)?.len();
    debug!(
        "Compression complete: {} bytes",
        dest_size
    );

    Ok(())
}

pub fn compress_multiple_to_zip(source_files: &[(PathBuf, String)], dest_path: &Path) -> Result<()> {
    compress_multiple_to_zip_silent(source_files, dest_path, false)
}

pub fn compress_multiple_to_zip_silent(source_files: &[(PathBuf, String)], dest_path: &Path, silent: bool) -> Result<()> {
    if !silent {
        info!("Compressing {} files to {}", source_files.len(), dest_path.display());
    }

    if let Some(parent) = dest_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let dest_file = File::create(dest_path)?;
    let buffered_writer = BufWriter::new(dest_file);
    let mut zip = ZipWriter::new(buffered_writer);

    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(6));

    let mut buffer = vec![0u8; 64 * 1024];

    for (source_path, archive_name) in source_files {
        if !silent {
            debug!("Adding {} as {}", source_path.display(), archive_name);
        }
        
        zip.start_file(archive_name, options)?;

        let source_file = File::open(source_path)?;
        let mut reader = BufReader::new(source_file);

        loop {
            let bytes_read = reader.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            zip.write_all(&buffer[..bytes_read])?;
        }
    }

    zip.finish()?;

    if !silent {
        let dest_size = std::fs::metadata(dest_path)?.len();
        info!(
            "Combined compression complete: {} files, {} bytes",
            source_files.len(),
            dest_size
        );
    }

    Ok(())
}

pub fn calculate_sha256(file_path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};

    let file = File::open(file_path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let hash = hasher.finalize();
    Ok(format!("{:x}", hash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_compress_to_zip() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("test.sql");
        let dest = dir.path().join("test.zip");

        let mut file = File::create(&source).unwrap();
        file.write_all(b"-- Test SQL content\nSELECT * FROM test;").unwrap();

        compress_to_zip(&source, &dest, "test.sql").unwrap();

        assert!(dest.exists());
        let dest_meta = std::fs::metadata(&dest).unwrap();
        assert!(dest_meta.len() > 0);
    }

    #[test]
    fn test_calculate_sha256() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");

        let mut file = File::create(&file_path).unwrap();
        file.write_all(b"hello world").unwrap();

        let hash = calculate_sha256(&file_path).unwrap();
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }
}
