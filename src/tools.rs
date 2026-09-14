use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use md5::{Digest, Md5};
use sha2::Sha256;

#[derive(Debug, Clone)]
pub struct FileHashes {
    pub md5: String,
    pub sha256: String,
}

pub fn hash_file(path: &Path) -> Result<FileHashes> {
    let mut file = File::open(path).with_context(|| format!("无法读取 {}", path.display()))?;
    let before = file.metadata()?;
    let mut md5 = Md5::new();
    let mut sha256 = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        md5.update(&buffer[..count]);
        sha256.update(&buffer[..count]);
    }
    let after = file.metadata()?;
    if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
        bail!("文件在计算过程中发生变化，请重新校验");
    }
    Ok(FileHashes {
        md5: hex(&md5.finalize()),
        sha256: hex(&sha256.finalize()),
    })
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        value.push(DIGITS[(byte >> 4) as usize] as char);
        value.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    value
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RenameMode {
    #[default]
    Numbered,
    Replace,
    Affix,
}

#[derive(Debug, Clone)]
pub struct RenameOptions {
    pub mode: RenameMode,
    pub pattern: String,
    pub start: u32,
    pub find: String,
    pub replacement: String,
    pub prefix: String,
    pub suffix: String,
}

impl Default for RenameOptions {
    fn default() -> Self {
        Self {
            mode: RenameMode::Numbered,
            pattern: "文件_{n}".to_owned(),
            start: 1,
            find: String::new(),
            replacement: String::new(),
            prefix: String::new(),
            suffix: String::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RenamePreview {
    pub source: PathBuf,
    pub target: PathBuf,
}

pub fn preview_rename(paths: &[PathBuf], options: &RenameOptions) -> Result<Vec<RenamePreview>> {
    if paths.is_empty() {
        bail!("请先添加文件或文件夹");
    }
    if options.mode == RenameMode::Replace && options.find.is_empty() {
        bail!("请填写要替换的字符");
    }
    if options.mode == RenameMode::Numbered && !options.pattern.contains("{n}") {
        bail!("编号规则必须包含 {{n}}");
    }
    if options.mode == RenameMode::Affix && options.prefix.is_empty() && options.suffix.is_empty() {
        bail!("请填写前缀或后缀");
    }
    let mut seen_sources = HashSet::new();
    let mut seen_targets = HashSet::new();
    let mut plan = Vec::with_capacity(paths.len());
    for (index, source) in paths.iter().enumerate() {
        let metadata =
            fs::symlink_metadata(source).with_context(|| format!("找不到 {}", source.display()))?;
        if metadata.file_type().is_symlink() {
            bail!("暂不支持重命名符号链接：{}", source.display());
        }
        let parent = source.parent().ok_or_else(|| anyhow!("路径缺少父目录"))?;
        let (stem, extension) = if metadata.is_dir() {
            (
                source
                    .file_name()
                    .and_then(|value| value.to_str())
                    .ok_or_else(|| anyhow!("文件夹名不是有效的 Unicode"))?,
                "",
            )
        } else {
            (
                source
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .ok_or_else(|| anyhow!("文件名不是有效的 Unicode"))?,
                source
                    .extension()
                    .and_then(|value| value.to_str())
                    .unwrap_or(""),
            )
        };
        let new_stem = match options.mode {
            RenameMode::Numbered => options.pattern.replace(
                "{n}",
                &options
                    .start
                    .checked_add(index as u32)
                    .ok_or_else(|| anyhow!("编号超出范围"))?
                    .to_string(),
            ),
            RenameMode::Replace => stem.replace(&options.find, &options.replacement),
            RenameMode::Affix => format!("{}{}{}", options.prefix, stem, options.suffix),
        };
        validate_name(&new_stem)?;
        let new_name = if extension.is_empty() {
            new_stem
        } else {
            format!("{new_stem}.{extension}")
        };
        let target = parent.join(new_name);
        let source_key = path_key(source);
        let target_key = path_key(&target);
        if !seen_sources.insert(source_key) {
            bail!("输入列表中有重复路径：{}", source.display());
        }
        if !seen_targets.insert(target_key) {
            bail!("多个项目会得到相同名称：{}", target.display());
        }
        if target != *source && target.exists() {
            bail!("目标已存在，不会覆盖：{}", target.display());
        }
        plan.push(RenamePreview {
            source: source.clone(),
            target,
        });
    }
    for (index, first) in paths.iter().enumerate() {
        if paths
            .iter()
            .skip(index + 1)
            .any(|second| second.starts_with(first) || first.starts_with(second))
        {
            bail!("不能同时重命名文件夹及其内部项目");
        }
    }
    Ok(plan)
}

fn path_key(path: &Path) -> String {
    let value = path.to_string_lossy();
    if cfg!(windows) {
        value.to_lowercase()
    } else {
        value.into_owned()
    }
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.ends_with([' ', '.'])
        || name
            .chars()
            .any(|ch| ch.is_control() || "\\/:*?\"<>|".contains(ch))
    {
        bail!("生成的文件名无效：{name}");
    }
    #[cfg(windows)]
    {
        let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
        if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || ((stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.len() == 4
                && stem.as_bytes()[3].is_ascii_digit()
                && stem.as_bytes()[3] != b'0')
        {
            bail!("Windows 保留文件名不能使用：{name}");
        }
    }
    Ok(())
}

pub fn execute_rename(plan: &[RenamePreview]) -> Result<usize> {
    let mut completed: Vec<&RenamePreview> = Vec::new();
    for item in plan {
        if item.source == item.target {
            continue;
        }
        let result = if item.target.exists() {
            Err(anyhow!("目标已存在，不会覆盖：{}", item.target.display()))
        } else {
            fs::rename(&item.source, &item.target).map_err(anyhow::Error::from)
        };
        if let Err(error) = result {
            let mut rollback_errors = Vec::new();
            for previous in completed.iter().rev() {
                if previous.source.exists() {
                    rollback_errors.push(previous.source.display().to_string());
                } else if let Err(rollback_error) = fs::rename(&previous.target, &previous.source) {
                    rollback_errors
                        .push(format!("{}：{rollback_error}", previous.source.display()));
                }
            }
            bail!(
                "重命名失败：{}：{error}{}",
                item.source.display(),
                if rollback_errors.is_empty() {
                    String::new()
                } else {
                    format!("；部分回滚失败：{}", rollback_errors.join("、"))
                }
            );
        }
        completed.push(item);
    }
    Ok(completed.len())
}

#[derive(Debug, Clone)]
pub struct TextReplacePreview {
    pub source: PathBuf,
    pub output: PathBuf,
    pub matches: usize,
}

const MAX_TEXT_BYTES: u64 = 32 * 1024 * 1024;

pub fn preview_text_replace(paths: &[PathBuf], find: &str) -> Result<Vec<TextReplacePreview>> {
    if paths.is_empty() {
        bail!("请先添加文本文件");
    }
    if find.is_empty() {
        bail!("请填写要查找的字符");
    }
    let mut seen = HashSet::new();
    let mut plan = Vec::with_capacity(paths.len());
    for source in paths {
        let metadata = fs::symlink_metadata(source)?;
        if !metadata.is_file() || metadata.len() > MAX_TEXT_BYTES {
            bail!("仅支持不超过 32 MiB 的普通文本文件：{}", source.display());
        }
        let content = fs::read_to_string(source)
            .with_context(|| format!("只支持 UTF-8 文本：{}", source.display()))?;
        let output = replaced_path(source)?;
        if output.exists() {
            bail!("输出文件已存在，不会覆盖：{}", output.display());
        }
        if !seen.insert(path_key(&output)) {
            bail!("多个文件会生成相同输出路径：{}", output.display());
        }
        plan.push(TextReplacePreview {
            source: source.clone(),
            output,
            matches: content.matches(find).count(),
        });
    }
    Ok(plan)
}

fn replaced_path(source: &Path) -> Result<PathBuf> {
    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("文件名不是有效的 Unicode"))?;
    let extension = source.extension().and_then(|value| value.to_str());
    let name = match extension {
        Some(extension) => format!("{stem}.replaced.{extension}"),
        None => format!("{stem}.replaced"),
    };
    Ok(source.with_file_name(name))
}

pub fn execute_text_replace(
    plan: &[TextReplacePreview],
    find: &str,
    replacement: &str,
) -> Result<usize> {
    if find.is_empty() {
        bail!("请填写要查找的字符");
    }
    let mut written = 0;
    for item in plan {
        if item.matches == 0 {
            continue;
        }
        let metadata = fs::symlink_metadata(&item.source)?;
        if !metadata.is_file() || metadata.len() > MAX_TEXT_BYTES {
            bail!("原文件已变化：{}", item.source.display());
        }
        let content = fs::read_to_string(&item.source)?;
        if content.matches(find).count() != item.matches {
            bail!("文件内容已变化，请重新生成预览：{}", item.source.display());
        }
        let updated = content.replace(find, replacement);
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&item.output)
            .with_context(|| format!("输出文件已存在或无法写入：{}", item.output.display()))?;
        if let Err(error) = output.write_all(updated.as_bytes()) {
            drop(output);
            let _ = fs::remove_file(&item.output);
            return Err(error).with_context(|| format!("写入失败：{}", item.output.display()));
        }
        written += 1;
    }
    Ok(written)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ImageOutputFormat {
    #[default]
    Png,
    Jpeg,
    WebP,
    Bmp,
}

impl ImageOutputFormat {
    pub fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::WebP => "WebP",
            Self::Bmp => "BMP",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::WebP => "webp",
            Self::Bmp => "bmp",
        }
    }

    fn image_format(self) -> image::ImageFormat {
        match self {
            Self::Png => image::ImageFormat::Png,
            Self::Jpeg => image::ImageFormat::Jpeg,
            Self::WebP => image::ImageFormat::WebP,
            Self::Bmp => image::ImageFormat::Bmp,
        }
    }
}

pub fn convert_images(paths: &[PathBuf], format: ImageOutputFormat) -> Result<usize> {
    if paths.is_empty() {
        bail!("请先添加图片");
    }
    let mut targets = HashSet::new();
    let mut plan = Vec::with_capacity(paths.len());
    for source in paths {
        let metadata = fs::symlink_metadata(source)?;
        if !metadata.is_file() || metadata.len() > 64 * 1024 * 1024 {
            bail!("仅支持不超过 64 MiB 的普通图片：{}", source.display());
        }
        let stem = source
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| anyhow!("图片文件名不是有效的 Unicode"))?;
        let target = source.with_file_name(format!("{stem}.converted.{}", format.extension()));
        if target.exists() || !targets.insert(path_key(&target)) {
            bail!("输出文件已存在或冲突，不会覆盖：{}", target.display());
        }
        plan.push((source, target));
    }
    let mut converted = 0;
    for (source, target) in plan {
        let (width, height) = image::image_dimensions(source)
            .with_context(|| format!("无法识别图片：{}", source.display()))?;
        if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 36_000_000 {
            bail!("图片像素过大：{}", source.display());
        }
        let picture = image::ImageReader::open(source)?.decode()?;
        let parent = target
            .parent()
            .ok_or_else(|| anyhow!("输出路径缺少父目录"))?;
        let temporary = tempfile::NamedTempFile::new_in(parent)?;
        picture.save_with_format(temporary.path(), format.image_format())?;
        temporary
            .persist_noclobber(&target)
            .with_context(|| format!("输出文件已存在或无法写入：{}", target.display()))?;
        converted += 1;
    }
    Ok(converted)
}

