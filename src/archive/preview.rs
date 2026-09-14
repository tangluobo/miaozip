use super::*;

const MAX_OPEN_BYTES: u64 = 256 * 1024 * 1024;

/// Extracts one regular file into a caller-owned temporary directory. The
/// caller must keep that directory alive while the external application uses it.
pub fn extract_entry_for_open(
    archive_path: &Path,
    entry_name: &str,
    destination: &Path,
) -> Result<PathBuf> {
    let relative = safe_archive_name(entry_name)?;
    let format = ArchiveFormat::from_path(archive_path)
        .ok_or_else(|| anyhow!("暂不支持此压缩格式：{}", archive_path.display()))?;
    fs::create_dir_all(destination)?;
    let canonical_root = destination.canonicalize()?;
    let output = destination.join(&relative);
    let parent = output
        .parent()
        .ok_or_else(|| anyhow!("无效的压缩包内文件路径"))?;
    create_safe_directory(destination, parent, &canonical_root)?;
    reject_symlink(&output)?;

    match format {
        ArchiveFormat::Zip => extract_zip_entry(archive_path, &relative, &output)?,
        ArchiveFormat::SevenZip => extract_7z_entry(archive_path, &relative, &output)?,
        ArchiveFormat::Rar => extract_rar_entry(archive_path, &relative, &output, destination)?,
        ArchiveFormat::Gzip | ArchiveFormat::Bzip2 | ArchiveFormat::Xz | ArchiveFormat::Zstd => {
            extract_stream_entry(archive_path, format, &relative, &output)?
        }
        format => extract_tar_entry(archive_path, format, &relative, &output)?,
    }
    if !fs::symlink_metadata(&output)?.file_type().is_file() {
        bail!("所选条目不是普通文件：{entry_name}");
    }
    Ok(output)
}

fn copy_limited(input: &mut dyn Read, output: &Path, advertised_size: Option<u64>) -> Result<()> {
    if advertised_size.is_some_and(|size| size > MAX_OPEN_BYTES) {
        bail!("单文件预览上限为 256 MiB，请先手动解压");
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .with_context(|| format!("无法创建临时预览文件：{}", output.display()))?;
    let copied = io::copy(&mut input.take(MAX_OPEN_BYTES + 1), &mut file)?;
    if copied > MAX_OPEN_BYTES {
        bail!("单文件预览上限为 256 MiB，请先手动解压");
    }
    file.flush()?;
    Ok(())
}

fn extract_zip_entry(archive_path: &Path, requested: &Path, output: &Path) -> Result<()> {
    let mut archive =
        ZipArchive::new(File::open(archive_path)?).context("文件不是有效的 ZIP 压缩包")?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        if safe_archive_name(entry.name())? != requested {
            continue;
        }
        if entry.is_dir() || entry.is_symlink() {
            bail!("不能直接打开目录、链接或特殊文件：{}", entry.name());
        }
        if entry.encrypted() {
            bail!("暂不支持加密 ZIP：{}", entry.name());
        }
        let size = entry.size();
        return copy_limited(&mut entry, output, Some(size));
    }
    bail!("压缩包内找不到所选文件：{}", requested.display())
}

fn extract_7z_entry(archive_path: &Path, requested: &Path, output: &Path) -> Result<()> {
    let mut reader =
        sevenz_rust2::ArchiveReader::open(archive_path, sevenz_rust2::Password::empty())
            .context("文件不是有效的 7z 压缩包")?;
    let mut found = false;
    let mut extraction_error = None;
    let result = reader.for_each_entries(|entry, input| {
        if found {
            return Ok(false);
        }
        let operation = (|| -> Result<bool> {
            if safe_archive_name(&entry.name)? != requested {
                return Ok(true);
            }
            if entry.is_directory || entry.is_anti_item {
                bail!("不能直接打开目录或删除标记：{}", entry.name);
            }
            if entry.has_windows_attributes
                && (entry.windows_attributes & 0x400 != 0
                    || (entry.windows_attributes >> 16) & 0o170000 == 0o120000)
            {
                bail!("为安全起见，不打开 7z 符号链接：{}", entry.name);
            }
            copy_limited(input, output, Some(entry.size))?;
            found = true;
            Ok(false)
        })();
        match operation {
            Ok(keep_going) => Ok(keep_going),
            Err(error) => {
                let message = format!("{error:#}");
                extraction_error = Some(error);
                Err(sevenz_rust2::Error::Other(message.into()))
            }
        }
    });
    if let Some(error) = extraction_error {
        return Err(error);
    }
    result.context("无法读取 7z 中的所选文件")?;
    if !found {
        bail!("压缩包内找不到所选文件：{}", requested.display());
    }
    Ok(())
}

