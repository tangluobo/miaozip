use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

mod preview;
#[cfg(all(windows, target_arch = "x86"))]
mod rar_win32;
pub use preview::extract_entry_for_open;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ArchiveFormat {
    #[default]
    Zip,
    SevenZip,
    Rar,
    Tar,
    TarGz,
    TarBz2,
    TarXz,
    TarZst,
    Gzip,
    Bzip2,
    Xz,
    Zstd,
}

impl ArchiveFormat {
    pub const ALL: [Self; 12] = [
        Self::Zip,
        Self::SevenZip,
        Self::Rar,
        Self::Tar,
        Self::TarGz,
        Self::TarBz2,
        Self::TarXz,
        Self::TarZst,
        Self::Gzip,
        Self::Bzip2,
        Self::Xz,
        Self::Zstd,
    ];

    pub const CREATABLE: [Self; 7] = [
        Self::Zip,
        Self::SevenZip,
        Self::Tar,
        Self::TarGz,
        Self::TarBz2,
        Self::TarXz,
        Self::TarZst,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Zip => "ZIP",
            Self::SevenZip => "7z",
            Self::Rar => "RAR",
            Self::Tar => "TAR",
            Self::TarGz => "TAR.GZ",
            Self::TarBz2 => "TAR.BZ2",
            Self::TarXz => "TAR.XZ",
            Self::TarZst => "TAR.ZST",
            Self::Gzip => "GZ",
            Self::Bzip2 => "BZ2",
            Self::Xz => "XZ",
            Self::Zstd => "ZST",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Zip => ".zip",
            Self::SevenZip => ".7z",
            Self::Rar => ".rar",
            Self::Tar => ".tar",
            Self::TarGz => ".tar.gz",
            Self::TarBz2 => ".tar.bz2",
            Self::TarXz => ".tar.xz",
            Self::TarZst => ".tar.zst",
            Self::Gzip => ".gz",
            Self::Bzip2 => ".bz2",
            Self::Xz => ".xz",
            Self::Zstd => ".zst",
        }
    }

    pub fn from_path(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|format| name.ends_with(format.extension()))
            .or_else(|| match name.rsplit_once('.')?.1 {
                "tgz" => Some(Self::TarGz),
                "tbz" | "tbz2" => Some(Self::TarBz2),
                "txz" => Some(Self::TarXz),
                "tzst" => Some(Self::TarZst),
                _ => None,
            })
    }
}

#[derive(Debug, Clone)]
pub struct Progress {
    pub completed: usize,
    pub total: usize,
    pub current: String,
}