#[derive(Debug, Clone)]
pub struct ZipTestResult {
    pub entries: usize,
    pub unpacked_bytes: u64,
}

pub fn test_zip_archive(path: &Path) -> Result<ZipTestResult> {
    let source = File::open(path).with_context(|| format!("无法读取 {}", path.display()))?;
    let mut archive = zip::ZipArchive::new(source).context("不是有效的 ZIP 压缩包")?;
    let mut unpacked_bytes = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        unpacked_bytes = unpacked_bytes
            .checked_add(entry.size())
            .ok_or_else(|| anyhow!("解压后体积超出范围"))?;
        if unpacked_bytes > 2 * 1024 * 1024 * 1024 {
            bail!("为防止压缩炸弹，仅测试解压后总量不超过 2 GiB 的 ZIP");
        }
        let expected_crc = entry.crc32();
        let expected_size = entry.size();
        let mut digest = crc32fast::Hasher::new();
        let mut actual_size = 0_u64;
        let mut buffer = [0_u8; 128 * 1024];
        loop {
            let count = entry
                .read(&mut buffer)
                .with_context(|| format!("条目校验失败：{}", entry.name()))?;
            if count == 0 {
                break;
            }
            digest.update(&buffer[..count]);
            actual_size += count as u64;
            if actual_size > expected_size {
                bail!("条目解压大小超过声明值：{}", entry.name());
            }
        }
        if actual_size != expected_size || digest.finalize() != expected_crc {
            bail!("条目大小或 CRC32 校验失败：{}", entry.name());
        }
    }
    Ok(ZipTestResult {
        entries: archive.len(),
        unpacked_bytes,
    })
}

