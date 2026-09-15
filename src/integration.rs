use crate::archive::ArchiveFormat;
use std::ffi::OsString;
use std::io;
use std::path::PathBuf;

// Windows associates the last extension only: .tar.gz uses .gz, while .tgz
// can have its own entry. Keep this list limited to formats we can open.
const FILE_ASSOCIATIONS: [(&str, &str, &str); 13] = [
    (".zip", "MiaoZip.Zip", "妙压 ZIP 压缩文件"),
    (".7z", "MiaoZip.SevenZip", "妙压 7z 压缩文件"),
    (".rar", "MiaoZip.Rar", "妙压 RAR 压缩文件"),
    (".tar", "MiaoZip.Tar", "妙压 TAR 归档文件"),
    (".gz", "MiaoZip.Gzip", "妙压 GZIP 压缩文件"),
    (".bz2", "MiaoZip.Bzip2", "妙压 BZIP2 压缩文件"),
    (".xz", "MiaoZip.Xz", "妙压 XZ 压缩文件"),
    (".zst", "MiaoZip.Zstd", "妙压 ZSTD 压缩文件"),
    (".tgz", "MiaoZip.TarGz", "妙压 TAR.GZ 压缩文件"),
    (".tbz", "MiaoZip.TarBz2", "妙压 TAR.BZ2 压缩文件"),
    (".tbz2", "MiaoZip.TarBz2", "妙压 TAR.BZ2 压缩文件"),
    (".txz", "MiaoZip.TarXz", "妙压 TAR.XZ 压缩文件"),
    (".tzst", "MiaoZip.TarZst", "妙压 TAR.ZST 压缩文件"),
];

pub fn supported_association_extensions() -> impl Iterator<Item = &'static str> {
    FILE_ASSOCIATIONS.iter().map(|(extension, _, _)| *extension)
}

#[derive(Debug, PartialEq, Eq)]
pub enum LaunchAction {
    Normal,
    Add(Vec<PathBuf>),
    AddContext(Vec<PathBuf>),
    Open(PathBuf),
    Extract(PathBuf),
    ExtractHere(PathBuf),
    Mount(PathBuf),
    RegisterIntegration,
    SetDefaultArchives,
    RemoveContextMenu,
    Invalid(String),
}

impl LaunchAction {
    pub fn from_env() -> Self {
        Self::parse(std::env::args_os().skip(1))
    }

    fn parse(args: impl IntoIterator<Item = OsString>) -> Self {
        let mut args = args.into_iter();
        let Some(first) = args.next() else {
            return Self::Normal;
        };
        let remaining: Vec<PathBuf> = args.map(PathBuf::from).collect();
        match first.to_str() {
            Some("--register-integration") if remaining.is_empty() => Self::RegisterIntegration,
            Some("--set-default-archives") if remaining.is_empty() => Self::SetDefaultArchives,
            Some("--remove-context-menu") if remaining.is_empty() => Self::RemoveContextMenu,
            Some("--add") if !remaining.is_empty() => Self::Add(remaining),
            Some("--add-context") if !remaining.is_empty() => Self::AddContext(remaining),
            Some("--open") if remaining.len() == 1 => {
                Self::Open(remaining.into_iter().next().expect("one path"))
            }
            Some("--extract") if remaining.len() == 1 => {
                Self::Extract(remaining.into_iter().next().expect("one path"))
            }
            Some("--extract-here") if remaining.len() == 1 => {
                Self::ExtractHere(remaining.into_iter().next().expect("one path"))
            }
            Some("--mount") if remaining.len() == 1 => {
                Self::Mount(remaining.into_iter().next().expect("one path"))
            }
            Some(flag) if flag.starts_with('-') => Self::Invalid(format!("无效的启动参数：{flag}")),
            _ if remaining.is_empty() => {
                let path = PathBuf::from(first);
                if ArchiveFormat::from_path(&path).is_some() {
                    Self::Open(path)
                } else if path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("iso"))
                {
                    Self::Mount(path)
                } else {
                    Self::Invalid("只能直接打开受支持的压缩文件".to_owned())
                }
            }
            _ => Self::Invalid("启动参数格式不正确".to_owned()),
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::fs;
    use std::hash::{Hash, Hasher};
    use std::io::{Read, Write};
    use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::Path;
    use std::time::{Duration, Instant};
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};

    const APP_NAME: &str = "MiaoZip";
    const DISPLAY_NAME: &str = "妙压";
    const PROG_ID: &str = "MiaoZip.Zip";
    const OWNER_VALUE: &str = "MiaoZipOwner";
    const LEGACY_NAME: &str = "ZipDesk";
    const LEGACY_OWNER_VALUE: &str = "ZipDeskOwner";
    const CONTEXT_MAGIC: &[u8; 8] = b"MZPACK01";
    const CONTEXT_IDLE: Duration = Duration::from_millis(650);
    const CONTEXT_MAX: Duration = Duration::from_secs(4);
    fn verbs() -> Vec<(String, &'static str, &'static str)> {
        let mut result = vec![
            (
                r"Software\Classes\*\shell\MiaoZip.Add".to_owned(),
                "添加到 ZIP (妙压)",
                "add",
            ),
            (
                r"Software\Classes\Directory\shell\MiaoZip.Add".to_owned(),
                "添加到 ZIP (妙压)",
                "add",
            ),
            (
                r"Software\Classes\SystemFileAssociations\.iso\shell\MiaoZip.Mount".to_owned(),
                "使用妙压挂载 ISO",
                "mount",
            ),
        ];
        for (extension, _, _) in FILE_ASSOCIATIONS {
            let prefix = format!(r"Software\Classes\SystemFileAssociations\{extension}\shell");
            result.push((
                format!(r"{prefix}\MiaoZip.Extract"),
                "使用妙压解压",
                "extract",
            ));
            result.push((
                format!(r"{prefix}\MiaoZip.ExtractHere"),
                "解压到当前文件夹… (妙压)",
                "extract-here",
            ));
        }
        result
    }

    fn legacy_verb_paths() -> Vec<String> {
        verbs()
            .into_iter()
            .map(|(path, _, _)| path.replace(r"\MiaoZip.", r"\ZipDesk."))
            .collect()
    }

    fn hkcu() -> RegKey {
        RegKey::predef(HKEY_CURRENT_USER)
    }

    fn executable() -> io::Result<PathBuf> {
        std::env::current_exe()
    }