#[derive(Debug, Clone, Default)]
pub struct OperationSummary {
    pub files: usize,
    pub directories: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveListEntry {
    pub path: String,
    pub is_directory: bool,
    pub size: Option<u64>,
}

impl ArchiveListEntry {
    fn new(name: &str, is_directory: bool, size: Option<u64>) -> Result<Self> {
        let safe = safe_archive_name(name)?;
        Ok(Self {
            path: safe
                .iter()
                .map(|part| part.to_string_lossy())
                .collect::<Vec<_>>()
                .join("/"),
            is_directory,
            size: if is_directory { None } else { size },
        })
    }
}

/// Reads archive metadata for the main-window browser without extracting data.
pub fn list_archive_entries(archive_path: &Path) -> Result<Vec<ArchiveListEntry>> {
    let format = ArchiveFormat::from_path(archive_path)
        .ok_or_else(|| anyhow!("暂不支持此压缩格式：{}", archive_path.display()))?;
    let mut result = Vec::new();
    match format {
        ArchiveFormat::Zip => {
            let mut archive =
                ZipArchive::new(File::open(archive_path)?).context("文件不是有效的 ZIP 压缩包")?;
            for index in 0..archive.len() {
                let entry = archive.by_index(index).context("无法读取 ZIP 文件目录")?;
                result.push(ArchiveListEntry::new(
                    entry.name(),
                    entry.is_dir(),
                    Some(entry.size()),
                )?);
            }
        }
        ArchiveFormat::SevenZip => {
            let reader =
                sevenz_rust2::ArchiveReader::open(archive_path, sevenz_rust2::Password::empty())
                    .context("文件不是有效的 7z 压缩包")?;
            for entry in &reader.archive().files {
                result.push(ArchiveListEntry::new(
                    &entry.name,
                    entry.is_directory,
                    Some(entry.size),
                )?);
            }
        }
        ArchiveFormat::Rar => {
            #[cfg(not(all(windows, target_arch = "x86")))]
            {
                let archive = unrar::Archive::new(archive_path)
                    .open_for_listing()
                    .context("文件不是有效的 RAR 压缩包")?;
                for entry in archive {
                    let entry = entry.context("无法读取 RAR 文件目录")?;
                    result.push(ArchiveListEntry::new(
                        &entry.filename.to_string_lossy(),
                        entry.is_directory(),
                        Some(entry.unpacked_size),
                    )?);
                }
            }
            #[cfg(all(windows, target_arch = "x86"))]
            {
                let mut archive = rar_win32::Archive::open_for_listing(archive_path)
                    .context("文件不是有效的 RAR 压缩包")?;
                while let Some(entry) = archive.read_header()? {
                    result.push(ArchiveListEntry::new(
                        &entry.name,
                        entry.is_directory,
                        Some(entry.unpacked_size),
                    )?);
                    archive.skip()?;
                }
            }
        }
        ArchiveFormat::Gzip | ArchiveFormat::Bzip2 | ArchiveFormat::Xz | ArchiveFormat::Zstd => {
            let filename = archive_path
                .file_name()
                .ok_or_else(|| anyhow!("无法识别压缩文件名"))?
                .to_string_lossy();
            let suffix = format.extension();
            let output_name = filename
                .get(..filename.len().saturating_sub(suffix.len()))
                .filter(|_| filename.to_ascii_lowercase().ends_with(suffix))
                .filter(|name| !name.is_empty())
                .ok_or_else(|| anyhow!("无效的单文件压缩格式"))?;
            result.push(ArchiveListEntry::new(output_name, false, None)?);
        }
        _ => {
            let mut archive = tar::Archive::new(tar_input(archive_path, format)?);
            for entry in archive.entries().context("无法读取 TAR 文件目录")? {
                let entry = entry.context("无法读取 TAR 条目")?;
                result.push(ArchiveListEntry::new(
                    &entry.path()?.to_string_lossy(),
                    entry.header().entry_type().is_dir(),
                    Some(entry.header().size()?),
                )?);
            }
        }
    }
    Ok(result)
}

#[derive(Debug)]
struct ArchiveEntry {
    source: PathBuf,
    name: String,
    is_directory: bool,
}

pub fn create_archive(
    inputs: &[PathBuf],
    destination: &Path,
    format: ArchiveFormat,
    compression_level: u8,
    on_progress: impl FnMut(Progress),
) -> Result<OperationSummary> {
    if format
        != ArchiveFormat::from_path(destination)
            .ok_or_else(|| anyhow!("压缩文件扩展名与所选格式不匹配"))?
    {
        bail!(
            "压缩文件扩展名与所选格式不匹配：应以 {} 结尾",
            format.extension()
        );
    }
    match format {
        ArchiveFormat::Zip => {
            create_zip_archive(inputs, destination, compression_level, on_progress)
        }
        ArchiveFormat::SevenZip => create_7z_archive(inputs, destination, on_progress),
        ArchiveFormat::Rar => bail!("RAR 目前仅支持解压，不能创建 RAR 压缩包"),
        ArchiveFormat::Gzip | ArchiveFormat::Bzip2 | ArchiveFormat::Xz | ArchiveFormat::Zstd => {
            bail!("此单文件压缩格式目前仅支持解压")
        }
        _ => create_tar_archive(inputs, destination, format, compression_level, on_progress),
    }
}

fn create_zip_archive(
    inputs: &[PathBuf],
    destination: &Path,
    compression_level: u8,
    mut on_progress: impl FnMut(Progress),
) -> Result<OperationSummary> {
    if inputs.is_empty() {
        bail!("请至少添加一个文件或目录");
    }
    if compression_level > 9 {
        bail!("压缩级别必须在 0 到 9 之间");
    }

    let entries = collect_entries(inputs, destination)?;
    let total = entries.len();
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .with_context(|| format!("无法创建输出目录：{}", parent.display()))?;

    let output = File::create(destination)
        .with_context(|| format!("无法创建压缩包：{}", destination.display()))?;
    let mut writer = ZipWriter::new(output);
    let file_options = if compression_level == 0 {
        SimpleFileOptions::default().compression_method(CompressionMethod::Stored)
    } else {
        SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .compression_level(Some(i64::from(compression_level)))
    }
    .unix_permissions(0o644);
    let directory_options = SimpleFileOptions::default().unix_permissions(0o755);
    let mut summary = OperationSummary::default();

    let result = (|| -> Result<()> {
        for (index, entry) in entries.iter().enumerate() {
            on_progress(Progress {
                completed: index,
                total,
                current: entry.name.clone(),
            });

            if entry.is_directory {
                let directory_name = format!("{}/", entry.name.trim_end_matches('/'));
                writer
                    .add_directory(directory_name, directory_options)
                    .with_context(|| format!("无法写入目录：{}", entry.name))?;
                summary.directories += 1;
            } else {
                writer
                    .start_file(&entry.name, file_options)
                    .with_context(|| format!("无法写入文件头：{}", entry.name))?;
                let mut source = File::open(&entry.source)
                    .with_context(|| format!("无法读取文件：{}", entry.source.display()))?;
                summary.bytes += io::copy(&mut source, &mut writer)
                    .with_context(|| format!("无法压缩文件：{}", entry.source.display()))?;
                summary.files += 1;
            }

            on_progress(Progress {
                completed: index + 1,
                total,
                current: entry.name.clone(),
            });
        }

        writer.finish().context("无法完成 ZIP 文件写入")?;
        Ok(())
    })();

    if let Err(error) = result {
        // A failed operation must not leave a file that looks like a valid archive.
        let _ = fs::remove_file(destination);
        return Err(error);
    }

    Ok(summary)
}

pub fn extract_archive(
    archive_path: &Path,
    destination: &Path,
    on_progress: impl FnMut(Progress),
) -> Result<OperationSummary> {
    match ArchiveFormat::from_path(archive_path) {
        Some(ArchiveFormat::Zip) => extract_zip_archive(archive_path, destination, on_progress),
        Some(ArchiveFormat::SevenZip) => extract_7z_archive(archive_path, destination, on_progress),
        Some(ArchiveFormat::Rar) => extract_rar_archive(archive_path, destination, on_progress),
        Some(
            format @ (ArchiveFormat::Gzip
            | ArchiveFormat::Bzip2
            | ArchiveFormat::Xz
            | ArchiveFormat::Zstd),
        ) => extract_single_stream(archive_path, destination, format, on_progress),
        Some(format) => extract_tar_archive(archive_path, destination, format, on_progress),
        None => bail!("暂不支持此压缩格式：{}", archive_path.display()),
    }
}

fn extract_zip_archive(
    archive_path: &Path,
    destination: &Path,
    mut on_progress: impl FnMut(Progress),
) -> Result<OperationSummary> {
    let input = File::open(archive_path)
        .with_context(|| format!("无法打开压缩包：{}", archive_path.display()))?;
    let mut archive = ZipArchive::new(input).context("文件不是有效的 ZIP 压缩包")?;

    // Validate every entry before writing anything. `enclosed_name` blocks absolute
    // paths and parent traversal such as ../../outside.txt.
    for index in 0..archive.len() {
        let entry = archive.by_index(index).context("无法读取 ZIP 文件目录")?;
        if entry.enclosed_name().is_none() {
            bail!("压缩包包含不安全路径：{}", entry.name());
        }
        if entry.is_symlink() {
            bail!("为安全起见，不解压符号链接：{}", entry.name());
        }
        if entry.encrypted() {
            bail!("暂不支持加密 ZIP：{}", entry.name());
        }
    }

    fs::create_dir_all(destination)
        .with_context(|| format!("无法创建解压目录：{}", destination.display()))?;
    let canonical_root = destination
        .canonicalize()
        .with_context(|| format!("无法解析解压目录：{}", destination.display()))?;

    let total = archive.len();
    let mut summary = OperationSummary::default();
    for index in 0..total {
        let mut entry = archive
            .by_index(index)
            .with_context(|| format!("无法读取 ZIP 中的第 {} 项", index + 1))?;
        let relative_path = entry
            .enclosed_name()
            .ok_or_else(|| anyhow!("压缩包包含不安全路径：{}", entry.name()))?;
        let output_path = destination.join(&relative_path);
        let display_name = relative_path.to_string_lossy().into_owned();

        on_progress(Progress {
            completed: index,
            total,
            current: display_name.clone(),
        });

        if entry.is_dir() {
            create_safe_directory(destination, &output_path, &canonical_root)?;
            summary.directories += 1;
        } else {
            let parent = output_path
                .parent()
                .ok_or_else(|| anyhow!("无效的输出路径：{}", output_path.display()))?;
            create_safe_directory(destination, parent, &canonical_root)?;

            if fs::symlink_metadata(&output_path)
                .is_ok_and(|metadata| metadata.file_type().is_symlink())
            {
                bail!("拒绝覆盖符号链接：{}", output_path.display());
            }

            let mut output = File::create(&output_path)
                .with_context(|| format!("无法创建文件：{}", output_path.display()))?;
            summary.bytes += io::copy(&mut entry, &mut output)
                .with_context(|| format!("无法解压文件：{}", output_path.display()))?;
            summary.files += 1;
        }

        on_progress(Progress {
            completed: index + 1,
            total,
            current: display_name,
        });
    }

    Ok(summary)
}

fn create_7z_archive(
    inputs: &[PathBuf],
    destination: &Path,
    mut on_progress: impl FnMut(Progress),
) -> Result<OperationSummary> {
    let entries = collect_entries(inputs, destination)?;
    ensure_output_parent(destination)?;
    let result = (|| -> Result<OperationSummary> {
        let mut writer =
            sevenz_rust2::ArchiveWriter::create(destination).context("无法创建 7z 文件")?;
        let mut summary = OperationSummary::default();
        let total = entries.len();
        for (index, entry) in entries.iter().enumerate() {
            on_progress(Progress {
                completed: index,
                total,
                current: entry.name.clone(),
            });
            let item = sevenz_rust2::ArchiveEntry::from_path(&entry.source, entry.name.clone());
            if entry.is_directory {
                writer
                    .push_archive_entry(item, None::<File>)
                    .context("无法写入 7z 目录")?;
                summary.directories += 1;
            } else {
                let file = File::open(&entry.source)
                    .with_context(|| format!("无法读取文件：{}", entry.source.display()))?;
                summary.bytes += file.metadata()?.len();
                writer
                    .push_archive_entry(item, Some(file))
                    .context("无法写入 7z 文件")?;
                summary.files += 1;
            }
            on_progress(Progress {
                completed: index + 1,
                total,
                current: entry.name.clone(),
            });
        }
        writer.finish().context("无法完成 7z 文件写入")?;
        Ok(summary)
    })();
    if result.is_err() {
        let _ = fs::remove_file(destination);
    }
    result
}

fn extract_7z_archive(
    archive_path: &Path,
    destination: &Path,
    mut on_progress: impl FnMut(Progress),
) -> Result<OperationSummary> {
    let mut reader =
        sevenz_rust2::ArchiveReader::open(archive_path, sevenz_rust2::Password::empty())
            .context("文件不是有效的 7z 压缩包")?;
    for entry in &reader.archive().files {
        safe_archive_name(&entry.name)?;
        if entry.is_anti_item {
            bail!("不支持 7z 删除标记：{}", entry.name);
        }
        if entry.has_windows_attributes
            && (entry.windows_attributes & 0x400 != 0
                || (entry.windows_attributes >> 16) & 0o170000 == 0o120000)
        {
            bail!("为安全起见，不解压 7z 符号链接：{}", entry.name);
        }
    }
    fs::create_dir_all(destination)?;
    let canonical_root = destination.canonicalize()?;
    let total = reader.archive().files.len();
    let mut summary = OperationSummary::default();
    let mut completed = 0;
    reader
        .for_each_entries(|entry, input| {
            let result = (|| -> Result<()> {
                let relative = safe_archive_name(&entry.name)?;
                let output = destination.join(relative);
                on_progress(Progress {
                    completed,
                    total,
                    current: entry.name.clone(),
                });
                if entry.is_directory {
                    create_safe_directory(destination, &output, &canonical_root)?;
                    summary.directories += 1;
                } else {
                    let parent = output.parent().ok_or_else(|| anyhow!("无效的输出路径"))?;
                    create_safe_directory(destination, parent, &canonical_root)?;
                    reject_symlink(&output)?;
                    let mut file = File::create(&output)?;
                    summary.bytes += io::copy(input, &mut file)?;
                    summary.files += 1;
                }
                completed += 1;
                on_progress(Progress {
                    completed,
                    total,
                    current: entry.name.clone(),
                });
                Ok(())
            })();
            result.map_err(|error| sevenz_rust2::Error::Other(format!("{error:#}").into()))?;
            Ok(true)
        })
        .context("7z 解压失败")?;
    Ok(summary)
}

#[cfg(not(all(windows, target_arch = "x86")))]
fn extract_rar_archive(
    archive_path: &Path,
    destination: &Path,
    mut on_progress: impl FnMut(Progress),
) -> Result<OperationSummary> {
    let listing = unrar::Archive::new(archive_path)
        .open_for_listing()
        .context("文件不是有效的 RAR 压缩包")?;
    let mut total = 0;
    for header in listing {
        let header = header.context("无法读取 RAR 文件目录")?;
        safe_archive_name(&header.filename.to_string_lossy())?;
        if header.is_encrypted() {
            bail!("暂不支持加密 RAR：{}", header.filename.display());
        }
        total += 1;
    }

    let staging = tempfile::tempdir().context("无法建立 RAR 解压临时目录")?;
    fs::create_dir_all(destination)
        .with_context(|| format!("无法创建解压目录：{}", destination.display()))?;
    let canonical_root = destination.canonicalize()?;
    let mut archive = unrar::Archive::new(archive_path)
        .open_for_processing()
        .context("无法打开 RAR 进行解压")?;
    let mut summary = OperationSummary::default();
    let mut completed = 0;
    while let Some(header) = archive.read_header().context("无法读取 RAR 文件头")? {
        let name = header.entry().filename.to_string_lossy().into_owned();
        let relative = safe_archive_name(&name)?;
        let output = destination.join(relative);
        on_progress(Progress {
            completed,
            total,
            current: name.clone(),
        });
        if header.entry().is_directory() {
            create_safe_directory(destination, &output, &canonical_root)?;
            summary.directories += 1;
            archive = header.skip().context("无法跳过 RAR 目录项")?;
        } else {
            let stage_path = staging.path().join(format!("entry-{completed}"));
            archive = header
                .extract_to(&stage_path)
                .with_context(|| format!("无法解压 RAR 项：{name}"))?;
            if !fs::symlink_metadata(&stage_path)?.file_type().is_file() {
                bail!("为安全起见，不解压 RAR 链接或特殊文件：{name}");
            }
            let parent = output
                .parent()
                .ok_or_else(|| anyhow!("无效的输出路径：{name}"))?;
            create_safe_directory(destination, parent, &canonical_root)?;
            reject_symlink(&output)?;
            summary.bytes += fs::copy(&stage_path, &output)
                .with_context(|| format!("无法写入解压文件：{}", output.display()))?;
            summary.files += 1;
        }
        completed += 1;
        on_progress(Progress {
            completed,
            total,
            current: name,
        });
    }
    Ok(summary)
}

#[cfg(all(windows, target_arch = "x86"))]
fn extract_rar_archive(
    archive_path: &Path,
    destination: &Path,
    mut on_progress: impl FnMut(Progress),
) -> Result<OperationSummary> {
    let mut listing =
        rar_win32::Archive::open_for_listing(archive_path).context("文件不是有效的 RAR 压缩包")?;
    let mut total = 0;
    while let Some(header) = listing.read_header()? {
        safe_archive_name(&header.name)?;
        if header.is_encrypted {
            bail!("暂不支持加密 RAR：{}", header.name);
        }
        if header.is_redirection {
            bail!("为安全起见，不解压 RAR 链接或重定向项：{}", header.name);
        }
        total += 1;
        listing.skip()?;
    }

    let staging = tempfile::tempdir().context("无法建立 RAR 解压临时目录")?;
    fs::create_dir_all(destination)
        .with_context(|| format!("无法创建解压目录：{}", destination.display()))?;
    let canonical_root = destination.canonicalize()?;
    let mut archive =
        rar_win32::Archive::open_for_processing(archive_path).context("无法打开 RAR 进行解压")?;
    let mut summary = OperationSummary::default();
    let mut completed = 0;
    while let Some(header) = archive.read_header()? {
        let relative = safe_archive_name(&header.name)?;
        let output = destination.join(relative);
        on_progress(Progress {
            completed,
            total,
            current: header.name.clone(),
        });

        if header.is_directory {
            create_safe_directory(destination, &output, &canonical_root)?;
            summary.directories += 1;
            archive.skip()?;
        } else {
            let stage_path = staging.path().join(format!("entry-{completed}"));
            archive
                .extract_to(&stage_path)
                .with_context(|| format!("无法解压 RAR 项：{}", header.name))?;
            if !fs::symlink_metadata(&stage_path)?.file_type().is_file() {
                bail!("为安全起见，不解压 RAR 链接或特殊文件：{}", header.name);
            }
            let parent = output
                .parent()
                .ok_or_else(|| anyhow!("无效的输出路径：{}", header.name))?;
            create_safe_directory(destination, parent, &canonical_root)?;
            reject_symlink(&output)?;
            summary.bytes += fs::copy(&stage_path, &output)
                .with_context(|| format!("无法写入解压文件：{}", output.display()))?;
            summary.files += 1;
        }

        completed += 1;
        on_progress(Progress {
            completed,
            total,
            current: header.name,
        });
    }
    Ok(summary)
}

fn extract_single_stream(
    archive_path: &Path,
    destination: &Path,
    format: ArchiveFormat,
    mut on_progress: impl FnMut(Progress),
) -> Result<OperationSummary> {
    let name = archive_path
        .file_name()
        .ok_or_else(|| anyhow!("无法识别压缩文件名"))?
        .to_string_lossy();
    let suffix = format.extension();
    let output_name = name
        .get(..name.len().saturating_sub(suffix.len()))
        .filter(|_| name.to_ascii_lowercase().ends_with(suffix))
        .filter(|name| !name.is_empty())
        .ok_or_else(|| anyhow!("无效的单文件压缩格式：{}", archive_path.display()))?;
    let relative = safe_archive_name(output_name)?;
    let input = File::open(archive_path)?;
    let mut decoder: Box<dyn Read> = match format {
        ArchiveFormat::Gzip => Box::new(flate2::read::GzDecoder::new(input)),
        ArchiveFormat::Bzip2 => Box::new(bzip2::read::BzDecoder::new(input)),
        ArchiveFormat::Xz => Box::new(xz2::read::XzDecoder::new(input)),
        ArchiveFormat::Zstd => Box::new(zstd::stream::read::Decoder::new(input)?),
        _ => unreachable!(),
    };
    fs::create_dir_all(destination)?;
    let root = destination.canonicalize()?;
    let output = destination.join(relative);
    ensure_path_is_inside(destination, &root)?;
    reject_symlink(&output)?;
    on_progress(Progress {
        completed: 0,
        total: 1,
        current: output_name.to_owned(),
    });
    let mut file = File::create(&output)?;
    let bytes = io::copy(&mut decoder, &mut file)
        .with_context(|| format!("无法解压：{}", archive_path.display()))?;
    on_progress(Progress {
        completed: 1,
        total: 1,
        current: output_name.to_owned(),
    });
    Ok(OperationSummary {
        files: 1,
        directories: 0,
        bytes,
    })
}

fn create_tar_archive(
    inputs: &[PathBuf],
    destination: &Path,
    format: ArchiveFormat,
    level: u8,
    mut on_progress: impl FnMut(Progress),
) -> Result<OperationSummary> {
    let entries = collect_entries(inputs, destination)?;
    ensure_output_parent(destination)?;
    let result = (|| -> Result<OperationSummary> {
        let output = File::create(destination)?;
        let summary = match format {
            ArchiveFormat::Tar => {
                let (file, summary) = write_tar(&entries, output, &mut on_progress)?;
                file.sync_all()?;
                summary
            }
            ArchiveFormat::TarGz => {
                let encoder =
                    flate2::write::GzEncoder::new(output, flate2::Compression::new(level.into()));
                let (encoder, summary) = write_tar(&entries, encoder, &mut on_progress)?;
                encoder.finish()?;
                summary
            }
            ArchiveFormat::TarBz2 => {
                let encoder =
                    bzip2::write::BzEncoder::new(output, bzip2::Compression::new(level.into()));
                let (encoder, summary) = write_tar(&entries, encoder, &mut on_progress)?;
                encoder.finish()?;
                summary
            }
            ArchiveFormat::TarXz => {
                let encoder = xz2::write::XzEncoder::new(output, level.into());
                let (encoder, summary) = write_tar(&entries, encoder, &mut on_progress)?;
                encoder.finish()?;
                summary
            }
            ArchiveFormat::TarZst => {
                let encoder = zstd::stream::write::Encoder::new(output, i32::from(level))?;
                let (encoder, summary) = write_tar(&entries, encoder, &mut on_progress)?;
                encoder.finish()?;
                summary
            }
            _ => unreachable!(),
        };
        Ok(summary)
    })();
    if result.is_err() {
        let _ = fs::remove_file(destination);
    }
    result
}

fn write_tar<W: Write>(
    entries: &[ArchiveEntry],
    output: W,
    on_progress: &mut impl FnMut(Progress),
) -> Result<(W, OperationSummary)> {
    let mut writer = tar::Builder::new(output);
    let mut summary = OperationSummary::default();
    let total = entries.len();
    for (index, entry) in entries.iter().enumerate() {
        on_progress(Progress {
            completed: index,
            total,
            current: entry.name.clone(),
        });
        if entry.is_directory {
            writer.append_dir(&entry.name, &entry.source)?;
            summary.directories += 1;
        } else {
            let mut file = File::open(&entry.source)?;
            summary.bytes += file.metadata()?.len();
            writer.append_file(&entry.name, &mut file)?;
            summary.files += 1;
        }
        on_progress(Progress {
            completed: index + 1,
            total,
            current: entry.name.clone(),
        });
    }
    let output = writer.into_inner()?;
    Ok((output, summary))
}

fn extract_tar_archive(
    archive_path: &Path,
    destination: &Path,
    format: ArchiveFormat,
    mut on_progress: impl FnMut(Progress),
) -> Result<OperationSummary> {
    // Validate every header before touching the destination. This requires a second streaming pass.
    let total = {
        let mut archive = tar::Archive::new(tar_input(archive_path, format)?);
        let mut count = 0;
        for entry in archive.entries()? {
            let entry = entry?;
            let name = entry.path()?;
            safe_archive_name(&name.to_string_lossy())?;
            if !entry.header().entry_type().is_file() && !entry.header().entry_type().is_dir() {
                bail!("为安全起见，不解压链接或特殊文件：{}", name.display());
            }
            count += 1;
        }
        count
    };
    fs::create_dir_all(destination)?;
    let canonical_root = destination.canonicalize()?;
    let mut archive = tar::Archive::new(tar_input(archive_path, format)?);
    let mut summary = OperationSummary::default();
    for (index, entry) in archive.entries()?.enumerate() {
        let mut entry = entry?;
        let name = entry.path()?.to_string_lossy().into_owned();
        let relative = safe_archive_name(&name)?;
        let output = destination.join(relative);
        on_progress(Progress {
            completed: index,
            total,
            current: name.clone(),
        });
        if entry.header().entry_type().is_dir() {
            create_safe_directory(destination, &output, &canonical_root)?;
            summary.directories += 1;
        } else {
            let parent = output.parent().ok_or_else(|| anyhow!("无效的输出路径"))?;
            create_safe_directory(destination, parent, &canonical_root)?;
            reject_symlink(&output)?;
            let mut file = File::create(&output)?;
            summary.bytes += io::copy(&mut entry, &mut file)?;
            summary.files += 1;
        }
        on_progress(Progress {
            completed: index + 1,
            total,
            current: name,
        });
    }
    Ok(summary)
}

fn tar_input(path: &Path, format: ArchiveFormat) -> Result<Box<dyn Read>> {
    let file = File::open(path)?;
    let reader: Box<dyn Read> = match format {
        ArchiveFormat::Tar => Box::new(file),
        ArchiveFormat::TarGz => Box::new(flate2::read::GzDecoder::new(file)),
        ArchiveFormat::TarBz2 => Box::new(bzip2::read::BzDecoder::new(file)),
        ArchiveFormat::TarXz => Box::new(xz2::read::XzDecoder::new(file)),
        ArchiveFormat::TarZst => Box::new(zstd::stream::read::Decoder::new(file)?),
        _ => unreachable!(),
    };
    Ok(reader)
}

fn safe_archive_name(name: &str) -> Result<PathBuf> {
    if name.is_empty() || name.starts_with('/') || name.starts_with('\\') || name.contains(':') {
        bail!("压缩包包含不安全路径：{name}");
    }
    let mut path = PathBuf::new();
    for part in name.split(['/', '\\']) {
        if part == ".." || part.is_empty() && path.as_os_str().is_empty() {
            bail!("压缩包包含不安全路径：{name}");
        }
        if part != "." && !part.is_empty() {
            path.push(part);
        }
    }
    if path.as_os_str().is_empty() {
        bail!("压缩包包含空路径");
    }
    Ok(path)
}

fn reject_symlink(path: &Path) -> Result<()> {
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        bail!("拒绝覆盖符号链接：{}", path.display());
    }
    Ok(())
}

