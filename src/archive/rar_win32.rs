//! Correct UnRAR ABI adapter for 32-bit Windows.
//!
//! `unrar_sys` 0.5.8 declares these PASCAL functions with the C ABI. That is
//! harmless on 64-bit targets, but gives the wrong symbol names and calling
//! convention on Win32. Keep the upstream data structures and static library,
//! while declaring the small API surface MiaoZip uses as `extern "system"`.

use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::Path;
use std::ptr::NonNull;

use anyhow::{Result, anyhow};

const FLAG_ENCRYPTED: u32 = 1 << 2;
const FLAG_DIRECTORY: u32 = 1 << 5;

unsafe extern "system" {
    fn RAROpenArchiveEx(data: *mut unrar_sys::OpenArchiveDataEx) -> *mut unrar_sys::Handle;
    fn RARCloseArchive(handle: *const unrar_sys::Handle) -> i32;
    fn RARReadHeaderEx(
        handle: *const unrar_sys::Handle,
        header: *mut unrar_sys::HeaderDataEx,
    ) -> i32;
    fn RARProcessFileW(
        handle: *const unrar_sys::Handle,
        operation: i32,
        destination_path: *const unrar_sys::WCHAR,
        destination_name: *const unrar_sys::WCHAR,
    ) -> i32;
}

#[derive(Debug, Clone)]
pub(super) struct Header {
    pub(super) name: String,
    pub(super) is_directory: bool,
    pub(super) is_encrypted: bool,
    pub(super) is_redirection: bool,
    pub(super) unpacked_size: u64,
}

pub(super) struct Archive {
    handle: NonNull<unrar_sys::Handle>,
}

impl Archive {
    pub(super) fn open_for_listing(path: &Path) -> Result<Self> {
        Self::open(path, unrar_sys::RAR_OM_LIST)
    }

    pub(super) fn open_for_processing(path: &Path) -> Result<Self> {
        Self::open(path, unrar_sys::RAR_OM_EXTRACT)
    }

    fn open(path: &Path, mode: u32) -> Result<Self> {
        let wide_path = wide(path);
        let mut data = unrar_sys::OpenArchiveDataEx::new(wide_path.as_ptr(), mode);
        // SAFETY: `data` and the NUL-terminated path remain alive for the call.
        let handle = unsafe { RAROpenArchiveEx(&mut data) };
        NonNull::new(handle)
            .map(|handle| Self { handle })
            .ok_or_else(|| rar_error("无法打开 RAR 压缩包", data.open_result as i32))
    }

    pub(super) fn read_header(&mut self) -> Result<Option<Header>> {
        let mut raw = unrar_sys::HeaderDataEx::default();
        // SAFETY: the handle is owned by `self` and `raw` is writable.
        let code = unsafe { RARReadHeaderEx(self.handle.as_ptr(), &mut raw) };
        match code {
            unrar_sys::ERAR_SUCCESS => {
                let end = raw
                    .filename_w
                    .iter()
                    .position(|unit| *unit == 0)
                    .unwrap_or(raw.filename_w.len());
                let units = raw.filename_w[..end]
                    .iter()
                    .map(|unit| *unit as u16)
                    .collect::<Vec<_>>();
                let name = OsString::from_wide(&units).to_string_lossy().into_owned();
                Ok(Some(Header {
                    name,
                    is_directory: raw.flags & FLAG_DIRECTORY != 0,
                    is_encrypted: raw.flags & FLAG_ENCRYPTED != 0,
                    is_redirection: raw.redir_type != 0,
                    unpacked_size: ((raw.unp_size_high as u64) << 32) | raw.unp_size as u64,
                }))
            }
            unrar_sys::ERAR_END_ARCHIVE => Ok(None),
            code => Err(rar_error("无法读取 RAR 文件头", code)),
        }
    }

    pub(super) fn skip(&mut self) -> Result<()> {
        self.process(unrar_sys::RAR_SKIP, None, "无法跳过 RAR 项")
    }

    pub(super) fn extract_to(&mut self, destination: &Path) -> Result<()> {
        self.process(unrar_sys::RAR_EXTRACT, Some(destination), "无法解压 RAR 项")
    }

    fn process(&mut self, operation: i32, destination: Option<&Path>, action: &str) -> Result<()> {
        let destination = destination.map(wide);
        let destination_name = destination
            .as_ref()
            .map_or(std::ptr::null(), |path| path.as_ptr());
        // SAFETY: the handle is valid and the optional destination is a live,
        // NUL-terminated UTF-16 string for the duration of this call.
        let code = unsafe {
            RARProcessFileW(
                self.handle.as_ptr(),
                operation,
                std::ptr::null(),
                destination_name,
            )
        };
        if code == unrar_sys::ERAR_SUCCESS {
            Ok(())
        } else {
            Err(rar_error(action, code))
        }
    }
}

impl Drop for Archive {
    fn drop(&mut self) {
        // SAFETY: this is the unique live handle and it is closed exactly once.
        unsafe {
            RARCloseArchive(self.handle.as_ptr());
        }
    }
}

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

fn rar_error(action: &str, code: i32) -> anyhow::Error {
    let reason = match code {
        unrar_sys::ERAR_NO_MEMORY => "内存不足",
        unrar_sys::ERAR_BAD_DATA => "数据或校验和损坏",
        unrar_sys::ERAR_BAD_ARCHIVE => "压缩包损坏",
        unrar_sys::ERAR_UNKNOWN_FORMAT => "不是可识别的 RAR 格式",
        unrar_sys::ERAR_EOPEN => "无法打开文件或分卷",
        unrar_sys::ERAR_ECREATE => "无法创建输出文件",
        unrar_sys::ERAR_ECLOSE => "无法关闭文件",
        unrar_sys::ERAR_EREAD => "读取失败",
        unrar_sys::ERAR_EWRITE => "写入失败",
        unrar_sys::ERAR_MISSING_PASSWORD => "缺少密码",
        unrar_sys::ERAR_BAD_PASSWORD => "密码错误",
        _ => "未知 UnRAR 错误",
    };
    anyhow!("{action}：{reason}（错误码 {code}）")
}