#[derive(Debug, Clone)]
pub struct ZipRepairResult {
    pub recovered: usize,
    pub skipped: usize,
    pub output: PathBuf,
}

pub fn repair_readable_zip(path: &Path) -> Result<ZipRepairResult> {
    let source = File::open(path).with_context(|| format!("无法读取 {}", path.display()))?;
    let mut archive = zip::ZipArchive::new(source)
        .context("ZIP 中央目录无法读取；此工具无法重建完全损坏的目录")?;
    let parent = path.parent().ok_or_else(|| anyhow!("原包路径缺少父目录"))?;
    let stem = path
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("原包文件名不是有效的 Unicode"))?;
    let output = parent.join(format!("{stem}.recovered.zip"));
    if output.exists() {
        bail!("恢复包已存在，不会覆盖：{}", output.display());
    }
    let temporary = tempfile::NamedTempFile::new_in(parent)?;
    let mut writer = zip::ZipWriter::new(temporary.reopen()?);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let mut recovered = 0;
    let mut skipped = 0;
    let mut total_bytes = 0_u64;
    let mut names = HashSet::new();
    for index in 0..archive.len() {
        let Ok(mut entry) = archive.by_index(index) else {
            skipped += 1;
            continue;
        };
        let name = entry.name().to_owned();
        if !safe_zip_entry_name(&name) || !names.insert(name.clone()) {
            skipped += 1;
            continue;
        }
        if entry.is_dir() {
            writer.add_directory(name, zip::write::SimpleFileOptions::default())?;
            recovered += 1;
            continue;
        }
        if entry.size() > 64 * 1024 * 1024
            || total_bytes.saturating_add(entry.size()) > 512 * 1024 * 1024
        {
            skipped += 1;
            continue;
        }
        let expected_size = entry.size();
        let expected_crc = entry.crc32();
        let mut data = Vec::with_capacity(expected_size as usize);
        let read_ok = (&mut entry)
            .take(64 * 1024 * 1024 + 1)
            .read_to_end(&mut data)
            .is_ok();
        let mut digest = crc32fast::Hasher::new();
        digest.update(&data);
        if !read_ok || data.len() as u64 != expected_size || digest.finalize() != expected_crc {
            skipped += 1;
            continue;
        }
        writer.start_file(name, options)?;
        writer.write_all(&data)?;
        total_bytes += data.len() as u64;
        recovered += 1;
    }
    if recovered == 0 {
        bail!("未找到可恢复的有效条目");
    }
    writer.finish()?;
    temporary
        .persist_noclobber(&output)
        .with_context(|| format!("恢复包已存在或无法写入：{}", output.display()))?;
    Ok(ZipRepairResult {
        recovered,
        skipped,
        output,
    })
}