fn ensure_output_parent(destination: &Path) -> Result<()> {
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).with_context(|| format!("无法创建输出目录：{}", parent.display()))
}

fn collect_entries(inputs: &[PathBuf], destination: &Path) -> Result<Vec<ArchiveEntry>> {
    let mut entries = Vec::new();
    let mut used_roots = HashSet::new();
    let normalized_destination = absolute_path(destination);

    for input in inputs {
        if !input.exists() {
            bail!("文件或目录不存在：{}", input.display());
        }
        if fs::symlink_metadata(input)
            .with_context(|| format!("无法读取路径信息：{}", input.display()))?
            .file_type()
            .is_symlink()
        {
            bail!("为安全起见，不压缩符号链接：{}", input.display());
        }

        let original_root = input
            .file_name()
            .ok_or_else(|| anyhow!("无法确定路径名称：{}", input.display()))?
            .to_string_lossy();
        let root_name = unique_root_name(&original_root, &mut used_roots);

        if input.is_file() {
            if !same_path(input, &normalized_destination) {
                entries.push(ArchiveEntry {
                    source: input.clone(),
                    name: root_name,
                    is_directory: false,
                });
            }
            continue;
        }

        if !input.is_dir() {
            bail!("不支持此文件类型：{}", input.display());
        }

        entries.push(ArchiveEntry {
            source: input.clone(),
            name: root_name.clone(),
            is_directory: true,
        });

        for walked in WalkDir::new(input)
            .min_depth(1)
            .follow_links(false)
            .sort_by_file_name()
        {
            let walked = walked.with_context(|| format!("无法遍历目录：{}", input.display()))?;
            if walked.path_is_symlink() {
                continue;
            }
            if same_path(walked.path(), &normalized_destination) {
                continue;
            }
            if !walked.file_type().is_dir() && !walked.file_type().is_file() {
                continue;
            }

            let relative = walked
                .path()
                .strip_prefix(input)
                .with_context(|| format!("无法计算相对路径：{}", walked.path().display()))?;
            let relative_name = portable_zip_path(relative);
            let name = if relative_name.is_empty() {
                root_name.clone()
            } else {
                format!("{root_name}/{relative_name}")
            };
            let is_directory = walked.file_type().is_dir();
            entries.push(ArchiveEntry {
                source: walked.into_path(),
                name,
                is_directory,
            });
        }
    }

    if entries.is_empty() {
        bail!("没有可压缩的内容");
    }
    Ok(entries)
}

