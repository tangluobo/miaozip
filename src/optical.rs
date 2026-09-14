use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[cfg(any(target_os = "linux", target_os = "macos"))]
use anyhow::anyhow;
use anyhow::{Context, Result, bail};

#[derive(Debug, Clone)]
pub struct MountedImage {
    pub image: PathBuf,
    pub device: Option<String>,
    pub location: String,
}

pub fn mount_iso(path: &Path) -> Result<MountedImage> {
    if !path.is_file()
        || !path
            .extension()
            .is_some_and(|value| value.eq_ignore_ascii_case("iso"))
    {
        bail!("请选择存在的 .iso 光盘镜像");
    }
    let image = path.canonicalize().context("无法解析 ISO 路径")?;
    mount_platform(image)
}

pub fn unmount_iso(mounted: &MountedImage) -> Result<()> {
    unmount_platform(mounted)
}

fn check_output(output: Output, operation: &str) -> Result<String> {
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        bail!(
            "{operation}失败：{}",
            if detail.is_empty() {
                "系统命令未成功执行"
            } else {
                &detail
            }
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[cfg(windows)]
fn powershell(script: &str, image: &Path) -> Result<String> {
    use std::os::windows::process::CommandExt;
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("MIAOZIP_ISO_PATH", image)
        .creation_flags(0x0800_0000)
        .output()
        .context("无法启动 Windows PowerShell")?;
    check_output(output, "虚拟光驱操作")
}

#[cfg(windows)]
fn mount_platform(image: PathBuf) -> Result<MountedImage> {
    let output = powershell(
        "$ErrorActionPreference='Stop'; [Console]::OutputEncoding=[Text.Encoding]::UTF8; $p=$env:MIAOZIP_ISO_PATH; Mount-DiskImage -ImagePath $p -StorageType ISO -ErrorAction Stop | Out-Null; try { $v=Get-DiskImage -ImagePath $p | Get-Volume; if ($v -and $v.DriveLetter) { Write-Output ($v.DriveLetter + ':\\') } } catch { }",
        &image,
    )?;
    Ok(MountedImage {
        image,
        device: None,
        location: if output.is_empty() {
            "已挂载；请刷新磁盘列表".to_owned()
        } else {
            output
        },
    })
}

#[cfg(windows)]
fn unmount_platform(mounted: &MountedImage) -> Result<()> {
    powershell(
        "$ErrorActionPreference='Stop'; Dismount-DiskImage -ImagePath $env:MIAOZIP_ISO_PATH -StorageType ISO -ErrorAction Stop | Out-Null",
        &mounted.image,
    )?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn mount_platform(image: PathBuf) -> Result<MountedImage> {
    let setup = check_output(
        Command::new("udisksctl")
            .args(["loop-setup", "--read-only", "--file"])
            .arg(&image)
            .output()
            .context("请安装 udisks2（udisksctl）")?,
        "创建只读虚拟光驱",
    )?;
    let device = parse_device(&setup, "/dev/loop")
        .ok_or_else(|| anyhow!("无法识别 udisksctl 返回的 loop 设备：{setup}"))?;
    let mount = Command::new("udisksctl")
        .args(["mount", "--block-device", &device])
        .output();
    let location = match mount {
        Ok(output) => match check_output(output, "挂载 ISO") {
            Ok(text) => text,
            Err(error) => {
                let _ = Command::new("udisksctl")
                    .args(["loop-delete", "--block-device", &device])
                    .output();
                return Err(error);
            }
        },
        Err(error) => {
            let _ = Command::new("udisksctl")
                .args(["loop-delete", "--block-device", &device])
                .output();
            return Err(error).context("无法启动 udisksctl");
        }
    };
    Ok(MountedImage {
        image,
        device: Some(device),
        location,
    })
}

#[cfg(target_os = "linux")]
fn unmount_platform(mounted: &MountedImage) -> Result<()> {
    let device = mounted
        .device
        .as_deref()
        .ok_or_else(|| anyhow!("缺少 loop 设备信息"))?;
    check_output(
        Command::new("udisksctl")
            .args(["unmount", "--block-device", device])
            .output()?,
        "卸载 ISO",
    )?;
    check_output(
        Command::new("udisksctl")
            .args(["loop-delete", "--block-device", device])
            .output()?,
        "移除 loop 设备",
    )?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn mount_platform(image: PathBuf) -> Result<MountedImage> {
    let output = check_output(
        Command::new("hdiutil")
            .args(["attach", "-readonly"])
            .arg(&image)
            .output()
            .context("无法启动 hdiutil")?,
        "挂载 ISO",
    )?;
    let device = parse_device(&output, "/dev/disk")
        .ok_or_else(|| anyhow!("无法识别 hdiutil 返回的设备：{output}"))?;
    let location = output
        .lines()
        .find_map(|line| {
            line.split_whitespace()
                .last()
                .filter(|part| part.starts_with("/Volumes/"))
                .map(str::to_owned)
        })
        .unwrap_or_else(|| device.clone());
    Ok(MountedImage {
        image,
        device: Some(device),
        location,
    })
}

#[cfg(target_os = "macos")]
fn unmount_platform(mounted: &MountedImage) -> Result<()> {
    let device = mounted
        .device
        .as_deref()
        .ok_or_else(|| anyhow!("缺少磁盘设备信息"))?;
    check_output(
        Command::new("hdiutil").args(["detach", device]).output()?,
        "卸载 ISO",
    )?;
    Ok(())
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
fn mount_platform(_image: PathBuf) -> Result<MountedImage> {
    bail!("此操作系统暂不支持虚拟光驱")
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
fn unmount_platform(_mounted: &MountedImage) -> Result<()> {
    bail!("此操作系统暂不支持虚拟光驱")
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn parse_device(output: &str, prefix: &str) -> Option<String> {
    output.split_whitespace().find_map(|part| {
        let suffix = part.strip_prefix(prefix)?;
        let numeric: String = suffix
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == 's')
            .collect();
        if numeric.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            Some(format!("{prefix}{numeric}"))
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_missing_or_non_iso_file() {
        assert!(mount_iso(Path::new("missing.iso")).is_err());
        assert!(mount_iso(Path::new("missing.zip")).is_err());
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn parses_device_names() {
        assert_eq!(
            parse_device("Mapped file as /dev/loop12.", "/dev/loop"),
            Some("/dev/loop12".to_owned())
        );
        assert_eq!(
            parse_device("已映射为 /dev/loop12。", "/dev/loop"),
            Some("/dev/loop12".to_owned())
        );
    }
}