fn safe_zip_entry_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && !name.starts_with('\\')
        && !name.chars().any(|ch| matches!(ch, '\\' | ':' | '\0'))
        && name.split('/').all(|part| part != "..")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_known_content() {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("test.txt");
        fs::write(&file, b"abc").unwrap();
        let hashes = hash_file(&file).unwrap();
        assert_eq!(hashes.md5, "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            hashes.sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn rename_previews_and_rejects_existing_targets() {
        let folder = tempfile::tempdir().unwrap();
        let first = folder.path().join("alpha.txt");
        let second = folder.path().join("beta.txt");
        fs::write(&first, "a").unwrap();
        fs::write(&second, "b").unwrap();
        let options = RenameOptions::default();
        let plan = preview_rename(&[first.clone(), second.clone()], &options).unwrap();
        assert_eq!(plan[0].target.file_name().unwrap(), "文件_1.txt");
        assert_eq!(execute_rename(&plan).unwrap(), 2);
        assert!(!first.exists());
        assert!(plan[0].target.exists());
        let third = folder.path().join("gamma.txt");
        fs::write(&third, "c").unwrap();
        assert!(preview_rename(&[third], &options).is_err());
    }

    #[test]
    fn rename_rolls_back_when_later_target_appears() {
        let folder = tempfile::tempdir().unwrap();
        let first = folder.path().join("alpha.txt");
        let second = folder.path().join("beta.txt");
        fs::write(&first, "a").unwrap();
        fs::write(&second, "b").unwrap();
        let plan =
            preview_rename(&[first.clone(), second.clone()], &RenameOptions::default()).unwrap();
        fs::write(&plan[1].target, "other").unwrap();
        assert!(execute_rename(&plan).is_err());
        assert!(first.exists());
        assert!(second.exists());
        assert!(!plan[0].target.exists());
        assert_eq!(fs::read_to_string(&plan[1].target).unwrap(), "other");
    }

    #[test]
    fn text_replace_preserves_original() {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("notes.txt");
        fs::write(&file, "foo and foo").unwrap();
        let plan = preview_text_replace(&[file.clone()], "foo").unwrap();
        assert_eq!(plan[0].matches, 2);
        assert_eq!(execute_text_replace(&plan, "foo", "bar").unwrap(), 1);
        assert_eq!(fs::read_to_string(&file).unwrap(), "foo and foo");
        assert_eq!(fs::read_to_string(&plan[0].output).unwrap(), "bar and bar");
    }

    #[test]
    fn converts_png_to_jpeg_without_overwriting_original() {
        let folder = tempfile::tempdir().unwrap();
        let source = folder.path().join("sample.png");
        image::DynamicImage::new_rgb8(2, 3).save(&source).unwrap();
        assert_eq!(
            convert_images(&[source.clone()], ImageOutputFormat::Jpeg).unwrap(),
            1
        );
        let target = folder.path().join("sample.converted.jpg");
        assert!(source.exists());
        assert_eq!(image::image_dimensions(target).unwrap(), (2, 3));
    }

    #[test]
    fn detects_damaged_zip_entry() {
        use zip::write::SimpleFileOptions;
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("test.zip");
        let output = File::create(&path).unwrap();
        let mut writer = zip::ZipWriter::new(output);
        writer
            .start_file("hello.txt", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"hello").unwrap();
        writer.finish().unwrap();
        assert_eq!(test_zip_archive(&path).unwrap().entries, 1);
        let mut bytes = fs::read(&path).unwrap();
        let name_len = u16::from_le_bytes([bytes[26], bytes[27]]) as usize;
        let extra_len = u16::from_le_bytes([bytes[28], bytes[29]]) as usize;
        let offset = 30 + name_len + extra_len;
        bytes[offset] = b'j';
        fs::write(&path, bytes).unwrap();
        assert!(test_zip_archive(&path).is_err());
    }

    #[test]
    fn repairs_zip_by_skipping_corrupted_entry() {
        use zip::write::SimpleFileOptions;
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("broken.zip");
        let output = File::create(&path).unwrap();
        let mut writer = zip::ZipWriter::new(output);
        writer
            .start_file("bad.txt", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"damage-me").unwrap();
        writer
            .start_file("good.txt", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"keep-me").unwrap();
        writer.finish().unwrap();
        let mut bytes = fs::read(&path).unwrap();
        let name_len = u16::from_le_bytes([bytes[26], bytes[27]]) as usize;
        let extra_len = u16::from_le_bytes([bytes[28], bytes[29]]) as usize;
        bytes[30 + name_len + extra_len] ^= 1;
        fs::write(&path, bytes).unwrap();
        let result = repair_readable_zip(&path).unwrap();
        assert_eq!(result.recovered, 1);
        assert_eq!(result.skipped, 1);
        let mut archive = zip::ZipArchive::new(File::open(result.output).unwrap()).unwrap();
        assert!(archive.by_name("bad.txt").is_err());
        let mut good = String::new();
        archive
            .by_name("good.txt")
            .unwrap()
            .read_to_string(&mut good)
            .unwrap();
        assert_eq!(good, "keep-me");
    }
}