fn portable_zip_path(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn unique_root_name(original: &str, used: &mut HashSet<String>) -> String {
    if used.insert(original.to_lowercase()) {
        return original.to_owned();
    }

    let path = Path::new(original);
    let stem = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let extension = path.extension().map(|value| value.to_string_lossy());
    for number in 2.. {
        let candidate = match &extension {
            Some(extension) => format!("{stem} ({number}).{extension}"),
            None => format!("{stem} ({number})"),
        };
        if used.insert(candidate.to_lowercase()) {
            return candidate;
        }
    }
    unreachable!()
}

fn absolute_path(path: &Path) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let canonical_parent = parent
        .canonicalize()
        .unwrap_or_else(|_| parent.to_path_buf());
    path.file_name()
        .map(|name| canonical_parent.join(name))
        .unwrap_or(canonical_parent)
}

fn same_path(path: &Path, other_absolute_path: &Path) -> bool {
    let path = absolute_path(path);
    if cfg!(windows) {
        path.to_string_lossy()
            .eq_ignore_ascii_case(&other_absolute_path.to_string_lossy())
    } else {
        path == other_absolute_path
    }
}

fn ensure_path_is_inside(path: &Path, canonical_root: &Path) -> Result<()> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("无法解析路径：{}", path.display()))?;
    if !canonical.starts_with(canonical_root) {
        bail!("拒绝写入解压目录之外：{}", path.display());
    }
    Ok(())
}

