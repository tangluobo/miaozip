use std::io;
use std::path::Path;

pub fn open_file(path: &Path) -> io::Result<()> {
    if !path.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("文件不存在：{}", path.display()),
        ));
    }
    open_with_platform(path)
}

#[cfg(windows)]
fn open_with_platform(path: &Path) -> io::Result<()> {
    use std::ffi::{OsStr, c_void};
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "Shell32")]
    unsafe extern "system" {
        fn ShellExecuteW(
            window: *mut c_void,
            operation: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show: i32,
        ) -> isize;
    }

    let operation: Vec<u16> = OsStr::new("open").encode_wide().chain(Some(0)).collect();
    let readable = crate::app::display_path(path);
    let file: Vec<u16> = OsStr::new(&readable).encode_wide().chain(Some(0)).collect();
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
        )
    };
    if result > 32 {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "Windows 未能用默认应用打开文件（ShellExecuteW 代码 {result}）"
        )))
    }
}

#[cfg(target_os = "macos")]
fn open_with_platform(path: &Path) -> io::Result<()> {
    std::process::Command::new("open")
        .arg(path)
        .spawn()
        .map(|_| ())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_with_platform(path: &Path) -> io::Result<()> {
    std::process::Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map(|_| ())
}

#[cfg(not(any(windows, unix)))]
fn open_with_platform(_path: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "当前平台尚不支持打开系统默认应用",
    ))
}