    fn quoted_executable(path: &Path) -> io::Result<String> {
        let text = path.to_str().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "程序路径不是有效的 Unicode")
        })?;
        Ok(format!("\"{text}\""))
    }

    fn bundled_icon() -> &'static [u8] {
        include_bytes!(concat!(env!("OUT_DIR"), "/miaozip.ico"))
    }

    fn icon_path() -> io::Result<PathBuf> {
        let local_data = std::env::var_os("LOCALAPPDATA")
            .or_else(|| std::env::var_os("APPDATA"))
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "未找到用户应用数据目录"))?;
        let hash = bundled_icon()
            .iter()
            .fold(0xcbf29ce484222325u64, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
            });
        Ok(PathBuf::from(local_data)
            .join("MiaoZip")
            .join("Icons")
            .join(format!("miaozip-{hash:016x}.ico")))
    }

    fn ensure_icon_file() -> io::Result<String> {
        let path = icon_path()?;
        if path.exists() {
            if fs::read(&path)? != bundled_icon() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("图标文件已存在但内容不同：{}", path.display()),
                ));
            }
        } else {
            fs::create_dir_all(path.parent().expect("icon has parent"))?;
            fs::write(&path, bundled_icon())?;
        }
        let text = path.to_str().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "图标路径不是有效的 Unicode")
        })?;
        Ok(format!("\"{text}\""))
    }

    fn shell_command(exe: &Path, action: &str) -> io::Result<String> {
        let action = if action == "add" {
            "add-context"
        } else {
            action
        };
        Ok(format!("{} --{action} \"%1\"", quoted_executable(exe)?))
    }

    fn selection_model(action: &str) -> &'static str {
        if action == "add" { "Player" } else { "Single" }
    }

    fn context_port() -> io::Result<u16> {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        executable()?.hash(&mut hasher);
        std::env::var_os("USERPROFILE").hash(&mut hasher);
        Ok(40000 + (hasher.finish() % 20000) as u16)
    }

    fn write_context_paths(stream: &mut impl Write, paths: &[PathBuf]) -> io::Result<()> {
        stream.write_all(CONTEXT_MAGIC)?;
        stream.write_all(&(paths.len() as u32).to_le_bytes())?;
        for path in paths {
            let units: Vec<u16> = path.as_os_str().encode_wide().collect();
            if units.len() > 32767 {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "文件路径过长"));
            }
            stream.write_all(&(units.len() as u32).to_le_bytes())?;
            for unit in units {
                stream.write_all(&unit.to_le_bytes())?;
            }
        }
        Ok(())
    }

    fn read_context_paths(stream: &mut impl Read) -> io::Result<Vec<PathBuf>> {
        let mut magic = [0; 8];
        stream.read_exact(&mut magic)?;
        if &magic != CONTEXT_MAGIC {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "无效的菜单请求"));
        }
        let mut number = [0; 4];
        stream.read_exact(&mut number)?;
        let count = u32::from_le_bytes(number);
        if count == 0 || count > 100 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "文件数量无效"));
        }
        let mut paths = Vec::with_capacity(count as usize);
        for _ in 0..count {
            stream.read_exact(&mut number)?;
            let len = u32::from_le_bytes(number);
            if len == 0 || len > 32767 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "文件路径长度无效",
                ));
            }
            let mut units = Vec::with_capacity(len as usize);
            for _ in 0..len {
                let mut bytes = [0; 2];
                stream.read_exact(&mut bytes)?;
                units.push(u16::from_le_bytes(bytes));
            }
            paths.push(PathBuf::from(OsString::from_wide(&units)));
        }
        Ok(paths)
    }

    /// Explorer runs a legacy command once per selected item. Merge those short-lived
    /// launches before opening one compression dialog containing the whole selection.
    pub fn collect_context_selection(paths: Vec<PathBuf>) -> Option<Vec<PathBuf>> {
        let Ok(port) = context_port() else {
            return Some(paths);
        };
        let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
        let listener = match TcpListener::bind(address) {
            Ok(listener) => listener,
            Err(_) => {
                if let Ok(mut stream) =
                    TcpStream::connect_timeout(&address.into(), Duration::from_millis(250))
                {
                    let _ = stream.set_read_timeout(Some(Duration::from_millis(400)));
                    let _ = stream.set_write_timeout(Some(Duration::from_millis(400)));
                    if write_context_paths(&mut stream, &paths).is_ok() {
                        let mut ack = [0];
                        if stream.read_exact(&mut ack).is_ok() && ack[0] == 1 {
                            return None;
                        }
                    }
                }
                return Some(paths);
            }
        };
        if listener.set_nonblocking(true).is_err() {
            return Some(paths);
        }
        let started = Instant::now();
        let mut last_path = started;
        let mut collected = paths;
        while started.elapsed() < CONTEXT_MAX && last_path.elapsed() < CONTEXT_IDLE {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_read_timeout(Some(Duration::from_millis(400)));
                    let _ = stream.set_write_timeout(Some(Duration::from_millis(400)));
                    if let Ok(incoming) = read_context_paths(&mut stream) {
                        collected.extend(incoming);
                        last_path = Instant::now();
                        let _ = stream.write_all(&[1]);
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(15));
                }
                Err(_) => break,
            }
        }
        let mut unique = Vec::with_capacity(collected.len());
        for path in collected {
            if !unique.contains(&path) {
                unique.push(path);
            }
        }
        Some(unique)
    }

    fn set_default(key: &RegKey, value: &str) -> io::Result<()> {
        key.set_value("", &value)
    }

    fn is_legacy_handler(prog_id: &str) -> bool {
        prog_id.to_ascii_lowercase().starts_with("zipdesk.")
            || prog_id.eq_ignore_ascii_case(r"Applications\zipdesk.exe")
    }

    fn legacy_key_owned(path: &str) -> io::Result<bool> {
        match hkcu().open_subkey_with_flags(path, KEY_READ) {
            Ok(key) => Ok(key
                .get_value::<String, _>(LEGACY_OWNER_VALUE)
                .is_ok_and(|owner| owner == LEGACY_NAME)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }

    fn legacy_open_command(path: &str) -> bool {
        hkcu()
            .open_subkey(format!(r"{path}\shell\open\command"))
            .and_then(|key| key.get_value::<String, _>(""))
            .is_ok_and(|command| {
                command
                    .to_ascii_lowercase()
                    .ends_with("\\zipdesk.exe\" --open \"%1\"")
            })
    }

    fn cleanup_legacy_context_menu() -> io::Result<()> {
        let root = hkcu();
        for path in legacy_verb_paths() {
            if !legacy_key_owned(&path)? {
                continue;
            }
            let command: String = match root.open_subkey(format!(r"{path}\command")) {
                Ok(key) => key.get_value("")?,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            if command.to_ascii_lowercase().contains("\\zipdesk.exe\" --") {
                root.delete_subkey_all(&path)?;
            }
        }
        Ok(())
    }

    fn legacy_handler_still_selected() -> io::Result<bool> {
        let root = hkcu();
        for (extension, _, _) in FILE_ASSOCIATIONS {
            let class_path = format!(r"Software\Classes\{extension}");
            if let Ok(key) = root.open_subkey_with_flags(&class_path, KEY_READ)
                && let Ok(default) = key.get_value::<String, _>("")
                && is_legacy_handler(&default)
            {
                return Ok(true);
            }
            let choice_path = format!(
                r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\{extension}\UserChoice"
            );
            match root.open_subkey_with_flags(&choice_path, KEY_READ) {
                Ok(key) => {
                    let choice: String = key.get_value("ProgId")?;
                    if is_legacy_handler(&choice) {
                        return Ok(true);
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        Ok(false)
    }

    /// Retire only keys that the previous ZipDesk build marked as its own, and
    /// only after no file association still references its handler.
    fn cleanup_legacy_registration() -> io::Result<()> {
        if legacy_handler_still_selected()? {
            return Ok(());
        }
        let root = hkcu();
        let legacy_app = r"Software\Classes\Applications\zipdesk.exe";
        if legacy_key_owned(legacy_app)? && legacy_open_command(legacy_app) {
            root.delete_subkey_all(legacy_app)?;
        }
        let legacy_capabilities = r"Software\ZipDesk\Capabilities";
        if legacy_key_owned(legacy_capabilities)? {
            if let Ok(registered) = root
                .open_subkey_with_flags(r"Software\RegisteredApplications", KEY_READ | KEY_WRITE)
                && registered
                    .get_value::<String, _>(LEGACY_NAME)
                    .is_ok_and(|path| path == legacy_capabilities)
            {
                registered.delete_value(LEGACY_NAME)?;
            }
            root.delete_subkey_all(legacy_capabilities)?;
        }
        let mut removed_prog_ids = std::collections::HashSet::new();
        for (_, prog_id, _) in FILE_ASSOCIATIONS {
            let legacy_prog_id = prog_id.replacen("MiaoZip.", "ZipDesk.", 1);
            if !removed_prog_ids.insert(legacy_prog_id.clone()) {
                continue;
            }
            let legacy_path = format!(r"Software\Classes\{legacy_prog_id}");
            if !legacy_key_owned(&legacy_path)? || !legacy_open_command(&legacy_path) {
                continue;
            }
            for (candidate_extension, candidate_prog_id, _) in FILE_ASSOCIATIONS {
                if candidate_prog_id != prog_id {
                    continue;
                }
                if let Ok(open_with) = root.open_subkey_with_flags(
                    format!(r"Software\Classes\{candidate_extension}\OpenWithProgids"),
                    KEY_READ | KEY_WRITE,
                ) {
                    match open_with.delete_value(&legacy_prog_id) {
                        Ok(()) => {}
                        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                        Err(error) => return Err(error),
                    }
                }
            }
            root.delete_subkey_all(legacy_path)?;
        }
        Ok(())
    }

    fn has_miaozip_open_command(key: &RegKey) -> bool {
        key.open_subkey(r"shell\open\command")
            .and_then(|command| command.get_value::<String, _>(""))
            .is_ok_and(|command| {
                command
                    .to_ascii_lowercase()
                    .ends_with("\\miaozip.exe\" --open \"%1\"")
            })
    }

    fn is_legacy_candidate(path: &str, key: &RegKey) -> bool {
        match path {
            r"Software\Classes\MiaoZip.Zip" => {
                key.get_value::<String, _>("")
                    .is_ok_and(|name| name == "妙压 ZIP 压缩文件")
                    && has_miaozip_open_command(key)
            }
            r"Software\Classes\Applications\miaozip.exe" => {
                key.get_value::<String, _>("FriendlyAppName")
                    .is_ok_and(|name| name == DISPLAY_NAME)
                    && has_miaozip_open_command(key)
            }
            r"Software\MiaoZip\Capabilities" => {
                key.get_value::<String, _>("ApplicationName")
                    .is_ok_and(|name| name == DISPLAY_NAME)
                    && key
                        .open_subkey("FileAssociations")
                        .and_then(|types| types.get_value::<String, _>(".zip"))
                        .is_ok_and(|prog_id| prog_id == PROG_ID)
            }
            _ => false,
        }
    }

    fn ensure_owned_or_absent(path: &str) -> io::Result<()> {
        match hkcu().open_subkey_with_flags(path, KEY_READ) {
            Ok(existing) => {
                let owner: String = existing.get_value(OWNER_VALUE).unwrap_or_default();
                let legacy_icon = path.ends_with(r"\DefaultIcon")
                    && existing
                        .get_value::<String, _>(LEGACY_OWNER_VALUE)
                        .is_ok_and(|owner| owner == LEGACY_NAME)
                    && existing.get_value::<String, _>("").is_ok_and(|icon| {
                        icon.to_ascii_lowercase()
                            .contains(r"\zipdesk\icons\zipdesk-")
                    });
                if owner == APP_NAME
                    || legacy_icon
                    || (owner.is_empty() && is_legacy_candidate(path, &existing))
                {
                    Ok(())
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        format!("注册表路径已由其他程序占用：{path}"),
                    ))
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn create_owned_verb(
        path: &str,
        title: &str,
        command: &str,
        icon: &str,
        action: &str,
    ) -> io::Result<()> {
        let root = hkcu();
        ensure_owned_or_absent(path)?;
        let (verb, _) = root.create_subkey(path)?;
        verb.set_value(OWNER_VALUE, &APP_NAME)?;
        verb.set_value("MUIVerb", &title)?;
        verb.set_value("Icon", &icon)?;
        verb.set_value("MultiSelectModel", &selection_model(action))?;
        let (command_key, _) = verb.create_subkey("command")?;
        set_default(&command_key, command)
    }

    fn notify_association_changed() {
        #[link(name = "Shell32")]
        unsafe extern "system" {
            fn SHChangeNotify(
                event_id: i32,
                flags: u32,
                item1: *const core::ffi::c_void,
                item2: *const core::ffi::c_void,
            );
        }
        // SHCNE_ASSOCCHANGED and SHCNF_IDLIST; Explorer refreshes its association cache.
        unsafe { SHChangeNotify(0x0800_0000, 0, std::ptr::null(), std::ptr::null()) };
    }

    pub fn register_default_candidate() -> io::Result<()> {
        let exe = executable()?;
        let icon = ensure_icon_file()?;
        let root = hkcu();

        for (_, prog_id, _) in FILE_ASSOCIATIONS {
            ensure_owned_or_absent(&format!(r"Software\Classes\{prog_id}"))?;
        }
        for (extension, _, _) in FILE_ASSOCIATIONS {
            ensure_owned_or_absent(&format!(r"Software\Classes\{extension}\DefaultIcon"))?;
        }
        ensure_owned_or_absent(r"Software\Classes\Applications\miaozip.exe")?;
        ensure_owned_or_absent(r"Software\MiaoZip\Capabilities")?;
        if let Ok(registered) =
            root.open_subkey_with_flags(r"Software\RegisteredApplications", KEY_READ)
            && let Ok(existing) = registered.get_value::<String, _>(APP_NAME)
            && existing != r"Software\MiaoZip\Capabilities"
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "另一个应用已使用妙压的默认应用注册名称",
            ));
        }
        let open_command = shell_command(&exe, "open")?;
        for (extension, prog_id_name, display_name) in FILE_ASSOCIATIONS {
            let (prog_id, _) = root.create_subkey(format!(r"Software\Classes\{prog_id_name}"))?;
            prog_id.set_value(OWNER_VALUE, &APP_NAME)?;
            set_default(&prog_id, display_name)?;
            let (default_icon, _) = prog_id.create_subkey("DefaultIcon")?;
            set_default(&default_icon, &icon)?;
            let (shell, _) = prog_id.create_subkey("shell")?;
            set_default(&shell, "open")?;
            let (open, _) = shell.create_subkey(r"open\command")?;
            set_default(&open, &open_command)?;
            // Add an Open With candidate without replacing another app's default.
            let (extension_key, _) =
                root.create_subkey(format!(r"Software\Classes\{extension}"))?;
            let (extension_icon, _) = extension_key.create_subkey("DefaultIcon")?;
            extension_icon.set_value(OWNER_VALUE, &APP_NAME)?;
            set_default(&extension_icon, &icon)?;
            if extension_icon
                .get_value::<String, _>(LEGACY_OWNER_VALUE)
                .is_ok_and(|owner| owner == LEGACY_NAME)
            {
                extension_icon.delete_value(LEGACY_OWNER_VALUE)?;
            }
            let (open_with, _) = extension_key.create_subkey("OpenWithProgids")?;
            open_with.set_value(prog_id_name, &"")?;
        }

        let (application, _) = root.create_subkey(r"Software\Classes\Applications\miaozip.exe")?;
        application.set_value(OWNER_VALUE, &APP_NAME)?;
        application.set_value("FriendlyAppName", &DISPLAY_NAME)?;
        let (application_icon, _) = application.create_subkey("DefaultIcon")?;
        set_default(&application_icon, &icon)?;
        let (app_open, _) = application.create_subkey(r"shell\open\command")?;
        set_default(&app_open, &open_command)?;
        let (types, _) = application.create_subkey("SupportedTypes")?;
        for (extension, _, _) in FILE_ASSOCIATIONS {
            types.set_value(extension, &"")?;
        }

        let (capabilities, _) = root.create_subkey(r"Software\MiaoZip\Capabilities")?;
        capabilities.set_value(OWNER_VALUE, &APP_NAME)?;
        capabilities.set_value("ApplicationName", &DISPLAY_NAME)?;
        capabilities.set_value(
            "ApplicationDescription",
            &"支持多种压缩格式的跨平台文件管理工具",
        )?;
        let (file_associations, _) = capabilities.create_subkey("FileAssociations")?;
        for (extension, prog_id, _) in FILE_ASSOCIATIONS {
            file_associations.set_value(extension, &prog_id)?;
        }
        let (registered, _) = root.create_subkey(r"Software\RegisteredApplications")?;
        registered.set_value(APP_NAME, &r"Software\MiaoZip\Capabilities")?;

        cleanup_legacy_registration()?;
        notify_association_changed();
        Ok(())
    }

    /// Sets a per-user extension default only when Windows has no UserChoice
    /// for that extension. UserChoice must be changed through Windows Settings.
    pub fn set_default_associations() -> io::Result<Vec<&'static str>> {
        register_default_candidate()?;
        let root = hkcu();
        for (extension, prog_id, _) in FILE_ASSOCIATIONS {
            if is_default_for(extension, prog_id) {
                continue;
            }
            let user_choice_path = format!(
                r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\{extension}\UserChoice"
            );
            match root.open_subkey_with_flags(&user_choice_path, KEY_READ) {
                Ok(_) => continue,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
            let (extension_key, _) =
                root.create_subkey(format!(r"Software\Classes\{extension}"))?;
            set_default(&extension_key, prog_id)?;
        }
        cleanup_legacy_registration()?;
        notify_association_changed();
        Ok(FILE_ASSOCIATIONS
            .iter()
            .filter(|(extension, prog_id, _)| !is_default_for(extension, prog_id))
            .map(|(extension, _, _)| *extension)
            .collect())
    }

    pub fn default_candidate_registered() -> bool {
        let Ok(exe) = executable() else { return false };
        let Ok(icon_path) = icon_path() else {
            return false;
        };
        if !icon_path.is_file() {
            return false;
        }
        let expected_icon = format!("\"{}\"", icon_path.display());
        let root = hkcu();
        let app_path: String = root
            .open_subkey(r"Software\RegisteredApplications")
            .and_then(|key| key.get_value(APP_NAME))
            .unwrap_or_default();
        if app_path != r"Software\MiaoZip\Capabilities" {
            return false;
        }
        let Ok(expected_command) = shell_command(&exe, "open") else {
            return false;
        };
        let Ok(file_associations) =
            root.open_subkey(r"Software\MiaoZip\Capabilities\FileAssociations")
        else {
            return false;
        };
        FILE_ASSOCIATIONS.iter().all(|(extension, prog_id, _)| {
            let Ok(key) = root.open_subkey(format!(r"Software\Classes\{prog_id}")) else {
                return false;
            };
            let owner: String = key.get_value(OWNER_VALUE).unwrap_or_default();
            let icon: String = key
                .open_subkey("DefaultIcon")
                .and_then(|icon_key| icon_key.get_value(""))
                .unwrap_or_default();
            let command: String = key
                .open_subkey(r"shell\open\command")
                .and_then(|command_key| command_key.get_value(""))
                .unwrap_or_default();
            let default_verb: String = key
                .open_subkey("shell")
                .and_then(|shell| shell.get_value(""))
                .unwrap_or_default();
            let mapped: String = file_associations.get_value(*extension).unwrap_or_default();
            let extension_icon: String = root
                .open_subkey(format!(r"Software\Classes\{extension}\DefaultIcon"))
                .and_then(|icon_key| icon_key.get_value(""))
                .unwrap_or_default();
            owner == APP_NAME
                && icon.eq_ignore_ascii_case(&expected_icon)
                && extension_icon.eq_ignore_ascii_case(&expected_icon)
                && default_verb.eq_ignore_ascii_case("open")
                && command.eq_ignore_ascii_case(&expected_command)
                && mapped == *prog_id
        })
    }

    fn effective_prog_id(extension: &str) -> Option<String> {
        #[link(name = "Shlwapi")]
        unsafe extern "system" {
            fn AssocQueryStringW(
                flags: u32,
                query: u32,
                association: *const u16,
                extra: *const u16,
                result: *mut u16,
                result_len: *mut u32,
            ) -> i32;
        }
        let wide: Vec<u16> = std::ffi::OsStr::new(extension)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let mut result = vec![0u16; 1024];
        let mut result_len = result.len() as u32;
        // ASSOCSTR_PROGID = 20. This reports the handler Windows Shell uses,
        // including its UserChoice and built-in ZIP handler precedence.
        let status = unsafe {
            AssocQueryStringW(
                0,
                20,
                wide.as_ptr(),
                std::ptr::null(),
                result.as_mut_ptr(),
                &mut result_len,
            )
        };
        (status == 0).then(|| {
            let end = result
                .iter()
                .position(|unit| *unit == 0)
                .unwrap_or(result.len());
            String::from_utf16_lossy(&result[..end])
        })
    }

    fn is_default_for(extension: &str, expected_prog_id: &str) -> bool {
        effective_prog_id(extension).is_some_and(|prog_id| {
            prog_id.eq_ignore_ascii_case(expected_prog_id)
                || prog_id.eq_ignore_ascii_case("Applications\\miaozip.exe")
        })
    }

    pub fn is_default_zip() -> bool {
        is_default_for(".zip", PROG_ID)
    }

    pub fn default_association_count() -> usize {
        FILE_ASSOCIATIONS
            .iter()
            .filter(|(extension, prog_id, _)| is_default_for(extension, prog_id))
            .count()
    }

    pub fn open_default_apps_settings() -> io::Result<()> {
        use std::ffi::OsStr;
        #[link(name = "Shell32")]
        unsafe extern "system" {
            fn ShellExecuteW(
                hwnd: *mut core::ffi::c_void,
                operation: *const u16,
                file: *const u16,
                parameters: *const u16,
                directory: *const u16,
                show_cmd: i32,
            ) -> isize;
        }
        let uri: Vec<u16> = OsStr::new("ms-settings:defaultapps?registeredAppUser=MiaoZip")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                std::ptr::null(),
                uri.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
            )
        };
        if result > 32 {
            Ok(())
        } else {
            Err(io::Error::other(format!(
                "无法打开 Windows 默认应用设置（ShellExecuteW 返回 {result}）"
            )))
        }
    }

    pub fn register_context_menu() -> io::Result<()> {
        let exe = executable()?;
        let icon = ensure_icon_file()?;
        for (path, _, _) in verbs() {
            ensure_owned_or_absent(&path)?;
        }
        for (path, title, action) in verbs() {
            create_owned_verb(&path, title, &shell_command(&exe, action)?, &icon, action)?;
        }
        cleanup_legacy_context_menu()?;
        notify_association_changed();
        Ok(())
    }

    pub fn context_menu_registered() -> bool {
        let Ok(exe) = executable() else { return false };
        let Ok(icon_path) = icon_path() else {
            return false;
        };
        if !icon_path.is_file() {
            return false;
        }
        let expected_icon = format!("\"{}\"", icon_path.display());
        verbs().iter().all(|(path, _, action)| {
            let Ok(key) = hkcu().open_subkey(path) else {
                return false;
            };
            let owner: String = key.get_value(OWNER_VALUE).unwrap_or_default();
            let model: String = key.get_value("MultiSelectModel").unwrap_or_default();
            let icon: String = key.get_value("Icon").unwrap_or_default();
            let Ok(command) = key.open_subkey("command") else {
                return false;
            };
            let command: String = command.get_value("").unwrap_or_default();
            owner == APP_NAME
                && model == selection_model(action)
                && icon.eq_ignore_ascii_case(&expected_icon)
                && shell_command(&exe, action)
                    .is_ok_and(|expected| command.eq_ignore_ascii_case(&expected))
        })
    }

    pub fn unregister_context_menu() -> io::Result<()> {
        let root = hkcu();
        for (path, _, _) in verbs() {
            let key = match root.open_subkey(&path) {
                Ok(key) => key,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            let owner: String = key.get_value(OWNER_VALUE).unwrap_or_default();
            if owner != APP_NAME {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!("不会移除不属于妙压的菜单项：{path}"),
                ));
            }
        }
        for (path, _, _) in verbs() {
            match root.delete_subkey_all(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        notify_association_changed();
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn quotes_executable_and_explorer_argument() {
            let command =
                shell_command(Path::new(r"C:\Program Files\Miao Zip\miaozip.exe"), "add").unwrap();
            assert_eq!(
                command,
                r#""C:\Program Files\Miao Zip\miaozip.exe" --add-context "%1""#
            );
            let quick_extract = shell_command(
                Path::new(r"C:\Program Files\Miao Zip\miaozip.exe"),
                "extract-here",
            )
            .unwrap();
            assert_eq!(
                quick_extract,
                r#""C:\Program Files\Miao Zip\miaozip.exe" --extract-here "%1""#
            );
        }

        #[test]
        fn context_verbs_cover_every_registered_archive_extension() {
            let specs = verbs();
            assert_eq!(specs.len(), 3 + 2 * FILE_ASSOCIATIONS.len());
            let unique: std::collections::HashSet<_> =
                specs.iter().map(|(path, _, _)| path).collect();
            assert_eq!(unique.len(), specs.len());
            for (extension, _, _) in FILE_ASSOCIATIONS {
                assert!(specs.iter().any(|(path, _, _)| {
                    path.contains(&format!(r"\{extension}\shell\MiaoZip.Extract"))
                }));
            }
        }

        #[test]
        fn migration_only_targets_previous_app_names() {
            let current = verbs();
            let legacy = legacy_verb_paths();
            assert_eq!(legacy.len(), current.len());
            assert!(legacy.iter().all(|path| path.contains(r"\ZipDesk.")));
            assert!(
                current
                    .iter()
                    .all(|(path, _, _)| path.contains(r"\MiaoZip."))
            );
            assert!(is_legacy_handler("ZipDesk.Zip"));
            assert!(is_legacy_handler(r"Applications\zipdesk.exe"));
            assert!(!is_legacy_handler("MiaoZip.Zip"));
            assert!(!is_legacy_handler("CompressedFolder"));
        }

        #[test]
        fn context_selection_protocol_preserves_unicode_and_spaces() {
            let paths = vec![
                PathBuf::from(r"C:\中文 名称\甲.txt"),
                PathBuf::from(r"D:\a b\乙.txt"),
            ];
            let mut bytes = Vec::new();
            write_context_paths(&mut bytes, &paths).unwrap();
            assert_eq!(read_context_paths(&mut bytes.as_slice()).unwrap(), paths);
            assert_eq!(selection_model("add"), "Player");
            assert_eq!(selection_model("extract"), "Single");
        }

        #[test]
        fn merges_separate_shell_launches_into_one_selection() {
            let first = PathBuf::from(r"C:\测试\甲.txt");
            let second = PathBuf::from(r"C:\测试\乙.txt");
            let first_for_thread = first.clone();
            let leader = std::thread::spawn(move || {
                collect_context_selection(vec![first_for_thread]).expect("leader owns the dialog")
            });
            std::thread::sleep(Duration::from_millis(100));
            assert_eq!(collect_context_selection(vec![second.clone()]), None);
            assert_eq!(leader.join().unwrap(), vec![first, second]);
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use std::env;
    use std::fs;
    use std::path::Path;
    use std::process::{Command, Output};

    const DESKTOP_ID: &str = "miaozip.desktop";
    const ICON_PNG: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/miaozip-icon.png"));
    const MIME_TYPES: &[&str] = &[
        "application/zip",
        "application/x-zip",
        "application/x-zip-compressed",
        "application/x-7z-compressed",
        "application/vnd.rar",
        "application/x-rar",
        "application/x-rar-compressed",
        "application/x-tar",
        "application/gzip",
        "application/x-gzip",
        "application/x-compressed-tar",
        "application/x-bzip2",
        "application/bzip2",
        "application/x-bzip",
        "application/x-bzip2-compressed-tar",
        "application/x-bzip-compressed-tar",
        "application/x-xz",
        "application/x-xz-compressed-tar",
        "application/zstd",
        "application/x-zstd",
        "application/x-zstd-compressed-tar",
    ];

    fn canonical_mime(extension: &str) -> &'static str {
        match extension {
            ".zip" => "application/zip",
            ".7z" => "application/x-7z-compressed",
            ".rar" => "application/vnd.rar",
            ".tar" => "application/x-tar",
            ".gz" => "application/gzip",
            ".bz2" => "application/x-bzip2",
            ".xz" => "application/x-xz",
            ".zst" => "application/zstd",
            ".tgz" => "application/x-compressed-tar",
            ".tbz" | ".tbz2" => "application/x-bzip2-compressed-tar",
            ".txz" => "application/x-xz-compressed-tar",
            ".tzst" => "application/x-zstd-compressed-tar",
            _ => "application/octet-stream",
        }
    }

    fn data_home() -> io::Result<PathBuf> {
        if let Some(path) = env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
            return Ok(PathBuf::from(path));
        }
        let home = env::var_os("HOME").ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "无法确定用户目录：HOME 和 XDG_DATA_HOME 均未设置",
            )
        })?;
        Ok(PathBuf::from(home).join(".local/share"))
    }

    fn current_launcher() -> io::Result<PathBuf> {
        if let Some(path) = env::var_os("APPIMAGE").map(PathBuf::from)
            && path.is_file()
        {
            return Ok(path);
        }
        env::current_exe()
    }

    fn quote_desktop_exec(path: &Path) -> String {
        let mut escaped = String::new();
        for character in path.to_string_lossy().chars() {
            match character {
                '\\' => escaped.push_str("\\\\"),
                '"' => escaped.push_str("\\\""),
                '`' => escaped.push_str("\\`"),
                '$' => escaped.push_str("\\$"),
                character => escaped.push(character),
            }
        }
        format!("\"{escaped}\"")
    }

    fn user_desktop_path() -> io::Result<PathBuf> {
        Ok(data_home()?.join("applications").join(DESKTOP_ID))
    }

    fn system_candidate_exists() -> bool {
        [
            Path::new("/usr/share/applications/miaozip.desktop"),
            Path::new("/usr/local/share/applications/miaozip.desktop"),
        ]
        .into_iter()
        .any(Path::is_file)
    }

    fn run_xdg_mime(arguments: &[&str]) -> io::Result<Output> {
        Command::new("xdg-mime")
            .args(arguments)
            .output()
            .map_err(|error| {
                if error.kind() == io::ErrorKind::NotFound {
                    io::Error::new(
                        io::ErrorKind::NotFound,
                        "未找到 xdg-mime，请安装 xdg-utils 后重试",
                    )
                } else {
                    error
                }
            })
    }

    fn query_default(mime_type: &str) -> bool {
        run_xdg_mime(&["query", "default", mime_type])
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .is_some_and(|desktop| desktop.trim() == DESKTOP_ID)
    }

    fn unsupported_context_menu() -> io::Error {
        io::Error::new(
            io::ErrorKind::Unsupported,
            "Linux 文件管理器没有统一的右键菜单注册接口",
        )
    }

    pub fn register_default_candidate() -> io::Result<()> {
        if system_candidate_exists() {
            return Ok(());
        }

        let data = data_home()?;
        let applications = data.join("applications");
        let icons = data.join("icons/hicolor/256x256/apps");
        fs::create_dir_all(&applications)?;
        fs::create_dir_all(&icons)?;
        fs::write(icons.join("miaozip.png"), ICON_PNG)?;

        let mime_types = MIME_TYPES.join(";");
        let launcher = quote_desktop_exec(&current_launcher()?);
        let desktop = format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=妙压\n\
             GenericName=Archive Manager\n\
             Comment=Create, browse and extract archive files\n\
             Exec={launcher} %f\n\
             Icon=miaozip\n\
             Terminal=false\n\
             Categories=Utility;Archiving;\n\
             MimeType={mime_types};\n\
             StartupNotify=true\n"
        );
        fs::write(applications.join(DESKTOP_ID), desktop)?;

        match Command::new("update-desktop-database")
            .arg(&applications)
            .output()
        {
            Ok(output) if !output.status.success() => Err(io::Error::other(format!(
                "更新桌面应用数据库失败：{}",
                String::from_utf8_lossy(&output.stderr).trim()
            ))),
            Ok(_) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }

    pub fn set_default_associations() -> io::Result<Vec<&'static str>> {
        register_default_candidate()?;
        for mime_type in MIME_TYPES {
            let output = run_xdg_mime(&["default", DESKTOP_ID, mime_type])?;
            if !output.status.success() {
                return Err(io::Error::other(format!(
                    "无法设置 {mime_type} 的默认应用：{}",
                    String::from_utf8_lossy(&output.stderr).trim()
                )));
            }
        }
        Ok(supported_association_extensions()
            .filter(|extension| !query_default(canonical_mime(extension)))
            .collect())
    }

    pub fn is_default_zip() -> bool {
        query_default("application/zip")
    }

    pub fn default_association_count() -> usize {
        supported_association_extensions()
            .filter(|extension| query_default(canonical_mime(extension)))
            .count()
    }

    pub fn default_candidate_registered() -> bool {
        system_candidate_exists() || user_desktop_path().is_ok_and(|path| path.is_file())
    }

    pub fn open_default_apps_settings() -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Linux 桌面环境没有统一的默认应用设置页面，请使用妙压的“设为默认”按钮",
        ))
    }

    pub fn register_context_menu() -> io::Result<()> {
        Err(unsupported_context_menu())
    }

    pub fn collect_context_selection(paths: Vec<PathBuf>) -> Option<Vec<PathBuf>> {
        Some(paths)
    }

    pub fn context_menu_registered() -> bool {
        false
    }

    pub fn unregister_context_menu() -> io::Result<()> {
        Err(unsupported_context_menu())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn every_linux_association_has_a_known_mime_type() {
            for extension in supported_association_extensions() {
                assert_ne!(canonical_mime(extension), "application/octet-stream");
            }
        }

        #[test]
        fn packaged_desktop_entry_declares_every_mime_type() {
            let desktop = include_str!("../packaging/linux/miaozip.desktop");
            for mime_type in MIME_TYPES {
                assert!(
                    desktop.contains(&format!("{mime_type};")),
                    "desktop entry is missing {mime_type}"
                );
            }
        }

        #[test]
        fn desktop_exec_path_is_quoted_and_escaped() {
            assert_eq!(
                quote_desktop_exec(Path::new("/tmp/Miao Zip/$current")),
                "\"/tmp/Miao Zip/\\$current\""
            );
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use std::ffi::{CString, c_char, c_void};
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;
    use std::ptr;

    type CfTypeRef = *const c_void;
    type CfStringRef = *const c_void;
    type CfUrlRef = *const c_void;
    type OsStatus = i32;
    type LsRolesMask = u32;

    const BUNDLE_IDENTIFIER: &str = "com.tangluobo.miaozip";
    const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
    const K_LS_ROLES_VIEWER: LsRolesMask = 0x0000_0002;

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFStringCreateWithCString(
            allocator: *const c_void,
            string: *const c_char,
            encoding: u32,
        ) -> CfStringRef;
        fn CFURLCreateFromFileSystemRepresentation(
            allocator: *const c_void,
            buffer: *const u8,
            buffer_length: isize,
            is_directory: u8,
        ) -> CfUrlRef;
        fn CFEqual(first: CfTypeRef, second: CfTypeRef) -> u8;
        fn CFRelease(value: CfTypeRef);
    }

    #[link(name = "CoreServices", kind = "framework")]
    unsafe extern "C" {
        static kUTTagClassFilenameExtension: CfStringRef;

        fn LSRegisterURL(url: CfUrlRef, update: u8) -> OsStatus;
        fn LSSetDefaultRoleHandlerForContentType(
            content_type: CfStringRef,
            role: LsRolesMask,
            handler_bundle_identifier: CfStringRef,
        ) -> OsStatus;
        fn LSCopyDefaultRoleHandlerForContentType(
            content_type: CfStringRef,
            role: LsRolesMask,
        ) -> CfStringRef;
        fn UTTypeCreatePreferredIdentifierForTag(
            tag_class: CfStringRef,
            tag: CfStringRef,
            conforming_to_type: CfStringRef,
        ) -> CfStringRef;
    }

    struct OwnedCf(CfTypeRef);

    impl OwnedCf {
        fn string(value: &str) -> io::Result<Self> {
            let value = CString::new(value)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "字符串中包含空字符"))?;
            // SAFETY: `value` is a live, NUL-terminated UTF-8 CString for the duration
            // of this call. Core Foundation returns an owned reference.
            let reference = unsafe {
                CFStringCreateWithCString(ptr::null(), value.as_ptr(), K_CF_STRING_ENCODING_UTF8)
            };
            Self::from_created(reference, "无法创建 macOS 文件关联字符串")
        }

        fn from_created(reference: CfTypeRef, message: &str) -> io::Result<Self> {
            if reference.is_null() {
                Err(io::Error::other(message))
            } else {
                Ok(Self(reference))
            }
        }
    }

    impl Drop for OwnedCf {
        fn drop(&mut self) {
            // SAFETY: OwnedCf only wraps references returned with create/copy
            // ownership and releases each reference exactly once.
            unsafe { CFRelease(self.0) };
        }
    }

    fn app_bundle_from_executable(executable: &Path) -> Option<PathBuf> {
        let macos = executable.parent()?;
        if macos.file_name()? != "MacOS" {
            return None;
        }
        let contents = macos.parent()?;
        if contents.file_name()? != "Contents" {
            return None;
        }
        let bundle = contents.parent()?;
        bundle
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
            .then(|| bundle.to_path_buf())
    }

    fn current_app_bundle() -> io::Result<PathBuf> {
        let executable = std::env::current_exe()?;
        let bundle = app_bundle_from_executable(&executable).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "macOS 文件关联要求从 MiaoZip.app 启动；请将应用移入“应用程序”后重新打开",
            )
        })?;
        if !bundle.join("Contents/Info.plist").is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "MiaoZip.app 缺少 Contents/Info.plist，无法注册文件类型",
            ));
        }
        Ok(bundle)
    }

    fn extension_uti(extension: &str) -> io::Result<OwnedCf> {
        let extension = OwnedCf::string(extension.trim_start_matches('.'))?;
        // SAFETY: Both CFString references are valid for the call. Passing null for
        // `conforming_to_type` asks Launch Services for the preferred UTI.
        let uti = unsafe {
            UTTypeCreatePreferredIdentifierForTag(
                kUTTagClassFilenameExtension,
                extension.0,
                ptr::null(),
            )
        };
        OwnedCf::from_created(uti, "macOS 无法识别该文件扩展名")
    }

    fn is_default(extension: &str) -> bool {
        let Ok(uti) = extension_uti(extension) else {
            return false;
        };
        let Ok(bundle_identifier) = OwnedCf::string(BUNDLE_IDENTIFIER) else {
            return false;
        };
        // SAFETY: `uti` is a valid CFString. The Copy rule transfers ownership
        // of the returned handler to this function.
        let handler = unsafe { LSCopyDefaultRoleHandlerForContentType(uti.0, K_LS_ROLES_VIEWER) };
        let Ok(handler) = OwnedCf::from_created(handler, "没有默认处理程序") else {
            return false;
        };
        // SAFETY: Both values are valid Core Foundation strings.
        unsafe { CFEqual(handler.0, bundle_identifier.0) != 0 }
    }

    fn launch_services_error(action: &str, status: OsStatus) -> io::Error {
        io::Error::other(format!("{action}失败（Launch Services 状态码 {status}）"))
    }

    pub fn register_default_candidate() -> io::Result<()> {
        let bundle = current_app_bundle()?;
        let bytes = bundle.as_os_str().as_bytes();
        // SAFETY: The byte slice remains live during the call and represents a
        // local filesystem path. The returned URL follows the Create rule.
        let url = unsafe {
            CFURLCreateFromFileSystemRepresentation(
                ptr::null(),
                bytes.as_ptr(),
                bytes.len() as isize,
                1,
            )
        };
        let url = OwnedCf::from_created(url, "无法创建 MiaoZip.app 的文件 URL")?;
        // SAFETY: `url` is a valid file URL pointing at the application bundle.
        let status = unsafe { LSRegisterURL(url.0, 1) };
        if status == 0 {
            Ok(())
        } else {
            Err(launch_services_error("注册 MiaoZip.app", status))
        }
    }

    pub fn set_default_associations() -> io::Result<Vec<&'static str>> {
        register_default_candidate()?;
        let bundle_identifier = OwnedCf::string(BUNDLE_IDENTIFIER)?;
        let mut first_error = None;

        for extension in supported_association_extensions() {
            let uti = extension_uti(extension)?;
            // SAFETY: `uti` and `bundle_identifier` are valid CFStrings and the
            // requested Viewer role matches the declaration in Info.plist.
            let status = unsafe {
                LSSetDefaultRoleHandlerForContentType(uti.0, K_LS_ROLES_VIEWER, bundle_identifier.0)
            };
            if status != 0 && first_error.is_none() {
                first_error = Some((extension, status));
            }
        }

        let remaining: Vec<_> = supported_association_extensions()
            .filter(|extension| !is_default(extension))
            .collect();
        if remaining.len() == FILE_ASSOCIATIONS.len()
            && let Some((extension, status)) = first_error
        {
            return Err(launch_services_error(
                &format!("设置 {extension} 的默认打开方式"),
                status,
            ));
        }
        Ok(remaining)
    }

    pub fn is_default_zip() -> bool {
        is_default(".zip")
    }

    pub fn default_association_count() -> usize {
        supported_association_extensions()
            .filter(|extension| is_default(extension))
            .count()
    }

    pub fn default_candidate_registered() -> bool {
        current_app_bundle().is_ok()
    }

    pub fn open_default_apps_settings() -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "macOS 没有统一的压缩文件默认应用设置页，请使用妙压的“设为默认”按钮",
        ))
    }

    fn unsupported_context_menu() -> io::Error {
        io::Error::new(
            io::ErrorKind::Unsupported,
            "macOS Finder 没有与 Windows 注册表右键菜单相同的注册接口",
        )
    }

    pub fn register_context_menu() -> io::Result<()> {
        Err(unsupported_context_menu())
    }

    pub fn collect_context_selection(paths: Vec<PathBuf>) -> Option<Vec<PathBuf>> {
        Some(paths)
    }

    pub fn context_menu_registered() -> bool {
        false
    }

    pub fn unregister_context_menu() -> io::Result<()> {
        Err(unsupported_context_menu())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn locates_bundle_from_packaged_executable() {
            assert_eq!(
                app_bundle_from_executable(Path::new(
                    "/Applications/MiaoZip.app/Contents/MacOS/miaozip"
                )),
                Some(PathBuf::from("/Applications/MiaoZip.app"))
            );
            assert_eq!(
                app_bundle_from_executable(Path::new("/usr/bin/miaozip")),
                None
            );
        }

        #[test]
        fn packaged_plist_declares_every_archive_extension() {
            let plist = include_str!("../packaging/macos/Info.plist");
            for extension in supported_association_extensions() {
                let extension = extension.trim_start_matches('.');
                assert!(
                    plist.contains(&format!("<string>{extension}</string>")),
                    "Info.plist is missing {extension}"
                );
            }
        }
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod platform {
    use super::*;

    fn unsupported() -> io::Error {
        io::Error::new(io::ErrorKind::Unsupported, "此系统不支持该系统集成功能")
    }

    pub fn register_default_candidate() -> io::Result<()> {
        Err(unsupported())
    }
    pub fn set_default_associations() -> io::Result<Vec<&'static str>> {
        Err(unsupported())
    }
    pub fn is_default_zip() -> bool {
        false
    }
    pub fn default_association_count() -> usize {
        0
    }
    pub fn default_candidate_registered() -> bool {
        false
    }
    pub fn open_default_apps_settings() -> io::Result<()> {
        Err(unsupported())
    }
    pub fn register_context_menu() -> io::Result<()> {
        Err(unsupported())
    }
    pub fn collect_context_selection(paths: Vec<PathBuf>) -> Option<Vec<PathBuf>> {
        Some(paths)
    }
    pub fn context_menu_registered() -> bool {
        false
    }
    pub fn unregister_context_menu() -> io::Result<()> {
        Err(unsupported())
    }
}