fn create_safe_directory(destination: &Path, path: &Path, canonical_root: &Path) -> Result<()> {
    let relative = path
        .strip_prefix(destination)
        .with_context(|| format!("拒绝写入解压目录之外：{}", path.display()))?;
    let mut current = destination.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            bail!("压缩包包含不安全路径：{}", path.display());
        };
        current.push(name);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                bail!("拒绝经过符号链接：{}", current.display());
            }
            Ok(metadata) if !metadata.is_dir() => {
                bail!("输出路径不是目录：{}", current.display());
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&current)
                    .with_context(|| format!("无法创建目录：{}", current.display()))?;
            }
            Err(error) => return Err(error.into()),
        }
        ensure_path_is_inside(&current, canonical_root)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(name: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be valid")
                .as_nanos();
            let path =
                std::env::temp_dir().join(format!("miaozip-{name}-{}-{nonce}", std::process::id()));
            fs::create_dir_all(&path).expect("temporary directory should be created");
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn round_trip_directory() {
        let temp = TestDirectory::new("round-trip");
        let source = temp.0.join("source");
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::write(source.join("hello.txt"), "hello").unwrap();
        fs::write(source.join("nested/world.txt"), "world").unwrap();

        let archive_path = temp.0.join("archive.zip");
        let compressed =
            create_archive(&[source], &archive_path, ArchiveFormat::Zip, 6, |_| {}).unwrap();
        assert_eq!(compressed.files, 2);
        let listing = list_archive_entries(&archive_path).unwrap();
        assert!(
            listing
                .iter()
                .any(|entry| entry.path == "source/nested/world.txt")
        );

        let extracted = temp.0.join("extracted");
        let decompressed = extract_archive(&archive_path, &extracted, |_| {}).unwrap();
        assert_eq!(decompressed.files, 2);
        assert_eq!(
            fs::read_to_string(extracted.join("source/hello.txt")).unwrap(),
            "hello"
        );
        assert_eq!(
            fs::read_to_string(extracted.join("source/nested/world.txt")).unwrap(),
            "world"
        );
    }

    #[test]
    fn zip_store_mode_does_not_compress_entries() {
        let temp = TestDirectory::new("zip-store");
        let source = temp.0.join("plain.txt");
        fs::write(&source, b"plain text").unwrap();
        let archive_path = temp.0.join("stored.zip");
        create_archive(&[source], &archive_path, ArchiveFormat::Zip, 0, |_| {}).unwrap();
        let mut archive = ZipArchive::new(File::open(archive_path).unwrap()).unwrap();
        let entry = archive.by_name("plain.txt").unwrap();
        assert_eq!(entry.compression(), CompressionMethod::Stored);
    }

    #[test]
    fn extracts_rar_archive() {
        // Tiny RAR fixture from the unrar crate's test data (one VERSION file).
        const RAR_HEX: &str = "526172211A0700CF907300000D000000000000000F0C7420802700150000000B0000000345F37DC6A48A07471D330700A481000056455253494F4E0C008FEC8A45CC23C848088362FE5FDD5C5388F072C43D7B00400700";
        let temp = TestDirectory::new("rar-extract");
        let archive_path = temp.0.join("sample.rar");
        let bytes: Vec<u8> = (0..RAR_HEX.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&RAR_HEX[index..index + 2], 16).unwrap())
            .collect();
        fs::write(&archive_path, bytes).unwrap();
        assert_eq!(
            list_archive_entries(&archive_path).unwrap()[0].path,
            "VERSION"
        );
        let destination = temp.0.join("out");
        let summary = extract_archive(&archive_path, &destination, |_| {}).unwrap();
        assert_eq!(summary.files, 1);
        assert_eq!(
            fs::read(destination.join("VERSION")).unwrap(),
            b"unrar-0.4.0"
        );
    }

    #[test]
    fn extracts_single_file_streams_and_recognizes_tar_aliases() {
        let temp = TestDirectory::new("more-formats");
        let payload = b"format coverage";
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gzip.write_all(payload).unwrap();
        let mut bzip = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::default());
        bzip.write_all(payload).unwrap();
        let mut xz = xz2::write::XzEncoder::new(Vec::new(), 6);
        xz.write_all(payload).unwrap();
        let cases = [
            ("sample.gz", gzip.finish().unwrap()),
            ("sample.bz2", bzip.finish().unwrap()),
            ("sample.xz", xz.finish().unwrap()),
            (
                "sample.zst",
                zstd::stream::encode_all(&payload[..], 6).unwrap(),
            ),
        ];
        for (name, bytes) in cases {
            let archive_path = temp.0.join(name);
            fs::write(&archive_path, bytes).unwrap();
            assert_eq!(
                list_archive_entries(&archive_path).unwrap()[0].path,
                "sample"
            );
            let destination = temp.0.join(format!("out-{name}"));
            let summary = extract_archive(&archive_path, &destination, |_| {}).unwrap();
            assert_eq!(summary.files, 1, "{name}");
            assert_eq!(fs::read(destination.join("sample")).unwrap(), payload);
        }
        for (name, format) in [
            ("sample.tgz", ArchiveFormat::TarGz),
            ("sample.tbz2", ArchiveFormat::TarBz2),
            ("sample.txz", ArchiveFormat::TarXz),
            ("sample.tzst", ArchiveFormat::TarZst),
        ] {
            assert_eq!(ArchiveFormat::from_path(Path::new(name)), Some(format));
        }
        let source = temp.0.join("alias.txt");
        fs::write(&source, payload).unwrap();
        let archive = temp.0.join("alias.tar.gz");
        create_archive(&[source], &archive, ArchiveFormat::TarGz, 6, |_| {}).unwrap();
        let alias = temp.0.join("alias.tgz");
        fs::rename(archive, &alias).unwrap();
        let destination = temp.0.join("alias-out");
        extract_archive(&alias, &destination, |_| {}).unwrap();
        assert_eq!(fs::read(destination.join("alias.txt")).unwrap(), payload);
    }

    #[test]
    fn rejects_parent_directory_traversal() {
        let temp = TestDirectory::new("zip-slip");
        let archive_path = temp.0.join("unsafe.zip");
        let output = File::create(&archive_path).unwrap();
        let mut writer = ZipWriter::new(output);
        writer
            .start_file("../escape.txt", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"not outside").unwrap();
        writer.finish().unwrap();

        let extracted = temp.0.join("extracted");
        assert!(extract_archive(&archive_path, &extracted, |_| {}).is_err());
        assert!(!temp.0.join("escape.txt").exists());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_existing_symlink_directory_before_writing() {
        use std::os::unix::fs::symlink;

        let temp = TestDirectory::new("symlink-dir");
        let archive_path = temp.0.join("unsafe.zip");
        let mut writer = ZipWriter::new(File::create(&archive_path).unwrap());
        writer
            .start_file("link/escape.txt", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"no escape").unwrap();
        writer.finish().unwrap();

        let destination = temp.0.join("out");
        let outside = temp.0.join("outside");
        fs::create_dir_all(&destination).unwrap();
        fs::create_dir_all(&outside).unwrap();
        symlink(&outside, destination.join("link")).unwrap();
        assert!(extract_archive(&archive_path, &destination, |_| {}).is_err());
        assert!(!outside.join("escape.txt").exists());
    }

    #[test]
    fn round_trips_every_supported_format() {
        let temp = TestDirectory::new("formats");
        let source = temp.0.join("data");
        fs::create_dir_all(source.join("empty")).unwrap();
        fs::write(source.join("你好.txt"), b"archive test").unwrap();
        for format in ArchiveFormat::CREATABLE {
            let archive_path = temp.0.join(format!("sample{}", format.extension()));
            let created = create_archive(
                std::slice::from_ref(&source),
                &archive_path,
                format,
                6,
                |_| {},
            )
            .unwrap();
            assert_eq!(created.files, 1, "{}", format.label());
            let listing = list_archive_entries(&archive_path).unwrap();
            assert!(
                listing.iter().any(|entry| entry.path == "data/你好.txt"),
                "{} listing was empty",
                format.label()
            );
            let destination = temp.0.join(format!("out-{}", format.label()));
            let extracted = extract_archive(&archive_path, &destination, |_| {}).unwrap();
            assert_eq!(extracted.files, 1, "{}", format.label());
            assert_eq!(
                fs::read(destination.join("data/你好.txt")).unwrap(),
                b"archive test"
            );
            assert!(destination.join("data/empty").is_dir());
        }
    }

    #[test]
    fn rejects_unsafe_archive_names() {
        for name in [
            "../escape",
            "/absolute",
            "C:/windows",
            "sub\\..\\escape",
            "",
        ] {
            assert!(safe_archive_name(name).is_err(), "{name:?}");
        }
        assert_eq!(
            safe_archive_name("./folder/file.txt").unwrap(),
            PathBuf::from("folder/file.txt")
        );
    }

    #[test]
    fn rejects_mismatched_output_format() {
        let temp = TestDirectory::new("mismatch");
        let source = temp.0.join("a.txt");
        fs::write(&source, b"a").unwrap();
        assert!(
            create_archive(
                &[source],
                &temp.0.join("wrong.zip"),
                ArchiveFormat::SevenZip,
                6,
                |_| {}
            )
            .is_err()
        );
        assert!(!temp.0.join("wrong.zip").exists());
    }

    #[test]
    fn refuses_tar_links_before_creating_destination() {
        let temp = TestDirectory::new("tar-link");
        let archive_path = temp.0.join("link.tar");
        let mut builder = tar::Builder::new(File::create(&archive_path).unwrap());
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_cksum();
        builder.append_link(&mut header, "link", "outside").unwrap();
        builder.finish().unwrap();
        drop(builder);
        let destination = temp.0.join("out");
        assert!(extract_archive(&archive_path, &destination, |_| {}).is_err());
        assert!(!destination.exists());
    }
}