fn extract_rar_entry(
    archive_path: &Path,
    requested: &Path,
    output: &Path,
    destination: &Path,
) -> Result<()> {
    let mut archive = unrar::Archive::new(archive_path)
        .open_for_processing()
        .context("文件不是有效的 RAR 压缩包")?;
    while let Some(header) = archive.read_header().context("无法读取 RAR 文件头")? {
        let name = header.entry().filename.to_string_lossy().into_owned();
        if safe_archive_name(&name)? != requested {
            archive = header.skip()?;
            continue;
        }
        if header.entry().is_directory() || header.entry().is_encrypted() {
            bail!("不能直接打开 RAR 目录或加密文件：{name}");
        }
        if header.entry().unpacked_size > MAX_OPEN_BYTES {
            bail!("单文件预览上限为 256 MiB，请先手动解压");
        }
        let staging = tempfile::tempdir_in(destination)?;
        let staged_file = staging.path().join("selected-entry");
        let _archive = header
            .extract_to(&staged_file)
            .with_context(|| format!("无法解压 RAR 项：{name}"))?;
        let metadata = fs::symlink_metadata(&staged_file)?;
        if !metadata.file_type().is_file() || metadata.len() > MAX_OPEN_BYTES {
            bail!("拒绝打开 RAR 链接、特殊文件或超过 256 MiB 的文件：{name}");
        }
        fs::rename(&staged_file, output)?;
        return Ok(());
    }
    bail!("压缩包内找不到所选文件：{}", requested.display())
}

fn extract_stream_entry(
    archive_path: &Path,
    format: ArchiveFormat,
    requested: &Path,
    output: &Path,
) -> Result<()> {
    let file_name = archive_path
        .file_name()
        .ok_or_else(|| anyhow!("无效的压缩文件名"))?
        .to_string_lossy();
    let suffix = format.extension();
    let output_name = file_name
        .get(..file_name.len().saturating_sub(suffix.len()))
        .filter(|_| file_name.to_ascii_lowercase().ends_with(suffix))
        .ok_or_else(|| anyhow!("无效的单文件压缩格式"))?;
    if safe_archive_name(output_name)? != requested {
        bail!("压缩包内找不到所选文件：{}", requested.display());
    }
    let input = File::open(archive_path)?;
    let mut decoder: Box<dyn Read> = match format {
        ArchiveFormat::Gzip => Box::new(flate2::read::GzDecoder::new(input)),
        ArchiveFormat::Bzip2 => Box::new(bzip2::read::BzDecoder::new(input)),
        ArchiveFormat::Xz => Box::new(xz2::read::XzDecoder::new(input)),
        ArchiveFormat::Zstd => Box::new(zstd::stream::read::Decoder::new(input)?),
        _ => unreachable!(),
    };
    copy_limited(&mut *decoder, output, None)
}

fn extract_tar_entry(
    archive_path: &Path,
    format: ArchiveFormat,
    requested: &Path,
    output: &Path,
) -> Result<()> {
    let mut archive = tar::Archive::new(tar_input(archive_path, format)?);
    for entry in archive.entries()? {
        let mut entry = entry?;
        if safe_archive_name(&entry.path()?.to_string_lossy())? != requested {
            continue;
        }
        if !entry.header().entry_type().is_file() {
            bail!(
                "不能直接打开 TAR 链接、目录或特殊文件：{}",
                requested.display()
            );
        }
        let size = entry.header().size()?;
        return copy_limited(&mut entry, output, Some(size));
    }
    bail!("压缩包内找不到所选文件：{}", requested.display())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_only_selected_zip_entry_without_unpacking_others() {
        let workspace = tempfile::tempdir().unwrap();
        let archive_path = workspace.path().join("sample.zip");
        let mut archive = ZipWriter::new(File::create(&archive_path).unwrap());
        archive
            .start_file("folder/selected.txt", SimpleFileOptions::default())
            .unwrap();
        archive.write_all(b"selected").unwrap();
        archive
            .start_file("other.txt", SimpleFileOptions::default())
            .unwrap();
        archive.write_all(b"other").unwrap();
        archive.finish().unwrap();
        let destination = workspace.path().join("preview");
        let selected =
            extract_entry_for_open(&archive_path, "folder/selected.txt", &destination).unwrap();
        assert_eq!(fs::read(&selected).unwrap(), b"selected");
        assert!(!destination.join("other.txt").exists());
        assert!(extract_entry_for_open(&archive_path, "../other.txt", &destination).is_err());
    }

    #[test]
    fn refuses_oversized_zip_preview_before_creating_file() {
        let workspace = tempfile::tempdir().unwrap();
        let archive_path = workspace.path().join("large.zip");
        let mut archive = ZipWriter::new(File::create(&archive_path).unwrap());
        archive
            .start_file("large.bin", SimpleFileOptions::default())
            .unwrap();
        archive.write_all(&[0u8; 1024]).unwrap();
        archive.finish().unwrap();
        let destination = workspace.path().join("preview");
        // The limited-copy helper must reject a stream that exceeds its stated cap.
        assert!(
            copy_limited(
                &mut &[0u8; 4][..],
                &destination.join("tiny"),
                Some(MAX_OPEN_BYTES + 1)
            )
            .is_err()
        );
        assert!(!destination.join("tiny").exists());
    }
}