pub use platform::{
    collect_context_selection, context_menu_registered, default_association_count,
    default_candidate_registered, is_default_zip, open_default_apps_settings,
    register_context_menu, register_default_candidate, set_default_associations,
    unregister_context_menu,
};

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::Path;

    #[test]
    fn every_registered_extension_is_supported_by_archive_reader() {
        for (extension, _, _) in FILE_ASSOCIATIONS {
            assert!(
                ArchiveFormat::from_path(Path::new(&format!("sample{extension}"))).is_some(),
                "{extension} was registered without a reader"
            );
        }
    }

    #[test]
    fn parses_explorer_actions() {
        assert_eq!(
            LaunchAction::parse([OsString::from("--open"), OsString::from("C:\\测试.zip")]),
            LaunchAction::Open(PathBuf::from("C:\\测试.zip"))
        );
        assert_eq!(
            LaunchAction::parse([OsString::from("--extract"), OsString::from("C:\\测试.zip")]),
            LaunchAction::Extract(PathBuf::from("C:\\测试.zip"))
        );
        assert_eq!(
            LaunchAction::parse([OsString::from("--mount"), OsString::from("C:\\镜像.iso")]),
            LaunchAction::Mount(PathBuf::from("C:\\镜像.iso"))
        );
        assert_eq!(
            LaunchAction::parse([
                OsString::from("--extract-here"),
                OsString::from("C:\\测试.rar")
            ]),
            LaunchAction::ExtractHere(PathBuf::from("C:\\测试.rar"))
        );
        assert_eq!(
            LaunchAction::parse([
                OsString::from("--add"),
                OsString::from("a.txt"),
                OsString::from("b")
            ]),
            LaunchAction::Add(vec![PathBuf::from("a.txt"), PathBuf::from("b")])
        );
        assert_eq!(
            LaunchAction::parse([
                OsString::from("--add-context"),
                OsString::from("a.txt"),
                OsString::from("b")
            ]),
            LaunchAction::AddContext(vec![PathBuf::from("a.txt"), PathBuf::from("b")])
        );
    }

    #[test]
    fn rejects_missing_paths() {
        assert!(matches!(
            LaunchAction::parse([OsString::from("--extract")]),
            LaunchAction::Invalid(_)
        ));
        assert!(matches!(
            LaunchAction::parse([OsString::from("--add")]),
            LaunchAction::Invalid(_)
        ));
        assert!(matches!(
            LaunchAction::parse([OsString::from("--extract-here")]),
            LaunchAction::Invalid(_)
        ));
    }

    #[test]
    fn parses_management_actions_without_paths() {
        assert_eq!(
            LaunchAction::parse([OsString::from("--register-integration")]),
            LaunchAction::RegisterIntegration
        );
        assert_eq!(
            LaunchAction::parse([OsString::from("--set-default-archives")]),
            LaunchAction::SetDefaultArchives
        );
        assert_eq!(
            LaunchAction::parse([OsString::from("--remove-context-menu")]),
            LaunchAction::RemoveContextMenu
        );
        assert!(matches!(
            LaunchAction::parse([
                OsString::from("--register-integration"),
                OsString::from("extra")
            ]),
            LaunchAction::Invalid(_)
        ));
    }
}
