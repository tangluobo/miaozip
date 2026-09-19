use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use eframe::egui;
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use tempfile::TempDir;

use crate::archive::{self, ArchiveFormat, OperationSummary, Progress};
use crate::integration::{self, LaunchAction};
use crate::optical::{self, MountedImage};
use crate::tools::{
    FileHashes, ImageOutputFormat, RenameOptions, TextReplacePreview, ZipRepairResult,
    ZipTestResult,
};

const BLUE: egui::Color32 = egui::Color32::from_rgb(16, 118, 202);
const LINE: egui::Color32 = egui::Color32::from_rgb(213, 229, 241);
const TEXT: egui::Color32 = egui::Color32::from_rgb(24, 37, 49);

mod shell;
mod tools_ui;
use shell::drive_entries;

#[derive(Debug, Clone)]
struct FileEntry {
    path: PathBuf,
    name: String,
    kind: String,
    size: Option<u64>,
    modified: String,
    is_directory: bool,
}

#[derive(Debug, Clone)]
struct DriveEntry {
    path: PathBuf,
    name: String,
    total: Option<u64>,
    available: Option<u64>,
    file_system: String,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
enum ShellIconKey {
    Real(PathBuf),
    VirtualFile(String),
    VirtualFolder,
}

#[derive(Debug)]
enum WorkerMessage {
    Progress(Progress),
    Finished(Result<OperationSummary, String>),
}

#[derive(Debug, Default)]
enum JobStatus {
    #[default]
    Idle,
    Running {
        operation: &'static str,
        completed: usize,
        total: usize,
        current: String,
    },
    Success(String),
    Error(String),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum PasswordDialogMode {
    #[default]
    Unlock,
    Change,
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct MiaoZipApp {
    current_directory: PathBuf,
    sources: Vec<PathBuf>,
    archive_output: Option<PathBuf>,
    zip_input: Option<PathBuf>,
    extract_output: Option<PathBuf>,
    compression_level: u8,
    archive_format: ArchiveFormat,
    overwrite_existing: bool,
    ask_default_on_startup: bool,
    quick_create_mode: bool,

    #[serde(skip)]
    address_text: String,
    #[serde(skip)]
    entries: Vec<FileEntry>,
    #[serde(skip)]
    opened_archive: Option<PathBuf>,
    #[serde(skip)]
    archive_directory: String,
    #[serde(skip)]
    archive_entries: Vec<archive::ArchiveListEntry>,
    #[serde(skip)]
    archive_listing_worker: Option<Receiver<Result<Vec<archive::ArchiveListEntry>, String>>>,
    #[serde(skip)]
    archive_preview_worker: Option<Receiver<Result<(PathBuf, TempDir, PathBuf), String>>>,
    #[serde(skip)]
    archive_preview_directories: Vec<TempDir>,
    #[serde(skip)]
    pending_risky_archive_open: Option<PathBuf>,
    #[serde(skip)]
    selected_archive_item: Option<PathBuf>,
    #[serde(skip)]
    selected_paths: Vec<PathBuf>,
    #[serde(skip)]
    history: Vec<PathBuf>,
    #[serde(skip)]
    history_index: usize,
    #[serde(skip)]
    status: JobStatus,
    #[serde(skip)]
    worker: Option<Receiver<WorkerMessage>>,
    #[serde(skip)]
    show_create_dialog: bool,
    #[serde(skip)]
    show_add_to_archive_dialog: bool,
    #[serde(skip)]
    context_only_add: bool,
    #[serde(skip)]
    context_only_extract: bool,
    #[serde(skip)]
    context_add_started: bool,
    #[serde(skip)]
    create_dialog_tab: u8,
    #[serde(skip)]
    selected_source: Option<usize>,
    #[serde(skip)]
    show_extract_dialog: bool,
    #[serde(skip)]
    extract_dialog_tab: u8,
    #[serde(skip)]
    show_delete_dialog: bool,
    #[serde(skip)]
    show_password_dialog: bool,
    #[serde(skip)]
    password_dialog_mode: PasswordDialogMode,
    #[serde(skip)]
    archive_password: String,
    #[serde(skip)]
    archive_password_confirm: String,
    #[serde(skip)]
    current_archive_password: String,
    #[serde(skip)]
    new_archive_password: String,
    #[serde(skip)]
    new_archive_password_confirm: String,
    #[serde(skip)]
    pending_password_after_job: Option<String>,
    #[serde(skip)]
    reload_archive_after_job: bool,
    #[serde(skip)]
    extracting_sfx: bool,
    #[serde(skip)]
    show_info_dialog: bool,
    #[serde(skip)]
    show_about_dialog: bool,
    #[serde(skip)]
    show_formats_dialog: bool,
    #[serde(skip)]
    show_progress_dialog: bool,
    #[serde(skip)]
    browser_error: Option<String>,
    #[serde(skip)]
    refresh_after_job: bool,
    #[serde(skip)]
    computer_view: bool,
    #[serde(skip)]
    selected_drive: Option<PathBuf>,
    #[serde(skip)]
    search_text: String,
    #[serde(skip)]
    list_scroll: usize,
    #[serde(skip)]
    tree_scroll: usize,
    #[serde(skip)]
    computer_tree_expanded: bool,
    #[serde(skip)]
    expanded_tree_paths: HashSet<PathBuf>,
    #[serde(skip)]
    tree_children: HashMap<PathBuf, Vec<PathBuf>>,
    #[serde(skip)]
    tree_icon_cache: HashMap<ShellIconKey, egui::TextureHandle>,
    #[serde(skip)]
    tree_icon_requested: HashSet<ShellIconKey>,
    #[serde(skip)]
    tree_icon_generation: u64,
    #[serde(skip)]
    tree_icon_requests: Option<mpsc::Sender<(u64, ShellIconKey)>>,
    #[serde(skip)]
    tree_icon_results: Option<Receiver<(u64, ShellIconKey, Option<Vec<u8>>)>>,
    #[serde(skip)]
    reveal_tree_selection: bool,
    #[serde(skip)]
    show_default_prompt: bool,
    #[serde(skip)]
    show_integration_dialog: bool,
    #[serde(skip)]
    integration_dialog_tab: u8,
    #[serde(skip)]
    register_menu_with_default: bool,
    #[serde(skip)]
    dont_ask_again: bool,
    #[serde(skip)]
    integration_error: Option<String>,
    #[serde(skip)]
    show_toolbox_dialog: bool,
    #[serde(skip)]
    show_optical_dialog: bool,
    #[serde(skip)]
    iso_path: Option<PathBuf>,
    #[serde(skip)]
    mounted_image: Option<MountedImage>,
    #[serde(skip)]
    optical_worker: Option<Receiver<Result<Option<MountedImage>, String>>>,
    #[serde(skip)]
    show_hash_dialog: bool,
    #[serde(skip)]
    hash_path: Option<PathBuf>,
    #[serde(skip)]
    hash_expected: String,
    #[serde(skip)]
    hash_result: Option<FileHashes>,
    #[serde(skip)]
    hash_error: Option<String>,
    #[serde(skip)]
    hash_worker: Option<Receiver<Result<FileHashes, String>>>,
    #[serde(skip)]
    show_rename_dialog: bool,
    #[serde(skip)]
    rename_items: Vec<PathBuf>,
    #[serde(skip)]
    rename_options: RenameOptions,
    #[serde(skip)]
    rename_feedback: Option<String>,
    #[serde(skip)]
    rename_confirm: bool,
    #[serde(skip)]
    show_text_replace_dialog: bool,
    #[serde(skip)]
    text_replace_items: Vec<PathBuf>,
    #[serde(skip)]
    text_find: String,
    #[serde(skip)]
    text_replacement: String,
    #[serde(skip)]
    text_replace_preview: Vec<TextReplacePreview>,
    #[serde(skip)]
    text_replace_feedback: Option<String>,
    #[serde(skip)]
    show_image_convert_dialog: bool,
    #[serde(skip)]
    image_convert_items: Vec<PathBuf>,
    #[serde(skip)]
    image_output_format: ImageOutputFormat,
    #[serde(skip)]
    image_convert_worker: Option<Receiver<Result<usize, String>>>,
    #[serde(skip)]
    image_convert_feedback: Option<String>,
    #[serde(skip)]
    show_zip_test_dialog: bool,
    #[serde(skip)]
    zip_test_path: Option<PathBuf>,
    #[serde(skip)]
    zip_test_result: Option<ZipTestResult>,
    #[serde(skip)]
    zip_test_error: Option<String>,
    #[serde(skip)]
    zip_test_worker: Option<Receiver<Result<ZipTestResult, String>>>,
    #[serde(skip)]
    zip_repair_result: Option<ZipRepairResult>,
    #[serde(skip)]
    zip_repair_worker: Option<Receiver<Result<ZipRepairResult, String>>>,
}

impl Default for MiaoZipApp {
    fn default() -> Self {
        let current_directory = default_browse_directory();
        Self {
            address_text: current_directory.display().to_string(),
            history: vec![current_directory.clone()],
            current_directory,
            sources: Vec::new(),
            archive_output: None,
            zip_input: None,
            extract_output: None,
            compression_level: 6,
            archive_format: ArchiveFormat::Zip,
            overwrite_existing: true,
            ask_default_on_startup: true,
            quick_create_mode: true,
            entries: Vec::new(),
            opened_archive: None,
            archive_directory: String::new(),
            archive_entries: Vec::new(),
            archive_listing_worker: None,
            archive_preview_worker: None,
            archive_preview_directories: Vec::new(),
            pending_risky_archive_open: None,
            selected_archive_item: None,
            selected_paths: Vec::new(),
            history_index: 0,
            status: JobStatus::Idle,
            worker: None,
            show_create_dialog: false,
            show_add_to_archive_dialog: false,
            context_only_add: false,
            context_only_extract: false,
            context_add_started: false,
            create_dialog_tab: 0,
            selected_source: None,
            show_extract_dialog: false,
            extract_dialog_tab: 0,
            show_delete_dialog: false,
            show_password_dialog: false,
            password_dialog_mode: PasswordDialogMode::Unlock,
            archive_password: String::new(),
            archive_password_confirm: String::new(),
            current_archive_password: String::new(),
            new_archive_password: String::new(),
            new_archive_password_confirm: String::new(),
            pending_password_after_job: None,
            reload_archive_after_job: false,
            extracting_sfx: false,
            show_info_dialog: false,
            show_about_dialog: false,
            show_formats_dialog: false,
            show_progress_dialog: false,
            browser_error: None,
            refresh_after_job: false,
            computer_view: true,
            selected_drive: None,
            search_text: String::new(),
            list_scroll: 0,
            tree_scroll: 0,
            computer_tree_expanded: true,
            expanded_tree_paths: HashSet::new(),
            tree_children: HashMap::new(),
            tree_icon_cache: HashMap::new(),
            tree_icon_requested: HashSet::new(),
            tree_icon_generation: 0,
            tree_icon_requests: None,
            tree_icon_results: None,
            reveal_tree_selection: false,
            show_default_prompt: false,
            show_integration_dialog: false,
            integration_dialog_tab: if cfg!(any(target_os = "linux", target_os = "macos")) {
                1
            } else {
                0
            },
            register_menu_with_default: true,
            dont_ask_again: false,
            integration_error: None,
            show_toolbox_dialog: false,
            show_optical_dialog: false,
            iso_path: None,
            mounted_image: None,
            optical_worker: None,
            show_hash_dialog: false,
            hash_path: None,
            hash_expected: String::new(),
            hash_result: None,
            hash_error: None,
            hash_worker: None,
            show_rename_dialog: false,
            rename_items: Vec::new(),
            rename_options: RenameOptions::default(),
            rename_feedback: None,
            rename_confirm: false,
            show_text_replace_dialog: false,
            text_replace_items: Vec::new(),
            text_find: String::new(),
            text_replacement: String::new(),
            text_replace_preview: Vec::new(),
            text_replace_feedback: None,
            show_image_convert_dialog: false,
            image_convert_items: Vec::new(),
            image_output_format: ImageOutputFormat::default(),
            image_convert_worker: None,
            image_convert_feedback: None,
            show_zip_test_dialog: false,
            zip_test_path: None,
            zip_test_result: None,
            zip_test_error: None,
            zip_test_worker: None,
            zip_repair_result: None,
            zip_repair_worker: None,
        }
    }
}

impl MiaoZipApp {
    pub fn new(
        creation_context: &eframe::CreationContext<'_>,
        launch_action: LaunchAction,
    ) -> Self {
        let context_only_add = matches!(&launch_action, LaunchAction::AddContext(_));
        let context_only_extract = matches!(&launch_action, LaunchAction::ExtractHere(_));
        let context_only_operation = context_only_add || context_only_extract;
        install_cjk_font(&creation_context.egui_ctx);
        apply_haozip_visuals(&creation_context.egui_ctx);
        creation_context.egui_ctx.set_embed_viewports(false);
        #[cfg(windows)]
        {
            creation_context.egui_ctx.set_pixels_per_point(1.0);
            if !context_only_operation {
                creation_context
                    .egui_ctx
                    .send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(1200.0, 753.0)));
            }
        }

        let mut app: Self = creation_context
            .storage
            .and_then(|storage| eframe::get_value(storage, eframe::APP_KEY))
            .unwrap_or_default();
        app.context_only_add = context_only_add;
        app.context_only_extract = context_only_extract;
        if !context_only_operation {
            app.start_tree_icon_loader(&creation_context.egui_ctx);
        }
        app.context_add_started = false;
        if !app.current_directory.is_dir() {
            app.current_directory = default_browse_directory();
        }
        app.computer_view = true;
        app.address_text = "此电脑".to_owned();
        app.computer_tree_expanded = true;
        app.history = vec![PathBuf::new()];
        app.history_index = 0;
        app.selected_drive = drive_entries().first().map(|drive| drive.path.clone());
        if !context_only_operation {
            app.refresh_entries();
        }
        match launch_action {
            LaunchAction::Normal => {
                app.show_default_prompt = should_show_registration_prompt(
                    app.ask_default_on_startup,
                    integration::default_candidate_registered(),
                    integration::default_association_count(),
                    integration::supported_association_extensions().count(),
                );
            }
            LaunchAction::FirstRun => {
                app.show_default_prompt = true;
                app.dont_ask_again = false;
            }
            LaunchAction::Add(paths) | LaunchAction::AddContext(paths) => {
                if paths.iter().all(|path| path.exists()) {
                    app.sources = paths;
                    app.prepare_archive_destination();
                    app.show_create_dialog = true;
                    if context_only_add {
                        app.quick_create_mode = true;
                    }
                } else {
                    app.status = JobStatus::Error("右键菜单传入的文件或目录不存在".to_owned());
                }
            }
            LaunchAction::Open(path) => {
                if path.is_file() && is_archive_file(&path) {
                    app.open_archive(path);
                } else {
                    app.status = JobStatus::Error("无法打开：不支持此压缩格式".to_owned());
                }
            }
            LaunchAction::Extract(path) => {
                if path.is_file() && is_archive_file(&path) {
                    app.prepare_extract_path(path);
                } else {
                    app.status = JobStatus::Error("无法打开：不支持此压缩格式".to_owned());
                }
            }
            LaunchAction::ExtractHere(path) => {
                if path.is_file() && is_archive_file(&path) {
                    app.prepare_extract_path(path);
                    if let Some(archive) = &app.zip_input {
                        app.extract_output =
                            Some(archive.parent().unwrap_or(Path::new(".")).to_path_buf());
                    }
                    // “解压到当前目录”是无设置页的快捷动作，不能继承普通
                    // 解压对话框中可能保存的“禁止覆盖”选项，否则目标父目录
                    // 必然已存在，任务会在进度窗口出现前直接失败。
                    app.overwrite_existing = true;
                    app.show_extract_dialog = false;
                    if app.start_extraction(&creation_context.egui_ctx) {
                        app.show_progress_dialog = false;
                    }
                } else {
                    app.status = JobStatus::Error("无法打开：不支持此压缩格式".to_owned());
                }
            }
            LaunchAction::Mount(path) => {
                if path.is_file()
                    && path
                        .extension()
                        .is_some_and(|value| value.eq_ignore_ascii_case("iso"))
                {
                    app.iso_path = Some(path);
                    app.show_optical_dialog = true;
                } else {
                    app.status = JobStatus::Error("无法打开：不是有效的 ISO 镜像路径".to_owned());
                }
            }
            LaunchAction::SelfExtract(path) => {
                app.current_directory = path
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .to_path_buf();
                app.extract_output = Some(app.current_directory.join(archive_base_name(&path)));
                app.zip_input = Some(path);
                app.extracting_sfx = true;
                app.overwrite_existing = true;
                app.show_extract_dialog = true;
            }
            LaunchAction::Invalid(message) => app.status = JobStatus::Error(message),
            LaunchAction::RegisterIntegration
            | LaunchAction::SetDefaultArchives
            | LaunchAction::RemoveContextMenu
            | LaunchAction::UnregisterIntegration => {
                unreachable!("system integration commands are handled before opening the UI")
            }
        }
        app
    }

    fn is_running(&self) -> bool {
        matches!(self.status, JobStatus::Running { .. })
    }

    fn invalidate_shell_icons(&mut self) {
        self.tree_icon_generation = self.tree_icon_generation.wrapping_add(1);
        self.tree_icon_cache.clear();
        self.tree_icon_requested.clear();
        if let Some(receiver) = &self.tree_icon_results {
            while receiver.try_recv().is_ok() {}
        }
    }

    fn refresh_entries(&mut self) {
        if self.opened_archive.is_some() {
            self.refresh_archive_directory();
            return;
        }
        self.entries.clear();
        self.browser_error = None;
        let directory = match fs::read_dir(&self.current_directory) {
            Ok(directory) => directory,
            Err(error) => {
                self.browser_error = Some(format!(
                    "无法打开 {}：{error}",
                    self.current_directory.display()
                ));
                return;
            }
        };

        for item in directory {
            let item = match item {
                Ok(item) => item,
                Err(error) => {
                    self.browser_error = Some(format!("部分文件无法读取：{error}"));
                    continue;
                }
            };
            let path = item.path();
            let metadata = match item.metadata() {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            let is_directory = metadata.is_dir();
            self.entries.push(FileEntry {
                name: item.file_name().to_string_lossy().into_owned(),
                kind: file_kind(&path, is_directory),
                size: (!is_directory).then_some(metadata.len()),
                modified: metadata
                    .modified()
                    .map(format_system_time)
                    .unwrap_or_else(|_| "-".to_owned()),
                path,
                is_directory,
            });
        }

        self.entries.sort_by(|left, right| {
            right
                .is_directory
                .cmp(&left.is_directory)
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });
        if !self.computer_view {
            self.address_text = display_path(&self.current_directory);
        }
    }

    fn navigate_to(&mut self, path: PathBuf) {
        if !path.is_dir() {
            self.status = JobStatus::Error(format!("不是有效目录：{}", path.display()));
            return;
        }
        let path = path.canonicalize().unwrap_or(path);
        self.clear_archive_view();
        if path == self.current_directory {
            self.refresh_entries();
            return;
        }

        self.history.truncate(self.history_index + 1);
        self.history.push(path.clone());
        self.history_index = self.history.len() - 1;
        self.current_directory = path;
        self.reveal_tree_path(&self.current_directory.clone());
        self.computer_view = false;
        self.selected_drive = None;
        self.selected_paths.clear();
        self.list_scroll = 0;
        self.refresh_entries();
        self.status = JobStatus::Idle;
    }

    fn navigate_history(&mut self, index: usize) {
        if let Some(path) = self.history.get(index).cloned() {
            self.clear_archive_view();
            self.history_index = index;
            self.computer_view = path.as_os_str().is_empty();
            if !self.computer_view {
                self.current_directory = path;
                self.reveal_tree_path(&self.current_directory.clone());
            }
            self.selected_paths.clear();
            self.selected_drive = if self.computer_view {
                drive_entries().first().map(|drive| drive.path.clone())
            } else {
                None
            };
            self.list_scroll = 0;
            self.refresh_entries();
            if self.computer_view {
                self.address_text = "此电脑".to_owned();
            }
        }
    }

    fn navigate_computer(&mut self) {
        self.clear_archive_view();
        if self.computer_view {
            return;
        }
        self.history.truncate(self.history_index + 1);
        self.history.push(PathBuf::new());
        self.history_index = self.history.len() - 1;
        self.computer_view = true;
        self.selected_paths.clear();
        self.selected_drive = drive_entries().first().map(|drive| drive.path.clone());
        self.reveal_tree_selection = true;
        self.address_text = "此电脑".to_owned();
        self.list_scroll = 0;
        self.status = JobStatus::Idle;
    }

    fn toggle_selection(&mut self, path: &Path, additive: bool) {
        if additive {
            if let Some(index) = self
                .selected_paths
                .iter()
                .position(|selected| selected == path)
            {
                self.selected_paths.remove(index);
            } else {
                self.selected_paths.push(path.to_path_buf());
            }
        } else {
            self.selected_paths.clear();
            self.selected_paths.push(path.to_path_buf());
        }
    }

    fn handle_dropped_files(&mut self, context: &egui::Context) {
        if self.is_running() {
            return;
        }
        let paths: Vec<PathBuf> = context.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .map(|file| file.path().to_path_buf())
                .collect()
        });
        if paths.is_empty() {
            return;
        }
        if self.opened_archive.is_some() {
            self.start_zip_update(context, paths, HashSet::new());
            return;
        }
        if paths.len() == 1 && is_archive_file(&paths[0]) {
            self.open_archive(paths[0].clone());
        } else {
            self.sources = paths;
            self.prepare_archive_destination();
            self.show_create_dialog = true;
        }
    }

    fn prepare_add_dialog(&mut self) {
        if let Some(archive) = &self.opened_archive {
            if ArchiveFormat::from_path(archive) != Some(ArchiveFormat::Zip) {
                self.status = JobStatus::Error("目前仅支持向 ZIP 压缩包中添加文件".to_owned());
                return;
            }
            self.sources.clear();
            self.selected_source = None;
            self.show_add_to_archive_dialog = true;
            return;
        }
        if self.selected_paths.is_empty() {
            if let Some(files) = FileDialog::new().pick_files() {
                self.sources = files;
            } else {
                return;
            }
        } else {
            self.sources = self.selected_paths.clone();
        }
        self.prepare_archive_destination();
        self.show_create_dialog = true;
    }

    fn start_zip_update(
        &mut self,
        context: &egui::Context,
        additions: Vec<PathBuf>,
        removed: HashSet<String>,
    ) -> bool {
        let Some(archive_path) = self.opened_archive.clone() else {
            return false;
        };
        if ArchiveFormat::from_path(&archive_path) != Some(ArchiveFormat::Zip) {
            self.status = JobStatus::Error("目前仅支持编辑 ZIP 压缩包".to_owned());
            return false;
        }
        if !additions.is_empty()
            && self.archive_entries.iter().any(|entry| entry.encrypted)
            && self.archive_password.is_empty()
        {
            self.password_dialog_mode = PasswordDialogMode::Unlock;
            self.show_password_dialog = true;
            self.status = JobStatus::Error("请先输入 ZIP 密码，再添加文件".to_owned());
            return false;
        }
        let target_prefix = self.archive_directory.clone();
        let compression_level = self.compression_level;
        let password = (!self.archive_password.is_empty()).then(|| self.archive_password.clone());
        let (sender, receiver) = mpsc::channel();
        let repaint_context = context.clone();
        self.worker = Some(receiver);
        self.reload_archive_after_job = true;
        self.status = JobStatus::Running {
            operation: if removed.is_empty() {
                "正在添加文件"
            } else if additions.is_empty() {
                "正在从压缩包删除"
            } else {
                "正在更新压缩包"
            },
            completed: 0,
            total: 0,
            current: String::new(),
        };
        self.show_progress_dialog = true;
        thread::spawn(move || {
            let result = archive::update_zip_archive(
                &archive_path,
                &additions,
                &removed,
                &target_prefix,
                compression_level,
                password.as_deref(),
                |progress| {
                    let _ = sender.send(WorkerMessage::Progress(progress));
                    repaint_context.request_repaint();
                },
            )
            .map_err(|error| format!("{error:#}"));
            let _ = sender.send(WorkerMessage::Finished(result));
            repaint_context.request_repaint();
        });
        true
    }

    fn prepare_archive_destination(&mut self) {
        self.archive_output = Some(suggested_archive_path(
            &self.sources,
            &self.current_directory,
            self.archive_format,
        ));
    }

    fn prepare_extract_dialog(&mut self) {
        if let Some(path) = self.opened_archive.clone() {
            self.prepare_extract_path(path);
            return;
        }
        let selected_archive = self
            .selected_paths
            .iter()
            .find(|path| is_archive_file(path))
            .cloned()
            .or_else(|| {
                FileDialog::new()
                    .add_filter(
                        "支持的压缩包",
                        &[
                            "zip", "7z", "rar", "tar", "gz", "bz2", "xz", "zst", "tgz", "tbz",
                            "tbz2", "txz", "tzst",
                        ],
                    )
                    .pick_file()
            });
        if let Some(path) = selected_archive {
            self.prepare_extract_path(path);
        }
    }

    fn prepare_extract_path(&mut self, path: PathBuf) {
        let folder_name = archive_base_name(&path);
        let parent = path.parent().unwrap_or(&self.current_directory);
        self.extract_output = Some(parent.join(folder_name));
        self.zip_input = Some(path);
        self.show_extract_dialog = true;
    }

    fn clear_archive_view(&mut self) {
        self.opened_archive = None;
        self.pending_risky_archive_open = None;
        self.archive_directory.clear();
        self.archive_entries.clear();
        self.archive_listing_worker = None;
        self.selected_archive_item = None;
        self.archive_password.clear();
        self.current_archive_password.clear();
        self.show_password_dialog = false;
    }

    fn open_archive(&mut self, path: PathBuf) {
        let path = path.canonicalize().unwrap_or(path);
        self.clear_archive_view();
        self.current_directory = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        self.reveal_tree_path(&self.current_directory.clone());
        self.computer_view = false;
        self.selected_drive = None;
        self.selected_paths.clear();
        self.list_scroll = 0;
        self.opened_archive = Some(path);
        self.status = JobStatus::Idle;
        self.load_archive_listing();
    }

    fn load_archive_listing(&mut self) {
        let Some(path) = self.opened_archive.clone() else {
            return;
        };
        self.entries.clear();
        self.archive_entries.clear();
        self.browser_error = None;
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let result = archive::list_archive_entries(&path).map_err(|error| format!("{error:#}"));
            let _ = sender.send(result);
        });
        self.archive_listing_worker = Some(receiver);
        self.refresh_archive_directory();
    }

    fn poll_archive_listing(&mut self) {
        let result = self
            .archive_listing_worker
            .as_ref()
            .and_then(|worker| worker.try_recv().ok());
        if let Some(result) = result {
            self.archive_listing_worker = None;
            match result {
                Ok(entries) => {
                    let encrypted = entries.iter().any(|entry| entry.encrypted);
                    self.archive_entries = entries;
                    self.refresh_archive_directory();
                    self.status = JobStatus::Idle;
                    if encrypted && self.archive_password.is_empty() {
                        self.password_dialog_mode = PasswordDialogMode::Unlock;
                        self.current_archive_password.clear();
                        self.show_password_dialog = true;
                    }
                }
                Err(error) => {
                    self.browser_error = Some(format!("无法读取压缩包：{error}"));
                    self.status = JobStatus::Error(format!("无法读取压缩包：{error}"));
                }
            }
        }
    }

    fn refresh_archive_directory(&mut self) {
        self.entries = archive_children(&self.archive_entries, &self.archive_directory);
        if let Some(path) = &self.opened_archive {
            self.address_text = if self.archive_directory.is_empty() {
                display_path(path)
            } else {
                format!("{} / {}", display_path(path), self.archive_directory)
            };
        }
        self.selected_archive_item = None;
        self.list_scroll = 0;
    }

    fn enter_archive_directory(&mut self, path: &Path) {
        self.archive_directory = path.to_string_lossy().replace('\\', "/");
        self.refresh_archive_directory();
    }

    fn request_open_archive_entry(&mut self, path: PathBuf) {
        if risky_external_file(&path) {
            self.pending_risky_archive_open = Some(path);
        } else {
            self.start_archive_entry_preview(path);
        }
    }

    fn start_archive_entry_preview(&mut self, entry: PathBuf) {
        let Some(archive_path) = self.opened_archive.clone() else {
            return;
        };
        if self.archive_preview_worker.is_some() {
            self.status = JobStatus::Error("正在准备上一个文件，请稍候".to_owned());
            return;
        }
        let entry_name = entry.to_string_lossy().into_owned();
        let password = (!self.archive_password.is_empty()).then(|| self.archive_password.clone());
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let result = (|| -> anyhow::Result<(PathBuf, TempDir, PathBuf)> {
                let temporary = tempfile::tempdir()?;
                let file = archive::extract_entry_for_open_with_password(
                    &archive_path,
                    &entry_name,
                    temporary.path(),
                    password.as_deref(),
                )?;
                Ok((archive_path, temporary, file))
            })()
            .map_err(|error| format!("{error:#}"));
            let _ = sender.send(result);
        });
        self.archive_preview_worker = Some(receiver);
        self.status = JobStatus::Idle;
    }

    fn poll_archive_preview(&mut self) {
        let Some(worker) = &self.archive_preview_worker else {
            return;
        };
        let result = match worker.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.archive_preview_worker = None;
                self.status = JobStatus::Error("包内文件预览任务意外中断".to_owned());
                return;
            }
        };
        self.archive_preview_worker = None;
        match result {
            Ok((archive, temporary, file)) if self.opened_archive.as_ref() == Some(&archive) => {
                match crate::system_open::open_file(&file) {
                    Ok(()) => {
                        self.archive_preview_directories.push(temporary);
                        self.status = JobStatus::Success(
                            "已用系统默认应用打开临时副本；修改不会写回压缩包".to_owned(),
                        );
                    }
                    Err(error) => {
                        self.status = JobStatus::Error(format!("无法打开包内文件：{error}"));
                    }
                }
            }
            Ok(_) => {}
            Err(error) => {
                self.status = JobStatus::Error(format!("无法准备包内文件：{error}"));
            }
        }
    }

    fn show_archive_open_confirmation(&mut self, context: &egui::Context) {
        let Some(path) = self.pending_risky_archive_open.clone() else {
            return;
        };
        let mut accepted = false;
        let close = show_native_popup(
            context,
            "miaozip_archive_external_open_warning",
            "打开包内文件 - 妙压",
            [440.0, 230.0],
            |ui, close| {
                operation_heading(ui, "打开包内文件", "确认运行可能执行代码的文件");
                ui.add_space(8.0);
                ui.label(format!("文件：{}", path.display()));
                ui.add_space(6.0);
                ui.colored_label(
                    egui::Color32::from_rgb(173, 72, 36),
                    "此文件来自压缩包。打开它可能运行程序或脚本。",
                );
                ui.weak("程序只会打开临时副本，修改不会写回压缩包。");
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if ui.button("取消").clicked() {
                        *close = true;
                    }
                    if ui.button("仍要打开").clicked() {
                        accepted = true;
                        *close = true;
                    }
                });
            },
        );
        if close {
            self.pending_risky_archive_open = None;
            if accepted {
                self.start_archive_entry_preview(path);
            }
        }
    }

    fn archive_up(&mut self) {
        if self.archive_directory.is_empty() {
            self.close_archive();
        } else {
            self.archive_directory = self
                .archive_directory
                .rsplit_once('/')
                .map(|(parent, _)| parent.to_owned())
                .unwrap_or_default();
            self.refresh_archive_directory();
        }
    }

    fn close_archive(&mut self) {
        if let Some(path) = self.opened_archive.clone()
            && let Some(parent) = path.parent()
        {
            self.navigate_to(parent.to_path_buf());
        }
    }

    fn poll_worker(&mut self) {
        while let Some(receiver) = self.worker.as_ref() {
            let message = receiver.try_recv();
            match message {
                Ok(WorkerMessage::Progress(progress)) => {
                    if let JobStatus::Running {
                        completed,
                        total,
                        current,
                        ..
                    } = &mut self.status
                    {
                        *completed = progress.completed;
                        *total = progress.total;
                        *current = progress.current;
                    }
                }
                Ok(WorkerMessage::Finished(result)) => {
                    self.worker = None;
                    self.refresh_after_job = true;
                    if result.is_ok()
                        && let Some(password) = self.pending_password_after_job.take()
                    {
                        self.archive_password = password;
                    } else if result.is_err() {
                        self.pending_password_after_job = None;
                    }
                    self.status = match result {
                        Ok(summary) => JobStatus::Success(format!(
                            "操作完成：{} 个文件，{} 个目录，共 {}",
                            summary.files,
                            summary.directories,
                            format_bytes(summary.bytes)
                        )),
                        Err(error) => JobStatus::Error(error),
                    };
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.worker = None;
                    self.status = JobStatus::Error("后台任务意外终止".to_owned());
                    break;
                }
            }
        }

        if self.refresh_after_job && !self.is_running() {
            self.refresh_after_job = false;
            if self.reload_archive_after_job && self.opened_archive.is_some() {
                self.reload_archive_after_job = false;
                self.load_archive_listing();
            } else if !self.context_only_extract {
                self.refresh_entries();
            }
        }
    }

    fn start_compression(&mut self, context: &egui::Context) -> bool {
        if self.sources.is_empty() {
            self.status = JobStatus::Error("请选择要添加到压缩包的文件".to_owned());
            return false;
        }
        let Some(destination) = self.archive_output.clone() else {
            self.status = JobStatus::Error("请设置压缩文件名".to_owned());
            return false;
        };

        let sources = self.sources.clone();
        let compression_level = self.compression_level;
        let archive_format = self.archive_format;
        if !self.archive_password.is_empty() {
            if archive_format != ArchiveFormat::Zip {
                self.status = JobStatus::Error("密码压缩目前只支持 ZIP".to_owned());
                return false;
            }
            if self.archive_password != self.archive_password_confirm {
                self.status = JobStatus::Error("两次输入的密码不一致".to_owned());
                return false;
            }
        }
        let password = (!self.archive_password.is_empty()).then(|| self.archive_password.clone());
        let (sender, receiver) = mpsc::channel();
        let repaint_context = context.clone();
        self.worker = Some(receiver);
        self.status = JobStatus::Running {
            operation: "正在压缩",
            completed: 0,
            total: 0,
            current: String::new(),
        };
        self.show_progress_dialog = !self.context_only_add;

        thread::spawn(move || {
            let result = archive::create_archive_with_password(
                &sources,
                &destination,
                archive_format,
                compression_level,
                password.as_deref(),
                |progress| {
                    let _ = sender.send(WorkerMessage::Progress(progress));
                    repaint_context.request_repaint();
                },
            )
            .map_err(|error| format!("{error:#}"));
            let _ = sender.send(WorkerMessage::Finished(result));
            repaint_context.request_repaint();
        });
        true
    }

    fn start_extraction(&mut self, context: &egui::Context) -> bool {
        let Some(archive_path) = self.zip_input.clone() else {
            self.status = JobStatus::Error("请选择压缩包".to_owned());
            return false;
        };
        let Some(destination) = self.extract_output.clone() else {
            self.status = JobStatus::Error("请选择解压目录".to_owned());
            return false;
        };

        if destination.exists() && !self.overwrite_existing {
            self.status = JobStatus::Error("目标目录已存在，请允许覆盖或更换目录".to_owned());
            return false;
        }

        let (sender, receiver) = mpsc::channel();
        let repaint_context = context.clone();
        let password = (!self.archive_password.is_empty()).then(|| self.archive_password.clone());
        let self_extracting = self.extracting_sfx;
        self.worker = Some(receiver);
        self.status = JobStatus::Running {
            operation: "正在解压",
            completed: 0,
            total: 0,
            current: String::new(),
        };
        self.show_progress_dialog = true;

        thread::spawn(move || {
            let notify = |progress| {
                let _ = sender.send(WorkerMessage::Progress(progress));
                repaint_context.request_repaint();
            };
            let result = if self_extracting {
                archive::extract_self_extracting(
                    &archive_path,
                    &destination,
                    password.as_deref(),
                    notify,
                )
            } else {
                archive::extract_archive_with_password(
                    &archive_path,
                    &destination,
                    password.as_deref(),
                    notify,
                )
            }
            .map_err(|error| format!("{error:#}"));
            let _ = sender.send(WorkerMessage::Finished(result));
            repaint_context.request_repaint();
        });
        true
    }

    fn show_job_progress_dialog(&mut self, context: &egui::Context) {
        if !self.show_progress_dialog {
            return;
        }
        let (running, title, detail, fraction) = match &self.status {
            JobStatus::Running {
                operation,
                completed,
                total,
                current,
            } => (
                true,
                (*operation).to_owned(),
                if current.is_empty() {
                    "正在准备文件…".to_owned()
                } else {
                    current.clone()
                },
                if *total == 0 {
                    0.0
                } else {
                    (*completed as f32 / *total as f32).clamp(0.0, 1.0)
                },
            ),
            JobStatus::Success(message) => (false, "操作已完成".to_owned(), message.clone(), 1.0),
            JobStatus::Error(message) => (false, "操作失败".to_owned(), message.clone(), 0.0),
            JobStatus::Idle => {
                self.show_progress_dialog = false;
                return;
            }
        };
        let mut close = false;
        context.show_viewport_immediate(
            egui::ViewportId::from_hash_of("miaozip_progress"),
            egui::ViewportBuilder::default()
                .with_title("操作进度 - 妙压")
                .with_icon(crate::app_icon())
                .with_inner_size([520.0, 230.0])
                .with_min_inner_size([520.0, 230.0])
                .with_resizable(false),
            |viewport, _class| {
                if viewport.input(|input| input.viewport().close_requested()) {
                    close = true;
                }
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(BLUE).inner_margin(0))
                    .show(viewport, |ui| {
                        let rect = ui.max_rect();
                        let painter = ui.painter_at(rect);
                        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(13, 120, 200));
                        painter.add(egui::Shape::convex_polygon(
                            vec![
                                egui::pos2(rect.left() + 265.0, rect.top()),
                                egui::pos2(rect.right(), rect.top()),
                                egui::pos2(rect.right() - 120.0, rect.bottom()),
                                egui::pos2(rect.left() + 95.0, rect.bottom()),
                            ],
                            egui::Color32::from_white_alpha(14),
                            egui::Stroke::NONE,
                        ));
                        painter.text(
                            egui::pos2(rect.left() + 24.0, rect.top() + 20.0),
                            egui::Align2::LEFT_TOP,
                            "▣  妙压",
                            egui::FontId::proportional(15.0),
                            egui::Color32::WHITE,
                        );
                        painter.text(
                            egui::pos2(rect.left() + 30.0, rect.top() + 77.0),
                            egui::Align2::LEFT_TOP,
                            &title,
                            egui::FontId::proportional(22.0),
                            egui::Color32::WHITE,
                        );
                        painter.text(
                            egui::pos2(rect.right() - 25.0, rect.top() + 51.0),
                            egui::Align2::RIGHT_TOP,
                            format!("{}%", (fraction * 100.0).round() as u32),
                            egui::FontId::proportional(48.0),
                            egui::Color32::WHITE,
                        );
                        let detail = if detail.chars().count() > 56 {
                            format!("{}…", detail.chars().take(55).collect::<String>())
                        } else {
                            detail.clone()
                        };
                        painter.text(
                            egui::pos2(rect.left() + 30.0, rect.top() + 118.0),
                            egui::Align2::LEFT_TOP,
                            &detail,
                            egui::FontId::proportional(13.0),
                            egui::Color32::from_rgb(225, 242, 255),
                        );
                        let track = egui::Rect::from_min_size(
                            egui::pos2(rect.left() + 30.0, rect.top() + 157.0),
                            egui::vec2(rect.width() - 60.0, 7.0),
                        );
                        painter.rect_filled(track, 4.0, egui::Color32::from_white_alpha(90));
                        painter.rect_filled(
                            egui::Rect::from_min_size(
                                track.min,
                                egui::vec2(track.width() * fraction, track.height()),
                            ),
                            4.0,
                            egui::Color32::WHITE,
                        );
                        if !running {
                            let button = egui::Rect::from_min_size(
                                egui::pos2(rect.right() - 110.0, rect.bottom() - 46.0),
                                egui::vec2(80.0, 27.0),
                            );
                            let response = ui.interact(
                                button,
                                egui::Id::new("progress_close"),
                                egui::Sense::click(),
                            );
                            painter.rect_filled(button, 3.0, egui::Color32::WHITE);
                            painter.text(
                                button.center(),
                                egui::Align2::CENTER_CENTER,
                                "关闭",
                                egui::FontId::proportional(14.0),
                                BLUE,
                            );
                            if response.clicked() {
                                close = true;
                            }
                        } else {
                            painter.text(
                                egui::pos2(rect.left() + 30.0, rect.bottom() - 31.0),
                                egui::Align2::LEFT_TOP,
                                "任务在后台执行，关闭窗口不会取消操作",
                                egui::FontId::proportional(12.0),
                                egui::Color32::from_rgb(218, 237, 253),
                            );
                        }
                    });
            },
        );
        if close {
            self.show_progress_dialog = false;
        }
    }

    fn delete_selected(&mut self, context: &egui::Context) {
        if self.opened_archive.is_some() {
            let Some(selected) = self.selected_archive_item.take() else {
                self.status = JobStatus::Error("请先选择压缩包内项目".to_owned());
                return;
            };
            let mut removed = HashSet::new();
            removed.insert(selected.to_string_lossy().replace('\\', "/"));
            self.start_zip_update(context, Vec::new(), removed);
            return;
        }
        let mut deleted = 0;
        let mut errors = Vec::new();
        for path in &self.selected_paths {
            let result = if path.is_dir() {
                fs::remove_dir_all(path)
            } else {
                fs::remove_file(path)
            };
            match result {
                Ok(()) => deleted += 1,
                Err(error) => errors.push(format!("{}：{error}", path.display())),
            }
        }
        self.selected_paths.clear();
        self.refresh_entries();
        self.status = if errors.is_empty() {
            JobStatus::Success(format!("已删除 {deleted} 项"))
        } else {
            JobStatus::Error(format!("部分项目删除失败：{}", errors.join("；")))
        };
    }

    fn show_quick_create_contents(
        &mut self,
        context: &egui::Context,
        ui: &mut egui::Ui,
        close_after_start: &mut bool,
    ) {
        if self.compression_level != 1 && self.compression_level != 9 {
            self.compression_level = if self.compression_level >= 8 { 9 } else { 1 };
        }
        branded_banner(ui, "压得巧，解得快", "妙压  ·  简洁压缩");
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            ui.label("压缩到：");
            let mut text = self
                .archive_output
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default();
            if ui
                .add_sized([430.0, 28.0], egui::TextEdit::singleline(&mut text))
                .changed()
            {
                self.archive_output = Some(PathBuf::from(text));
            }
            if ui.button("更换目录").clicked()
                && let Some(folder) = FileDialog::new().pick_folder()
            {
                let file_name = self
                    .archive_output
                    .as_ref()
                    .and_then(|path| path.file_name())
                    .map(|name| name.to_owned())
                    .unwrap_or_else(|| {
                        format!("新建压缩文件{}", self.archive_format.extension()).into()
                    });
                self.archive_output = Some(folder.join(file_name));
            }
        });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.label("压缩格式：");
            let previous = self.archive_format;
            egui::ComboBox::from_id_salt("quick_archive_format")
                .selected_text(self.archive_format.label())
                .width(110.0)
                .show_ui(ui, |ui| {
                    for format in ArchiveFormat::CREATABLE {
                        ui.selectable_value(&mut self.archive_format, format, format.label());
                    }
                });
            if previous != self.archive_format
                && let Some(path) = self.archive_output.as_mut()
            {
                *path = replace_archive_extension(path, previous, self.archive_format);
            }
            ui.add_space(24.0);
            ui.label("压缩方式：");
            ui.add_enabled_ui(
                !matches!(
                    self.archive_format,
                    ArchiveFormat::SevenZip | ArchiveFormat::Tar
                ),
                |ui| {
                    ui.radio_value(&mut self.compression_level, 1, "速度快");
                    ui.radio_value(&mut self.compression_level, 9, "体积小");
                },
            );
        });
        ui.add_space(6.0);
        ui.weak(match self.archive_format {
            ArchiveFormat::SevenZip => "7Z 使用固定 LZMA2 配置。",
            ArchiveFormat::Tar => "TAR 仅打包，不进行压缩。",
            _ => "速度快适合日常使用；体积小会花费更多时间。",
        });
        ui.add_space(18.0);
        ui.horizontal(|ui| {
            ui.label(format!("待压缩 {} 项", self.sources.len()));
            ui.add_space(14.0);
            if !self.context_only_add && ui.link("切换至经典模式").clicked() {
                self.quick_create_mode = false;
            }
            if !self.context_only_add
                && self.archive_format == ArchiveFormat::Zip
                && ui.link("设置密码").clicked()
            {
                self.quick_create_mode = false;
                self.create_dialog_tab = 0;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        !self.is_running(),
                        egui::Button::new(
                            egui::RichText::new("立即压缩")
                                .color(egui::Color32::WHITE)
                                .strong(),
                        )
                        .fill(BLUE)
                        .min_size(egui::vec2(110.0, 35.0)),
                    )
                    .clicked()
                    && self.start_compression(context)
                {
                    *close_after_start = true;
                }
                if self.context_only_add && ui.button("取消").clicked() {
                    *close_after_start = true;
                }
            });
        });
        ui.add_space(11.0);
        ui.separator();
        if !self.context_only_add {
            ui.weak("需要查看或调整文件列表？请切换至经典模式。 ");
        }
    }

    fn show_context_add_root(&mut self, root: &mut egui::Ui) {
        let context = root.ctx().clone();
        if self.is_running() && root.input(|input| input.viewport().close_requested()) {
            context.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        let mut close = false;
        egui::CentralPanel::default()
            .frame(operation_dialog_frame())
            .show(root, |ui| {
                if self.context_add_started {
                    let (title, detail, fraction, finished) = match &self.status {
                        JobStatus::Running {
                            completed,
                            total,
                            current,
                            ..
                        } => (
                            "正在压缩",
                            if current.is_empty() {
                                "正在准备文件…".to_owned()
                            } else {
                                current.clone()
                            },
                            if *total == 0 {
                                0.0
                            } else {
                                (*completed as f32 / *total as f32).clamp(0.0, 1.0)
                            },
                            false,
                        ),
                        JobStatus::Success(message) => ("压缩完成", message.clone(), 1.0, true),
                        JobStatus::Error(message) => ("压缩失败", message.clone(), 0.0, true),
                        JobStatus::Idle => ("压缩文件", String::new(), 0.0, true),
                    };
                    operation_heading(ui, title, "右键添加到压缩包");
                    ui.add_space(24.0);
                    ui.label(detail);
                    ui.add_space(20.0);
                    ui.add(
                        egui::ProgressBar::new(fraction)
                            .show_percentage()
                            .desired_width(ui.available_width()),
                    );
                    ui.add_space(28.0);
                    if finished {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("关闭").clicked() {
                                close = true;
                            }
                        });
                    } else {
                        ui.weak("压缩正在进行，完成后可关闭此窗口。");
                    }
                } else if self.show_create_dialog {
                    self.show_quick_create_contents(&context, ui, &mut close);
                    if self.is_running() {
                        self.context_add_started = true;
                    } else if let JobStatus::Error(message) = &self.status {
                        ui.colored_label(egui::Color32::from_rgb(181, 61, 53), message);
                    }
                } else {
                    operation_heading(ui, "无法压缩", "右键添加到压缩包");
                    if let JobStatus::Error(message) = &self.status {
                        ui.colored_label(egui::Color32::from_rgb(181, 61, 53), message);
                    }
                    if ui.button("关闭").clicked() {
                        close = true;
                    }
                }
            });
        if close && !self.is_running() {
            context.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn show_context_extract_root(&mut self, root: &mut egui::Ui) {
        let context = root.ctx().clone();
        if self.is_running() && root.input(|input| input.viewport().close_requested()) {
            context.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }

        if matches!(self.status, JobStatus::Success(_)) {
            context.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        let (running, title, detail, fraction) = match &self.status {
            JobStatus::Running {
                completed,
                total,
                current,
                ..
            } => (
                true,
                "正在解压",
                if current.is_empty() {
                    "正在准备文件…".to_owned()
                } else {
                    current.clone()
                },
                if *total == 0 {
                    0.0
                } else {
                    (*completed as f32 / *total as f32).clamp(0.0, 1.0)
                },
            ),
            JobStatus::Error(message) => (false, "解压失败", message.clone(), 0.0),
            JobStatus::Idle => (false, "无法解压", "解压任务未能启动".to_owned(), 0.0),
            JobStatus::Success(_) => unreachable!("successful quick extraction closes above"),
        };

        let mut close = false;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BLUE).inner_margin(0))
            .show(root, |ui| {
                let rect = ui.max_rect();
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(13, 120, 200));
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        egui::pos2(rect.left() + 265.0, rect.top()),
                        egui::pos2(rect.right(), rect.top()),
                        egui::pos2(rect.right() - 120.0, rect.bottom()),
                        egui::pos2(rect.left() + 95.0, rect.bottom()),
                    ],
                    egui::Color32::from_white_alpha(14),
                    egui::Stroke::NONE,
                ));
                painter.text(
                    egui::pos2(rect.left() + 24.0, rect.top() + 20.0),
                    egui::Align2::LEFT_TOP,
                    "▣  妙压",
                    egui::FontId::proportional(15.0),
                    egui::Color32::WHITE,
                );
                painter.text(
                    egui::pos2(rect.left() + 30.0, rect.top() + 77.0),
                    egui::Align2::LEFT_TOP,
                    title,
                    egui::FontId::proportional(22.0),
                    egui::Color32::WHITE,
                );
                painter.text(
                    egui::pos2(rect.right() - 25.0, rect.top() + 51.0),
                    egui::Align2::RIGHT_TOP,
                    format!("{}%", (fraction * 100.0).round() as u32),
                    egui::FontId::proportional(48.0),
                    egui::Color32::WHITE,
                );
                let detail = if detail.chars().count() > 56 {
                    format!("{}…", detail.chars().take(55).collect::<String>())
                } else {
                    detail
                };
                painter.text(
                    egui::pos2(rect.left() + 30.0, rect.top() + 118.0),
                    egui::Align2::LEFT_TOP,
                    detail,
                    egui::FontId::proportional(13.0),
                    egui::Color32::from_rgb(225, 242, 255),
                );
                let track = egui::Rect::from_min_size(
                    egui::pos2(rect.left() + 30.0, rect.top() + 157.0),
                    egui::vec2(rect.width() - 60.0, 7.0),
                );
                painter.rect_filled(track, 4.0, egui::Color32::from_white_alpha(90));
                painter.rect_filled(
                    egui::Rect::from_min_size(
                        track.min,
                        egui::vec2(track.width() * fraction, track.height()),
                    ),
                    4.0,
                    egui::Color32::WHITE,
                );
                if running {
                    painter.text(
                        egui::pos2(rect.left() + 30.0, rect.bottom() - 31.0),
                        egui::Align2::LEFT_TOP,
                        "解压完成后窗口将自动关闭",
                        egui::FontId::proportional(12.0),
                        egui::Color32::from_rgb(218, 237, 253),
                    );
                } else {
                    let button = egui::Rect::from_min_size(
                        egui::pos2(rect.right() - 110.0, rect.bottom() - 46.0),
                        egui::vec2(80.0, 27.0),
                    );
                    let response = ui.interact(
                        button,
                        egui::Id::new("quick_extract_close"),
                        egui::Sense::click(),
                    );
                    painter.rect_filled(button, 3.0, egui::Color32::WHITE);
                    painter.text(
                        button.center(),
                        egui::Align2::CENTER_CENTER,
                        "关闭",
                        egui::FontId::proportional(14.0),
                        BLUE,
                    );
                    close = response.clicked();
                }
            });
        if close {
            context.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn show_create_archive_dialog(&mut self, context: &egui::Context) {
        if !self.show_create_dialog {
            return;
        }
        let mut close_after_start = false;
        let dialog_size = if self.quick_create_mode {
            [630.0, 345.0]
        } else {
            [650.0, 480.0]
        };
        context.show_viewport_immediate(
            egui::ViewportId::from_hash_of("miaozip_create_archive"),
            egui::ViewportBuilder::default()
                .with_title("压缩文件 - 妙压")
                .with_icon(crate::app_icon())
                .with_inner_size(dialog_size)
                .with_min_inner_size([630.0, 345.0])
                .with_resizable(false),
            |viewport, _class| {
                if viewport.input(|input| input.viewport().close_requested()) {
                    close_after_start = true;
                }
                egui::CentralPanel::default()
                    .frame(operation_dialog_frame())
                    .show(viewport, |ui| {
                        if self.quick_create_mode {
                            self.show_quick_create_contents(context, ui, &mut close_after_start);
                            return;
                        }
                        operation_heading(ui, "压缩文件名和参数", "选择压缩格式、方式及待添加文件");
                        operation_tabs(ui, &mut self.create_dialog_tab, &["常规", "文件"]);
                        ui.add_space(10.0);
                        ui.label("压缩文件名及路径：");
                        ui.horizontal(|ui| {
                            let mut text = self
                                .archive_output
                                .as_ref()
                                .map(|path| path.display().to_string())
                                .unwrap_or_default();
                            if ui
                                .add_sized([522.0, 26.0], egui::TextEdit::singleline(&mut text))
                                .changed()
                            {
                                self.archive_output = Some(PathBuf::from(text));
                            }
                            if ui
                                .add_sized([70.0, 26.0], egui::Button::new("浏览..."))
                                .clicked()
                                && let Some(path) = FileDialog::new()
                                    .add_filter(
                                        "所选格式",
                                        &[self.archive_format.extension().trim_start_matches('.')],
                                    )
                                    .set_file_name(format!(
                                        "新建压缩文件{}",
                                        self.archive_format.extension()
                                    ))
                                    .save_file()
                            {
                                self.archive_output =
                                    Some(ensure_archive_extension(path, self.archive_format));
                            }
                        });
                        ui.add_space(12.0);
                        let previous = self.archive_format;
                        if self.create_dialog_tab == 0 {
                            ui.columns(2, |columns| {
                                columns[0].group(|ui| {
                                    ui.set_min_height(230.0);
                                    ui.label(egui::RichText::new("压缩格式").strong());
                                    ui.add_space(5.0);
                                    ui.horizontal(|ui| {
                                        ui.radio_value(
                                            &mut self.archive_format,
                                            ArchiveFormat::Zip,
                                            "ZIP",
                                        );
                                        ui.radio_value(
                                            &mut self.archive_format,
                                            ArchiveFormat::SevenZip,
                                            "7Z",
                                        );
                                        ui.radio_value(
                                            &mut self.archive_format,
                                            ArchiveFormat::Tar,
                                            "TAR",
                                        );
                                    });
                                    ui.add_space(5.0);
                                    ui.horizontal(|ui| {
                                        ui.label("更多格式：");
                                        egui::ComboBox::from_id_salt("archive_format")
                                            .selected_text(self.archive_format.label())
                                            .width(128.0)
                                            .show_ui(ui, |ui| {
                                                for format in ArchiveFormat::CREATABLE {
                                                    ui.selectable_value(
                                                        &mut self.archive_format,
                                                        format,
                                                        format.label(),
                                                    );
                                                }
                                            });
                                    });
                                    ui.separator();
                                    ui.label(egui::RichText::new("压缩方式").strong());
                                    ui.add_space(4.0);
                                    ui.add_enabled_ui(
                                        !matches!(
                                            self.archive_format,
                                            ArchiveFormat::SevenZip | ArchiveFormat::Tar
                                        ),
                                        |ui| {
                                            egui::ComboBox::from_id_salt("compression_method")
                                                .selected_text(compression_label(
                                                    self.compression_level,
                                                ))
                                                .width(235.0)
                                                .show_ui(ui, |ui| {
                                                    for (level, label) in [
                                                        (0, "存储"),
                                                        (1, "最快"),
                                                        (3, "快速"),
                                                        (6, "标准"),
                                                        (8, "较好"),
                                                        (9, "最好"),
                                                    ] {
                                                        ui.selectable_value(
                                                            &mut self.compression_level,
                                                            level,
                                                            label,
                                                        );
                                                    }
                                                });
                                        },
                                    );
                                    ui.add_space(5.0);
                                    ui.weak(match self.archive_format {
                                        ArchiveFormat::SevenZip => "7Z 当前使用固定 LZMA2 配置。",
                                        ArchiveFormat::Tar => "TAR 仅打包，不进行压缩。",
                                        _ => "级别越高，压缩通常越慢。",
                                    });
                                });
                                columns[1].group(|ui| {
                            ui.set_min_height(230.0);
                            ui.label(egui::RichText::new("压缩选项").strong());
                            ui.add_space(6.0);
                            ui.label(format!("待压缩项目：{} 项", self.sources.len()));
                            ui.add_space(5.0);
                            ui.label("文件和目录将保留相对路径。 ");
                            ui.add_space(9.0);
                            ui.add_enabled_ui(self.archive_format == ArchiveFormat::Zip, |ui| {
                                ui.label(egui::RichText::new("AES-256 密码（可选）").strong());
                                ui.add_sized(
                                    [250.0, 25.0],
                                    egui::TextEdit::singleline(&mut self.archive_password)
                                        .password(true)
                                        .hint_text("输入密码"),
                                );
                                ui.add_sized(
                                    [250.0, 25.0],
                                    egui::TextEdit::singleline(
                                        &mut self.archive_password_confirm,
                                    )
                                    .password(true)
                                    .hint_text("再次输入密码"),
                                );
                            });
                            ui.weak("分卷压缩暂未实现；创建 ZIP 后可用主界面的“自解压”生成 EXE。");
                        });
                            });
                        } else {
                            ui.group(|ui| {
                                ui.set_min_height(230.0);
                                ui.label(
                                    egui::RichText::new(format!(
                                        "待压缩文件（{} 项）",
                                        self.sources.len()
                                    ))
                                    .strong(),
                                );
                                ui.separator();
                                egui::ScrollArea::vertical()
                                    .max_height(155.0)
                                    .show(ui, |ui| {
                                        for (index, source) in self.sources.iter().enumerate() {
                                            let name = source
                                                .file_name()
                                                .map(|name| name.to_string_lossy().into_owned())
                                                .unwrap_or_else(|| source.display().to_string());
                                            let label = if source.is_dir() {
                                                format!("📁 {name}")
                                            } else {
                                                format!("📄 {name}")
                                            };
                                            ui.selectable_value(
                                                &mut self.selected_source,
                                                Some(index),
                                                label,
                                            )
                                            .on_hover_text(source.display().to_string());
                                        }
                                    });
                                ui.horizontal(|ui| {
                                    if ui.button("添加文件...").clicked()
                                        && let Some(paths) = FileDialog::new().pick_files()
                                    {
                                        for path in paths {
                                            if !self.sources.contains(&path) {
                                                self.sources.push(path);
                                            }
                                        }
                                    }
                                    if ui.button("添加文件夹...").clicked()
                                        && let Some(path) = FileDialog::new().pick_folder()
                                        && !self.sources.contains(&path)
                                    {
                                        self.sources.push(path);
                                    }
                                    if ui
                                        .add_enabled(
                                            self.selected_source.is_some(),
                                            egui::Button::new("移除"),
                                        )
                                        .clicked()
                                        && let Some(index) = self.selected_source.take()
                                        && index < self.sources.len()
                                    {
                                        self.sources.remove(index);
                                    }
                                });
                            });
                        }
                        if previous != self.archive_format
                            && let Some(path) = self.archive_output.as_mut()
                        {
                            *path = replace_archive_extension(path, previous, self.archive_format);
                        }
                        ui.add_space(10.0);
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.weak(format!(
                                "{} 项  ·  {}",
                                self.sources.len(),
                                self.archive_format.label()
                            ));
                            if ui.link("切换至简洁模式").clicked() {
                                self.quick_create_mode = true;
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add_sized([76.0, 28.0], egui::Button::new("取消"))
                                        .clicked()
                                    {
                                        close_after_start = true;
                                    }
                                    if ui
                                        .add_enabled(
                                            !self.is_running(),
                                            egui::Button::new(
                                                egui::RichText::new("确定")
                                                    .color(egui::Color32::WHITE),
                                            )
                                            .fill(BLUE)
                                            .min_size(egui::vec2(76.0, 28.0)),
                                        )
                                        .clicked()
                                        && self.start_compression(context)
                                    {
                                        close_after_start = true;
                                    }
                                },
                            );
                        });
                    });
            },
        );
        self.show_create_dialog = !close_after_start;
    }

    fn show_extract_archive_dialog(&mut self, context: &egui::Context) {
        if !self.show_extract_dialog {
            return;
        }
        let mut close_after_start = false;
        context.show_viewport_immediate(
            egui::ViewportId::from_hash_of("miaozip_extract_archive"),
            egui::ViewportBuilder::default()
                .with_title("解压文件 - 妙压")
                .with_icon(crate::app_icon())
                .with_inner_size([650.0, 470.0])
                .with_min_inner_size([650.0, 470.0])
                .with_resizable(false),
            |viewport, _class| {
                if viewport.input(|input| input.viewport().close_requested()) {
                    close_after_start = true;
                }
                egui::CentralPanel::default()
                    .frame(operation_dialog_frame())
                    .show(viewport, |ui| {
                        operation_heading(ui, "解压文件", "选择解压路径与覆盖方式");
                        operation_tabs(ui, &mut self.extract_dialog_tab, &["常规", "目标目录"]);
                        ui.add_space(10.0);
                        ui.label("解压到：");
                        ui.horizontal(|ui| {
                            let mut text = self
                                .extract_output
                                .as_ref()
                                .map(|path| path.display().to_string())
                                .unwrap_or_default();
                            if ui
                                .add_sized([522.0, 26.0], egui::TextEdit::singleline(&mut text))
                                .changed()
                            {
                                self.extract_output = Some(PathBuf::from(text));
                            }
                            if ui
                                .add_sized([70.0, 26.0], egui::Button::new("浏览..."))
                                .clicked()
                                && let Some(path) = FileDialog::new().pick_folder()
                            {
                                self.extract_output = Some(path);
                            }
                        });
                        if let Some(archive) = &self.zip_input {
                            let parent = archive.parent().unwrap_or(Path::new("."));
                            if ui.link("使用压缩包所在目录").clicked() {
                                self.extract_output = Some(parent.to_path_buf());
                            }
                            if self.extract_output.as_deref() == Some(parent)
                                && self.overwrite_existing
                            {
                                ui.colored_label(
                                    egui::Color32::from_rgb(171, 94, 25),
                                    "注意：解压到当前目录时，同名文件可能被覆盖。",
                                );
                            }
                        }
                        ui.add_space(12.0);
                        if self.extract_dialog_tab == 0 {
                            ui.columns(2, |columns| {
                                columns[0].group(|ui| {
                                    ui.set_min_height(190.0);
                                    ui.label(egui::RichText::new("压缩文件").strong());
                                    ui.add_space(5.0);
                                    if let Some(path) = &self.zip_input {
                                        ui.label(
                                            path.file_name()
                                                .map(|name| name.to_string_lossy().into_owned())
                                                .unwrap_or_default(),
                                        );
                                        ui.weak(path.display().to_string());
                                    }
                                    ui.add_space(8.0);
                                    ui.label(format!(
                                        "格式：{}",
                                        if self.extracting_sfx {
                                            Some("自解压 ZIP")
                                        } else {
                                            self.zip_input
                                                .as_deref()
                                                .and_then(ArchiveFormat::from_path)
                                                .map(ArchiveFormat::label)
                                        }
                                        .unwrap_or("未知")
                                    ));
                                });
                                columns[1].group(|ui| {
                                    ui.set_min_height(190.0);
                                    ui.label(egui::RichText::new("覆盖方式").strong());
                                    ui.add_space(5.0);
                                    ui.radio_value(
                                        &mut self.overwrite_existing,
                                        true,
                                        "覆盖已有目标目录",
                                    );
                                    ui.radio_value(
                                        &mut self.overwrite_existing,
                                        false,
                                        "目标目录已存在时停止",
                                    );
                                    ui.add_space(10.0);
                                    if self.extracting_sfx
                                        || self
                                            .zip_input
                                            .as_deref()
                                            .and_then(ArchiveFormat::from_path)
                                            == Some(ArchiveFormat::Zip)
                                    {
                                        ui.label("密码（如有）：");
                                        ui.add_sized(
                                            [255.0, 25.0],
                                            egui::TextEdit::singleline(&mut self.archive_password)
                                                .password(true),
                                        );
                                        ui.add_space(6.0);
                                    }
                                    ui.weak("解压前检查压缩包路径，阻止写入目标目录之外。");
                                });
                            });
                        } else {
                            ui.group(|ui| {
                                ui.set_min_height(190.0);
                                ui.label(egui::RichText::new("快速选择目标目录").strong());
                                ui.add_space(8.0);
                                if let Some(archive) = &self.zip_input {
                                    let parent = archive.parent().unwrap_or(Path::new("."));
                                    if ui
                                        .button(format!("解压到当前目录  {}", parent.display()))
                                        .clicked()
                                    {
                                        self.extract_output = Some(parent.to_path_buf());
                                    }
                                    ui.add_space(5.0);
                                    let named_folder = parent.join(archive_base_name(archive));
                                    if ui
                                        .button(format!(
                                            "解压到同名文件夹  {}",
                                            named_folder.display()
                                        ))
                                        .clicked()
                                    {
                                        self.extract_output = Some(named_folder);
                                    }
                                }
                            });
                        }
                        ui.add_space(10.0);
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.weak(if self.overwrite_existing {
                                "允许目标目录已存在"
                            } else {
                                "目标目录必须不存在"
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add_sized([76.0, 28.0], egui::Button::new("取消"))
                                        .clicked()
                                    {
                                        close_after_start = true;
                                    }
                                    if ui
                                        .add_enabled(
                                            !self.is_running(),
                                            egui::Button::new(
                                                egui::RichText::new("确定")
                                                    .color(egui::Color32::WHITE),
                                            )
                                            .fill(BLUE)
                                            .min_size(egui::vec2(76.0, 28.0)),
                                        )
                                        .clicked()
                                        && self.start_extraction(context)
                                    {
                                        close_after_start = true;
                                    }
                                },
                            );
                        });
                    });
            },
        );
        self.show_extract_dialog = !close_after_start;
    }

    fn show_delete_confirmation(&mut self, context: &egui::Context) {
        if !self.show_delete_dialog {
            return;
        }
        let deleting_from_archive = self.opened_archive.is_some();
        let mut confirm = false;
        let close = show_native_popup(
            context,
            "miaozip_delete",
            "确认删除 - 妙压",
            [440.0, 225.0],
            |ui, close| {
                operation_heading(
                    ui,
                    if deleting_from_archive {
                        "从压缩包删除"
                    } else {
                        "确认删除"
                    },
                    "此操作不可撤销",
                );
                ui.add_space(6.0);
                ui.group(|ui| {
                    ui.set_min_width(ui.available_width() - 12.0);
                    ui.colored_label(
                        egui::Color32::from_rgb(182, 59, 55),
                        egui::RichText::new(if deleting_from_archive {
                            format!(
                                "即将从 ZIP 中移除 {} 个项目",
                                usize::from(self.selected_archive_item.is_some())
                            )
                        } else {
                            format!("即将永久删除 {} 个项目", self.selected_paths.len())
                        })
                        .strong()
                        .size(17.0),
                    );
                    ui.add_space(5.0);
                    ui.label(if deleting_from_archive {
                        "将重写 ZIP 并移除所选条目；原文件会在新 ZIP 写入成功前保留。"
                    } else {
                        "文件不会移入系统回收站，删除后无法在应用内恢复。"
                    });
                });
                ui.add_space(15.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("取消").clicked() {
                        *close = true;
                    }
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(if deleting_from_archive {
                                    "从 ZIP 删除"
                                } else {
                                    "永久删除"
                                })
                                .color(egui::Color32::WHITE),
                            )
                            .fill(egui::Color32::from_rgb(191, 67, 61)),
                        )
                        .clicked()
                    {
                        confirm = true;
                    }
                });
            },
        );
        if confirm {
            self.delete_selected(context);
        }
        self.show_delete_dialog = !close && !confirm;
    }

    fn show_information_dialog(&mut self, context: &egui::Context) {
        if !self.show_info_dialog {
            return;
        }
        let close = show_native_popup(
            context,
            "miaozip_info",
            "文件信息 - 妙压",
            [500.0, 300.0],
            |ui, _close| {
                operation_heading(ui, "文件信息", "所选对象的路径与数量");
                ui.add_space(5.0);
                ui.group(|ui| {
                    ui.set_min_width(ui.available_width() - 12.0);
                    let count = if self.selected_paths.is_empty() {
                        self.entries.len()
                    } else {
                        self.selected_paths.len()
                    };
                    ui.label(
                        egui::RichText::new(format!("{count} 项"))
                            .size(23.0)
                            .strong()
                            .color(BLUE),
                    );
                    ui.weak(if self.selected_paths.is_empty() {
                        "当前位置中的对象"
                    } else {
                        "已选择的对象"
                    });
                });
                ui.add_space(8.0);
                ui.label(egui::RichText::new("位置与文件").strong());
                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height(108.0)
                    .show(ui, |ui| {
                        if self.selected_paths.is_empty() {
                            ui.label(self.current_directory.display().to_string());
                        } else {
                            for path in &self.selected_paths {
                                ui.label(path.display().to_string());
                            }
                        }
                    });
            },
        );
        self.show_info_dialog = !close;
    }

    fn show_default_app_prompt(&mut self, context: &egui::Context) {
        if !self.show_default_prompt {
            return;
        }
        let close = show_native_popup(
            context,
            "miaozip_default_prompt",
            "默认打开方式 - 妙压",
            [570.0, 350.0],
            |ui, close| {
                operation_heading(
                    ui,
                    "将妙压设为压缩文件默认程序",
                    "仅在你点击确认后更改当前用户的文件关联",
                );
                ui.add_space(6.0);
                ui.group(|ui| {
                    ui.set_min_width(ui.available_width() - 12.0);
                    ui.label("可为 ZIP、7z、RAR、TAR 等 13 种格式设置妙压图标和双击打开方式。部分系统可能要求你再次确认默认应用选择。");
                    ui.add_space(5.0);
                    if cfg!(windows) {
                        ui.checkbox(&mut self.register_menu_with_default, "同时注册文件与文件夹右键菜单");
                    }
                    ui.checkbox(&mut self.dont_ask_again, "暂不注册时，以后也不再提示");
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("注册并设为默认").color(egui::Color32::WHITE),
                            )
                            .fill(BLUE),
                        )
                        .clicked()
                    {
                        let result =
                            integration::set_default_associations().and_then(|remaining| {
                                if cfg!(windows) && self.register_menu_with_default {
                                    integration::register_context_menu()?;
                                }
                                Ok(remaining)
                            });
                        match result {
                            Ok(remaining) => {
                                self.invalidate_shell_icons();
                                let settings_error = if cfg!(windows) && !remaining.is_empty() {
                                    integration::open_default_apps_settings().err()
                                } else {
                                    None
                                };
                                self.status = match settings_error {
                                    Some(error) => JobStatus::Error(format!(
                                        "已注册文件类型，但无法自动打开 Windows 默认应用设置：{error}"
                                    )),
                                    None if remaining.is_empty() => JobStatus::Success(
                                        "已将支持的压缩格式设为妙压默认打开方式".to_owned(),
                                    ),
                                    None => JobStatus::Success(format!(
                                        "已打开 Windows 默认应用设置，请确认这些格式：{}",
                                        remaining.join("、")
                                    )),
                                };
                                self.integration_error = None;
                                self.ask_default_on_startup = false;
                                *close = true;
                            }
                            Err(error) => {
                                let message = format!("系统集成失败：{error}");
                                self.integration_error = Some(message.clone());
                                self.status = JobStatus::Error(message);
                            }
                        }
                    }
                    if ui.button("只注册，不改默认").clicked() {
                        let result = integration::register_default_candidate().and_then(|_| {
                            if cfg!(windows) && self.register_menu_with_default {
                                integration::register_context_menu()
                            } else {
                                Ok(())
                            }
                        });
                        match result {
                            Ok(()) => {
                                self.invalidate_shell_icons();
                                self.status = JobStatus::Success(
                                    "已注册妙压打开方式，未更改默认程序".to_owned(),
                                );
                                self.integration_error = None;
                                self.ask_default_on_startup = false;
                                *close = true;
                            }
                            Err(error) => {
                                let message = format!("注册打开方式失败：{error}");
                                self.integration_error = Some(message.clone());
                                self.status = JobStatus::Error(message);
                            }
                        }
                    }
                    if ui.button("暂不设置").clicked() {
                        *close = true;
                    }
                });
                if let Some(error) = &self.integration_error {
                    ui.colored_label(egui::Color32::DARK_RED, error);
                }
                if cfg!(windows) {
                    ui.weak(
                        "已由 Windows 记录用户选择的格式，需在“设置 → 关联”打开系统默认应用设置。",
                    );
                } else if cfg!(target_os = "linux") {
                    ui.weak("通过 xdg-mime 设置当前用户的默认应用，不会修改其他用户的选择。");
                } else if cfg!(target_os = "macos") {
                    ui.weak("通过 macOS Launch Services 设置当前用户的默认打开方式。");
                }
            },
        );
        if close {
            if self.dont_ask_again {
                self.ask_default_on_startup = false;
            }
            self.show_default_prompt = false;
        }
    }

    fn show_system_integration_dialog(&mut self, context: &egui::Context) {
        if !self.show_integration_dialog {
            return;
        }
        let close = show_native_popup(
            context,
            "miaozip_integration",
            "设置 - 妙压",
            [600.0, 480.0],
            |ui, close| {
                operation_heading(ui, "设置", "系统集成与文件关联");
                operation_tabs(ui, &mut self.integration_dialog_tab, &["综合", "关联"]);
                ui.add_space(10.0);
                if self.integration_dialog_tab == 0 {
                    if !cfg!(windows) {
                        ui.label("资源管理器右键菜单注册仅适用于 Windows；请切换到“关联”设置默认打开方式。");
                    } else {
                        ui.label(egui::RichText::new("资源管理器外壳整合").strong());
                        ui.add_space(4.0);
                        let registered = integration::context_menu_registered();
                        integration_status_row(
                            ui,
                            "文件与文件夹右键菜单",
                            if registered {
                                "已注册"
                            } else {
                                "未注册或程序路径已变化"
                            },
                            registered,
                        );
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new(if registered {
                                            "重新注册右键菜单"
                                        } else {
                                            "注册 / 修复右键菜单"
                                        })
                                        .color(egui::Color32::WHITE),
                                    )
                                    .fill(BLUE),
                                )
                                .clicked()
                            {
                                match integration::register_context_menu() {
                                    Ok(()) => {
                                        self.status =
                                            JobStatus::Success("已注册妙压右键菜单".to_owned());
                                        self.integration_error = None;
                                    }
                                    Err(error) => {
                                        let message = format!("注册右键菜单失败：{error}");
                                        self.integration_error = Some(message.clone());
                                        self.status = JobStatus::Error(message);
                                    }
                                }
                            }
                            if ui.button("移除右键菜单").clicked() {
                                match integration::unregister_context_menu() {
                                    Ok(()) => {
                                        self.status =
                                            JobStatus::Success("已移除妙压右键菜单".to_owned());
                                        self.integration_error = None;
                                    }
                                    Err(error) => {
                                        let message = format!("移除右键菜单失败：{error}");
                                        self.integration_error = Some(message.clone());
                                        self.status = JobStatus::Error(message);
                                    }
                                }
                            }
                        });
                        ui.add_space(12.0);
                        ui.separator();
                        ui.checkbox(
                            &mut self.ask_default_on_startup,
                            "未注册支持的压缩格式时在启动时提示",
                        );
                        ui.weak("Windows 11 中，经典右键菜单项通常位于“显示更多选项”。");
                    }
                } else {
                    ui.label(egui::RichText::new("压缩文件关联").strong());
                    ui.add_space(4.0);
                    let candidate_registered = integration::default_candidate_registered();
                    let is_default = integration::is_default_zip();
                    let supported_count = integration::supported_association_extensions().count();
                    let default_count = integration::default_association_count();
                    integration_status_row(
                        ui,
                        "压缩文件类型注册",
                        if candidate_registered {
                            "全部已注册"
                        } else {
                            "未注册、缺少图标或路径已变化"
                        },
                        candidate_registered,
                    );
                    ui.add_space(5.0);
                    let default_status = format!("已设默认 {default_count}/{supported_count} 种");
                    integration_status_row(
                        ui,
                        if cfg!(windows) {
                            "Windows 默认应用"
                        } else {
                            "系统默认应用"
                        },
                        &default_status,
                        default_count == supported_count,
                    );
                    ui.add_space(7.0);
                    ui.group(|ui| {
                        ui.set_min_width(ui.available_width() - 12.0);
                        ui.label(egui::RichText::new("可选择的文件类型").strong().color(BLUE));
                        ui.label(
                            integration::supported_association_extensions()
                                .collect::<Vec<_>>()
                                .join(" · "),
                        );
                        ui.label(format!(
                            ".zip：{}；ISO 只提供右键挂载，不设为压缩文件默认打开方式。",
                            if is_default {
                                "已设默认"
                            } else {
                                "尚未设默认"
                            }
                        ));
                    });
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new(if candidate_registered {
                                        "修复全部格式关联"
                                    } else {
                                        "注册全部支持格式"
                                    })
                                    .color(egui::Color32::WHITE),
                                )
                                .fill(BLUE),
                            )
                            .clicked()
                        {
                            match integration::register_default_candidate() {
                                Ok(()) => {
                                    self.invalidate_shell_icons();
                                    self.status = JobStatus::Success(
                                        "支持的压缩格式已注册，未更改系统默认应用".to_owned(),
                                    );
                                    self.integration_error = None;
                                    self.ask_default_on_startup = false;
                                }
                                Err(error) => {
                                    let message = format!("注册压缩格式失败：{error}");
                                    self.integration_error = Some(message.clone());
                                    self.status = JobStatus::Error(message);
                                }
                            }
                        }
                        if cfg!(windows) && ui.button("打开 Windows 默认应用设置").clicked()
                        {
                            match integration::open_default_apps_settings() {
                                Ok(()) => {
                                    self.status = JobStatus::Success(
                                        "已打开 Windows 默认应用设置".to_owned(),
                                    );
                                    self.integration_error = None;
                                }
                                Err(error) => {
                                    let message = format!("打开默认应用设置失败：{error}");
                                    self.integration_error = Some(message.clone());
                                    self.status = JobStatus::Error(message);
                                }
                            }
                        }
                    });
                    ui.add_space(4.0);
                    if ui.button("将全部支持格式设为默认打开方式").clicked() {
                        match integration::set_default_associations() {
                            Ok(remaining) => {
                                self.invalidate_shell_icons();
                                self.status = JobStatus::Success(if remaining.is_empty() {
                                    "全部支持格式已设为妙压默认打开方式".to_owned()
                                } else {
                                    format!(
                                        "已设置可直接更改的格式；请在系统设置中手动切换：{}",
                                        remaining.join("、")
                                    )
                                });
                                self.integration_error = None;
                                self.ask_default_on_startup = false;
                            }
                            Err(error) => {
                                let message = format!("设置默认打开方式失败：{error}");
                                self.integration_error = Some(message.clone());
                                self.status = JobStatus::Error(message);
                            }
                        }
                    }
                    if cfg!(windows) {
                        ui.weak(
                            "仅在没有 Windows UserChoice 的格式上直接设置；其余格式须在系统设置中确认。",
                        );
                    } else if cfg!(target_os = "linux") {
                        ui.weak(
                            "通过 xdg-mime 写入当前用户的默认应用设置，不会修改其他用户的选择。",
                        );
                    } else if cfg!(target_os = "macos") {
                        ui.weak(
                            "通过 macOS Launch Services 设置当前用户的默认打开方式；应用需从 MiaoZip.app 启动。",
                        );
                    }
                }
                if let Some(error) = &self.integration_error {
                    ui.add_space(5.0);
                    ui.colored_label(egui::Color32::DARK_RED, error);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("关闭").clicked() {
                        *close = true;
                    }
                });
            },
        );
        self.show_integration_dialog = !close;
    }

    fn poll_optical_worker(&mut self) {
        let Some(receiver) = self.optical_worker.as_ref() else {
            return;
        };
        match receiver.try_recv() {
            Ok(result) => {
                self.optical_worker = None;
                match result {
                    Ok(Some(mounted)) => {
                        let location = mounted.location.clone();
                        self.mounted_image = Some(mounted);
                        self.status = JobStatus::Success(format!("ISO 已挂载：{location}"));
                    }
                    Ok(None) => {
                        self.mounted_image = None;
                        self.status = JobStatus::Success("ISO 已卸载".to_owned());
                    }
                    Err(error) => self.status = JobStatus::Error(error),
                }
                self.refresh_entries();
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.optical_worker = None;
                self.status = JobStatus::Error("虚拟光驱后台任务意外终止".to_owned());
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }

    fn show_toolbox_dialog(&mut self, context: &egui::Context) {
        if !self.show_toolbox_dialog {
            return;
        }
        let close = show_native_popup(
            context,
            "miaozip_toolbox",
            "工具箱 - 妙压",
            [460.0, 480.0],
            |ui, close| {
                operation_heading(ui, "工具箱", "常用功能 · 点击图标打开独立窗口");
                ui.add_space(6.0);
                ui.columns(3, |columns| {
                    if tool_tile(
                        &mut columns[0],
                        "压",
                        "添加压缩",
                        egui::Color32::from_rgb(50, 165, 232),
                    ) {
                        self.prepare_add_dialog();
                        *close = true;
                    }
                    if tool_tile(
                        &mut columns[1],
                        "解",
                        "解压文件",
                        egui::Color32::from_rgb(75, 176, 113),
                    ) {
                        self.prepare_extract_dialog();
                        *close = true;
                    }
                    if tool_tile(
                        &mut columns[2],
                        "盘",
                        "虚拟光驱",
                        egui::Color32::from_rgb(119, 139, 209),
                    ) {
                        self.show_optical_dialog = true;
                        *close = true;
                    }
                });
                ui.separator();
                ui.columns(3, |columns| {
                    if tool_tile(
                        &mut columns[0],
                        "测",
                        "ZIP 测试/修复",
                        egui::Color32::from_rgb(231, 163, 72),
                    ) {
                        self.zip_test_path = self
                            .selected_paths
                            .iter()
                            .find(|path| {
                                path.extension()
                                    .is_some_and(|value| value.eq_ignore_ascii_case("zip"))
                            })
                            .cloned();
                        self.zip_test_result = None;
                        self.zip_repair_result = None;
                        self.zip_test_error = None;
                        self.show_zip_test_dialog = true;
                        *close = true;
                    }
                    if tool_tile(
                        &mut columns[1],
                        "格",
                        "格式支持",
                        egui::Color32::from_rgb(75, 160, 198),
                    ) {
                        self.show_formats_dialog = true;
                        *close = true;
                    }
                    if tool_tile(
                        &mut columns[2],
                        "图",
                        "图片转换",
                        egui::Color32::from_rgb(157, 135, 194),
                    ) {
                        self.image_convert_items = self
                            .selected_paths
                            .iter()
                            .filter(|path| path.is_file())
                            .cloned()
                            .collect();
                        self.image_convert_feedback = None;
                        self.show_image_convert_dialog = true;
                        *close = true;
                    }
                });
                ui.separator();
                ui.columns(3, |columns| {
                    if tool_tile(
                        &mut columns[0],
                        "验",
                        "MD5 校验",
                        egui::Color32::from_rgb(46, 155, 178),
                    ) {
                        self.hash_path = self
                            .selected_paths
                            .iter()
                            .find(|path| path.is_file())
                            .cloned();
                        self.hash_result = None;
                        self.hash_error = None;
                        self.show_hash_dialog = true;
                        *close = true;
                    }
                    if tool_tile(
                        &mut columns[1],
                        "名",
                        "批量文件改名",
                        egui::Color32::from_rgb(82, 143, 208),
                    ) {
                        self.rename_items = self.selected_paths.clone();
                        self.rename_feedback = None;
                        self.rename_confirm = false;
                        self.show_rename_dialog = true;
                        *close = true;
                    }
                    if tool_tile(
                        &mut columns[2],
                        "替",
                        "批量字符替换",
                        egui::Color32::from_rgb(111, 174, 123),
                    ) {
                        self.text_replace_items = self
                            .selected_paths
                            .iter()
                            .filter(|path| path.is_file())
                            .cloned()
                            .collect();
                        self.text_replace_preview.clear();
                        self.text_replace_feedback = None;
                        self.show_text_replace_dialog = true;
                        *close = true;
                    }
                });
                ui.add_space(6.0);
                if ui.link("▤  文件信息").clicked() {
                    self.show_info_dialog = true;
                    *close = true;
                }
                if ui.link("ⓘ  关于软件").clicked() {
                    self.show_about_dialog = true;
                    *close = true;
                }
                if cfg!(windows) && ui.link("⚙  默认应用与右键菜单设置").clicked() {
                    self.show_integration_dialog = true;
                    *close = true;
                }
            },
        );
        self.show_toolbox_dialog = !close;
    }

    fn show_formats_dialog(&mut self, context: &egui::Context) {
        if !self.show_formats_dialog {
            return;
        }
        let close = show_native_popup(
            context,
            "miaozip_formats",
            "支持的格式 - 妙压",
            [530.0, 310.0],
            |ui, close| {
                operation_heading(ui, "格式支持", "以实际可用功能为准");
                ui.add_space(4.0);
                ui.group(|ui| {
                    ui.set_min_width(ui.available_width() - 12.0);
                    ui.label(egui::RichText::new("创建压缩包").strong().color(BLUE));
                    ui.add_space(4.0);
                    ui.label("ZIP · 7Z · TAR · TAR.GZ · TAR.BZ2 · TAR.XZ · TAR.ZST");
                });
                ui.add_space(8.0);
                ui.group(|ui| {
                    ui.set_min_width(ui.available_width() - 12.0);
                    ui.label(egui::RichText::new("浏览与解压").strong().color(BLUE));
                    ui.add_space(4.0);
                    ui.label("ZIP · 7Z · RAR · TAR · GZ · BZ2 · XZ · ZST 及组合格式");
                });
                ui.add_space(8.0);
                ui.weak("RAR 可解压，但不支持创建；7Z 创建使用固定 LZMA2 配置。");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("关闭").clicked() {
                        *close = true;
                    }
                });
            },
        );
        self.show_formats_dialog = !close;
    }

    fn show_add_to_archive_dialog(&mut self, context: &egui::Context) {
        if !self.show_add_to_archive_dialog {
            return;
        }
        let mut start = false;
        let close = show_native_popup(
            context,
            "miaozip_add_to_archive",
            "添加到压缩包 - 妙压",
            [590.0, 410.0],
            |ui, close| {
                operation_heading(ui, "添加到当前 ZIP", "可添加文件、文件夹，或直接拖入主列表");
                ui.add_space(8.0);
                ui.group(|ui| {
                    ui.set_min_width(ui.available_width() - 12.0);
                    ui.label(format!("目标目录：/{}", self.archive_directory));
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .max_height(205.0)
                        .show(ui, |ui| {
                            for (index, source) in self.sources.iter().enumerate() {
                                let name = source
                                    .file_name()
                                    .map(|name| name.to_string_lossy().into_owned())
                                    .unwrap_or_else(|| source.display().to_string());
                                ui.selectable_value(
                                    &mut self.selected_source,
                                    Some(index),
                                    if source.is_dir() {
                                        format!("📁 {name}")
                                    } else {
                                        format!("📄 {name}")
                                    },
                                )
                                .on_hover_text(source.display().to_string());
                            }
                        });
                    ui.horizontal(|ui| {
                        if ui.button("添加文件...").clicked()
                            && let Some(paths) = FileDialog::new().pick_files()
                        {
                            for path in paths {
                                if !self.sources.contains(&path) {
                                    self.sources.push(path);
                                }
                            }
                        }
                        if ui.button("添加文件夹...").clicked()
                            && let Some(path) = FileDialog::new().pick_folder()
                            && !self.sources.contains(&path)
                        {
                            self.sources.push(path);
                        }
                        if ui
                            .add_enabled(self.selected_source.is_some(), egui::Button::new("移除"))
                            .clicked()
                            && let Some(index) = self.selected_source.take()
                            && index < self.sources.len()
                        {
                            self.sources.remove(index);
                        }
                    });
                });
                ui.add_space(8.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("取消").clicked() {
                        *close = true;
                    }
                    if ui
                        .add_enabled(
                            !self.sources.is_empty(),
                            egui::Button::new(
                                egui::RichText::new("添加").color(egui::Color32::WHITE),
                            )
                            .fill(BLUE),
                        )
                        .clicked()
                    {
                        start = true;
                    }
                });
            },
        );
        if start {
            let sources = std::mem::take(&mut self.sources);
            if self.start_zip_update(context, sources, HashSet::new()) {
                self.show_add_to_archive_dialog = false;
            }
        } else {
            self.show_add_to_archive_dialog = !close;
        }
    }

    fn open_password_dialog(&mut self) {
        let Some(path) = &self.opened_archive else {
            self.status = JobStatus::Error("请先打开 ZIP 压缩包".to_owned());
            return;
        };
        if ArchiveFormat::from_path(path) != Some(ArchiveFormat::Zip) {
            self.status = JobStatus::Error("密码管理目前仅支持 ZIP".to_owned());
            return;
        }
        self.password_dialog_mode = PasswordDialogMode::Change;
        self.current_archive_password = self.archive_password.clone();
        self.new_archive_password.clear();
        self.new_archive_password_confirm.clear();
        self.show_password_dialog = true;
    }

    fn start_password_rewrite(&mut self, context: &egui::Context) -> bool {
        let Some(archive_path) = self.opened_archive.clone() else {
            return false;
        };
        let encrypted = self.archive_entries.iter().any(|entry| entry.encrypted);
        if encrypted && self.current_archive_password.is_empty() {
            self.status = JobStatus::Error("请输入当前密码".to_owned());
            return false;
        }
        if self.new_archive_password != self.new_archive_password_confirm {
            self.status = JobStatus::Error("两次输入的新密码不一致".to_owned());
            return false;
        }
        if !encrypted && self.new_archive_password.is_empty() {
            self.status = JobStatus::Error("请输入要设置的新密码".to_owned());
            return false;
        }
        let old_password = (!self.current_archive_password.is_empty())
            .then(|| self.current_archive_password.clone());
        let new_password =
            (!self.new_archive_password.is_empty()).then(|| self.new_archive_password.clone());
        let (sender, receiver) = mpsc::channel();
        let repaint = context.clone();
        self.worker = Some(receiver);
        self.reload_archive_after_job = true;
        self.pending_password_after_job = Some(new_password.clone().unwrap_or_default());
        self.status = JobStatus::Running {
            operation: if new_password.is_some() {
                "正在设置 ZIP 密码"
            } else {
                "正在移除 ZIP 密码"
            },
            completed: 0,
            total: 0,
            current: String::new(),
        };
        self.show_progress_dialog = true;
        thread::spawn(move || {
            let result = archive::rewrite_zip_password(
                &archive_path,
                old_password.as_deref(),
                new_password.as_deref(),
                |progress| {
                    let _ = sender.send(WorkerMessage::Progress(progress));
                    repaint.request_repaint();
                },
            )
            .map_err(|error| format!("{error:#}"));
            let _ = sender.send(WorkerMessage::Finished(result));
            repaint.request_repaint();
        });
        true
    }

    fn show_password_management_dialog(&mut self, context: &egui::Context) {
        if !self.show_password_dialog {
            return;
        }
        let mode = self.password_dialog_mode;
        let encrypted = self.archive_entries.iter().any(|entry| entry.encrypted);
        let mut apply = false;
        let close = show_native_popup(
            context,
            "miaozip_password",
            "密码 - 妙压",
            [
                500.0,
                if mode == PasswordDialogMode::Unlock {
                    250.0
                } else {
                    350.0
                },
            ],
            |ui, close| {
                operation_heading(
                    ui,
                    if mode == PasswordDialogMode::Unlock {
                        "输入 ZIP 密码"
                    } else if encrypted {
                        "修改或移除密码"
                    } else {
                        "设置 ZIP 密码"
                    },
                    "ZIP 文件内容使用 AES-256 加密",
                );
                ui.add_space(8.0);
                if mode == PasswordDialogMode::Unlock || encrypted {
                    ui.label("当前密码：");
                    ui.add_sized(
                        [440.0, 27.0],
                        egui::TextEdit::singleline(&mut self.current_archive_password)
                            .password(true),
                    );
                }
                if mode == PasswordDialogMode::Change {
                    ui.add_space(7.0);
                    ui.label(if encrypted {
                        "新密码（留空表示移除密码）："
                    } else {
                        "新密码："
                    });
                    ui.add_sized(
                        [440.0, 27.0],
                        egui::TextEdit::singleline(&mut self.new_archive_password).password(true),
                    );
                    ui.label("确认新密码：");
                    ui.add_sized(
                        [440.0, 27.0],
                        egui::TextEdit::singleline(&mut self.new_archive_password_confirm)
                            .password(true),
                    );
                }
                ui.add_space(10.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("取消").clicked() {
                        *close = true;
                    }
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(if mode == PasswordDialogMode::Unlock {
                                    "解锁"
                                } else {
                                    "应用"
                                })
                                .color(egui::Color32::WHITE),
                            )
                            .fill(BLUE),
                        )
                        .clicked()
                    {
                        apply = true;
                    }
                });
            },
        );
        if apply && mode == PasswordDialogMode::Unlock {
            let Some(path) = self.opened_archive.clone() else {
                self.show_password_dialog = false;
                return;
            };
            match archive::validate_zip_password(&path, &self.current_archive_password) {
                Ok(()) => {
                    self.archive_password = self.current_archive_password.clone();
                    self.status = JobStatus::Success("ZIP 密码正确，已解锁当前会话".to_owned());
                    self.show_password_dialog = false;
                }
                Err(error) => self.status = JobStatus::Error(format!("无法解锁 ZIP：{error:#}")),
            }
        } else if apply && self.start_password_rewrite(context) {
            self.show_password_dialog = false;
        } else if close {
            self.show_password_dialog = false;
        }
    }

    fn create_self_extracting_archive(&mut self, context: &egui::Context) {
        let Some(zip_path) = self.opened_archive.clone() else {
            self.status = JobStatus::Error("请先打开 ZIP 压缩包".to_owned());
            return;
        };
        if ArchiveFormat::from_path(&zip_path) != Some(ArchiveFormat::Zip) {
            self.status = JobStatus::Error("自解压文件目前只支持 ZIP".to_owned());
            return;
        }
        let (extension, filter_name) = if cfg!(windows) {
            ("exe", "Windows 自解压程序")
        } else {
            ("run", "自解压程序")
        };
        let suggested = format!(
            "{}.{}",
            zip_path.file_stem().unwrap_or_default().to_string_lossy(),
            extension
        );
        let Some(output) = FileDialog::new()
            .add_filter(filter_name, &[extension])
            .set_file_name(suggested)
            .save_file()
        else {
            return;
        };
        let output = if output.extension().is_none() {
            output.with_extension(extension)
        } else {
            output
        };
        let Ok(stub) = std::env::current_exe() else {
            self.status = JobStatus::Error("无法定位妙压程序文件".to_owned());
            return;
        };
        let (sender, receiver) = mpsc::channel();
        let repaint = context.clone();
        self.worker = Some(receiver);
        self.status = JobStatus::Running {
            operation: "正在创建自解压文件",
            completed: 0,
            total: 1,
            current: output.display().to_string(),
        };
        self.show_progress_dialog = true;
        thread::spawn(move || {
            let result = archive::create_self_extracting(&stub, &zip_path, &output)
                .map_err(|error| format!("{error:#}"));
            let _ = sender.send(WorkerMessage::Finished(result));
            repaint.request_repaint();
        });
    }

    fn show_optical_dialog(&mut self, context: &egui::Context) {
        if !self.show_optical_dialog {
            return;
        }
        let close = show_native_popup(
            context,
            "miaozip_optical",
            "虚拟光驱 - 妙压",
            [590.0, 355.0],
            |ui, _close| {
                operation_heading(ui, "虚拟光驱", "以只读方式挂载或卸载 ISO 镜像");
                ui.label("ISO 镜像：");
                ui.horizontal(|ui| {
                    let mut text = self
                        .iso_path
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_default();
                    if ui
                        .add_sized([430.0, 25.0], egui::TextEdit::singleline(&mut text))
                        .changed()
                    {
                        self.iso_path = Some(PathBuf::from(text));
                    }
                    if ui.button("浏览...").clicked()
                        && let Some(path) = FileDialog::new()
                            .add_filter("ISO 光盘镜像", &["iso"])
                            .pick_file()
                    {
                        self.iso_path = Some(path);
                    }
                });
                ui.add_space(8.0);
                ui.group(|ui| {
                    ui.set_min_width(ui.available_width() - 12.0);
                    if let Some(mounted) = &self.mounted_image {
                        ui.label(
                            egui::RichText::new("●  已挂载")
                                .strong()
                                .color(egui::Color32::from_rgb(43, 147, 91)),
                        );
                        ui.label(mounted.image.display().to_string());
                        ui.weak(format!("位置：{}", mounted.location));
                        if let Some(device) = &mounted.device {
                            ui.weak(format!("设备：{device}"));
                        }
                    } else {
                        ui.label(egui::RichText::new("○  未挂载").strong().color(BLUE));
                        ui.weak("当前会话尚未挂载镜像");
                    }
                });
                ui.add_space(8.0);
                let busy = self.optical_worker.is_some();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !busy && self.mounted_image.is_none(),
                            egui::Button::new("挂载 ISO"),
                        )
                        .clicked()
                    {
                        if let Some(path) = self.iso_path.clone() {
                            let (sender, receiver) = mpsc::channel();
                            self.optical_worker = Some(receiver);
                            let repaint = context.clone();
                            self.status = JobStatus::Running {
                                operation: "正在挂载 ISO",
                                completed: 0,
                                total: 0,
                                current: String::new(),
                            };
                            self.show_progress_dialog = true;
                            thread::spawn(move || {
                                let result = optical::mount_iso(&path)
                                    .map(Some)
                                    .map_err(|error| format!("{error:#}"));
                                let _ = sender.send(result);
                                repaint.request_repaint();
                            });
                        } else {
                            self.status = JobStatus::Error("请先选择 ISO 镜像".to_owned());
                        }
                    }
                    if ui
                        .add_enabled(
                            !busy && self.mounted_image.is_some(),
                            egui::Button::new("卸载当前镜像"),
                        )
                        .clicked()
                        && let Some(mounted) = self.mounted_image.clone()
                    {
                        let (sender, receiver) = mpsc::channel();
                        self.optical_worker = Some(receiver);
                        let repaint = context.clone();
                        self.status = JobStatus::Running {
                            operation: "正在卸载 ISO",
                            completed: 0,
                            total: 0,
                            current: String::new(),
                        };
                        self.show_progress_dialog = true;
                        thread::spawn(move || {
                            let result = optical::unmount_iso(&mounted)
                                .map(|_| None)
                                .map_err(|error| format!("{error:#}"));
                            let _ = sender.send(result);
                            repaint.request_repaint();
                        });
                    }
                });
                ui.add_space(6.0);
                ui.weak(if cfg!(target_os = "linux") {
                    "Linux 需要 udisks2，桌面授权提示由系统处理。"
                } else if cfg!(target_os = "macos") {
                    "macOS 使用 hdiutil 挂载。"
                } else {
                    "Windows 使用系统的 Mount-DiskImage 功能。"
                });
                ui.weak("仅管理本次运行中由妙压挂载的镜像；关闭应用不会自动卸载。");
            },
        );
        self.show_optical_dialog = !close;
    }

    fn show_about_dialog(&mut self, context: &egui::Context) {
        if !self.show_about_dialog {
            return;
        }
        let close = show_native_popup(
            context,
            "miaozip_about",
            "关于 妙压",
            [490.0, 270.0],
            |ui, _close| {
                branded_banner(ui, "妙压", "跨平台压缩文件管理器");
                ui.add_space(13.0);
                ui.label("受经典压缩软件界面启发的 Rust 桌面应用");
                ui.add_space(4.0);
                ui.label("支持 Windows、Linux 和 macOS");
                ui.label("核心格式：ZIP、7Z、RAR 解压及 TAR 系列");
                ui.add_space(7.0);
                ui.weak("妙压是独立开发的软件，与 2345 好压没有隶属或授权关系。");
            },
        );
        self.show_about_dialog = !close;
    }
}

impl eframe::App for MiaoZipApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let context = ui.ctx().clone();
        self.poll_worker();
        self.poll_archive_listing();
        self.poll_archive_preview();
        self.poll_optical_worker();
        self.poll_hash_worker();
        self.poll_image_convert_worker();
        self.poll_zip_test_worker();
        self.poll_zip_repair_worker();
        self.handle_dropped_files(&context);

        if self.context_only_add {
            self.show_context_add_root(ui);
            if self.is_running() {
                context.request_repaint_after(Duration::from_millis(100));
            }
            return;
        }

        if self.context_only_extract {
            self.show_context_extract_root(ui);
            if self.is_running() {
                context.request_repaint_after(Duration::from_millis(100));
            }
            return;
        }

        self.show_shell(ui);

        self.show_archive_open_confirmation(&context);

        self.show_default_app_prompt(&context);
        self.show_system_integration_dialog(&context);
        self.show_toolbox_dialog(&context);
        self.show_optical_dialog(&context);
        self.show_create_archive_dialog(&context);
        self.show_add_to_archive_dialog(&context);
        self.show_extract_archive_dialog(&context);
        self.show_delete_confirmation(&context);
        self.show_password_management_dialog(&context);
        self.show_information_dialog(&context);
        self.show_about_dialog(&context);
        self.show_formats_dialog(&context);
        self.show_job_progress_dialog(&context);
        self.show_hash_tool_dialog(&context);
        self.show_rename_tool_dialog(&context);
        self.show_text_replace_tool_dialog(&context);
        self.show_image_convert_tool_dialog(&context);
        self.show_zip_test_tool_dialog(&context);

        if self.is_running()
            || self.archive_listing_worker.is_some()
            || self.archive_preview_worker.is_some()
        {
            context.request_repaint_after(Duration::from_millis(100));
        }
    }
}

fn apply_haozip_visuals(context: &egui::Context) {
    let mut visuals = egui::Visuals::light();
    visuals.panel_fill = egui::Color32::WHITE;
    visuals.window_fill = egui::Color32::WHITE;
    visuals.window_stroke = egui::Stroke::new(1.0, LINE);
    visuals.window_corner_radius = egui::CornerRadius::same(4);
    visuals.faint_bg_color = egui::Color32::from_rgb(239, 247, 253);
    visuals.extreme_bg_color = egui::Color32::WHITE;
    visuals.text_edit_bg_color = Some(egui::Color32::WHITE);
    visuals.weak_text_color = Some(egui::Color32::from_rgb(96, 116, 133));
    visuals.widgets.noninteractive.fg_stroke.color = TEXT;
    visuals.widgets.inactive.weak_bg_fill = egui::Color32::from_rgb(241, 248, 253);
    visuals.widgets.inactive.bg_fill = egui::Color32::WHITE;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, LINE);
    visuals.widgets.hovered.weak_bg_fill = egui::Color32::from_rgb(225, 242, 254);
    visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(225, 242, 254);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, BLUE);
    visuals.widgets.active.weak_bg_fill = egui::Color32::from_rgb(204, 232, 251);
    visuals.widgets.active.bg_fill = egui::Color32::from_rgb(204, 232, 251);
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, BLUE);
    for widget in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
    ] {
        widget.corner_radius = egui::CornerRadius::same(3);
    }
    visuals.selection.bg_fill = BLUE;
    visuals.selection.stroke.color = egui::Color32::WHITE;
    context.set_visuals(visuals);
    context.style_mut_of(egui::Theme::Light, |style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(11.0, 5.0);
    });
}

fn should_show_registration_prompt(
    ask_on_startup: bool,
    candidate_registered: bool,
    default_count: usize,
    supported_count: usize,
) -> bool {
    ask_on_startup && (!candidate_registered || default_count < supported_count)
}

fn operation_dialog_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(egui::Color32::WHITE)
        .stroke(egui::Stroke::new(1.0, LINE))
        .inner_margin(16)
}

fn show_native_popup(
    context: &egui::Context,
    id: &str,
    title: &str,
    size: [f32; 2],
    mut content: impl FnMut(&mut egui::Ui, &mut bool),
) -> bool {
    let mut close = false;
    context.show_viewport_immediate(
        egui::ViewportId::from_hash_of(id),
        egui::ViewportBuilder::default()
            .with_title(title)
            .with_icon(crate::app_icon())
            .with_inner_size(size)
            .with_min_inner_size(size)
            .with_resizable(false),
        |viewport, _class| {
            if viewport.input(|input| input.viewport().close_requested()) {
                close = true;
            }
            egui::CentralPanel::default()
                .frame(operation_dialog_frame())
                .show(viewport, |ui| content(ui, &mut close));
        },
    );
    close
}

fn operation_heading(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 53.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, BLUE);
    painter.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(rect.right() - 160.0, rect.top()),
            rect.right_top(),
            rect.right_bottom(),
            egui::pos2(rect.right() - 220.0, rect.bottom()),
        ],
        egui::Color32::from_white_alpha(18),
        egui::Stroke::NONE,
    ));
    painter.text(
        egui::pos2(rect.left() + 16.0, rect.top() + 8.0),
        egui::Align2::LEFT_TOP,
        title,
        egui::FontId::proportional(18.0),
        egui::Color32::WHITE,
    );
    painter.text(
        egui::pos2(rect.left() + 17.0, rect.top() + 32.0),
        egui::Align2::LEFT_TOP,
        subtitle,
        egui::FontId::proportional(12.0),
        egui::Color32::from_rgb(221, 240, 252),
    );
    ui.add_space(8.0);
}

fn branded_banner(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 100.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, BLUE);
    painter.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(rect.right() - 220.0, rect.top()),
            rect.right_top(),
            rect.right_bottom(),
            egui::pos2(rect.right() - 290.0, rect.bottom()),
        ],
        egui::Color32::from_white_alpha(16),
        egui::Stroke::NONE,
    ));
    painter.text(
        egui::pos2(rect.left() + 25.0, rect.top() + 22.0),
        egui::Align2::LEFT_TOP,
        title,
        egui::FontId::proportional(25.0),
        egui::Color32::WHITE,
    );
    painter.text(
        egui::pos2(rect.left() + 27.0, rect.top() + 64.0),
        egui::Align2::LEFT_TOP,
        subtitle,
        egui::FontId::proportional(13.0),
        egui::Color32::from_rgb(220, 241, 255),
    );
    let icon = egui::Rect::from_min_size(
        egui::pos2(rect.right() - 78.0, rect.top() + 25.0),
        egui::vec2(47.0, 51.0),
    );
    painter.rect_filled(icon, 4.0, egui::Color32::from_rgb(54, 158, 227));
    painter.rect_filled(
        egui::Rect::from_min_size(icon.min, egui::vec2(icon.width(), 14.0)),
        2.0,
        egui::Color32::from_rgb(249, 169, 61),
    );
    painter.rect_filled(
        egui::Rect::from_min_size(
            egui::pos2(icon.left(), icon.top() + 33.0),
            egui::vec2(icon.width(), 14.0),
        ),
        2.0,
        egui::Color32::from_rgb(77, 190, 112),
    );
    painter.rect_filled(
        egui::Rect::from_min_size(
            egui::pos2(icon.center().x - 4.0, icon.top()),
            egui::vec2(8.0, icon.height()),
        ),
        1.0,
        egui::Color32::from_rgb(238, 242, 236),
    );
}

fn tool_tile(ui: &mut egui::Ui, glyph: &str, label: &str, accent: egui::Color32) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 91.0), egui::Sense::click());
    let painter = ui.painter_at(rect);
    painter.rect_filled(
        rect.shrink(3.0),
        4.0,
        if response.hovered() {
            egui::Color32::from_rgb(229, 243, 253)
        } else {
            egui::Color32::TRANSPARENT
        },
    );
    painter.circle_filled(egui::pos2(rect.center().x, rect.top() + 31.0), 23.0, accent);
    painter.text(
        egui::pos2(rect.center().x, rect.top() + 30.0),
        egui::Align2::CENTER_CENTER,
        glyph,
        egui::FontId::proportional(20.0),
        egui::Color32::WHITE,
    );
    painter.text(
        egui::pos2(rect.center().x, rect.top() + 70.0),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(14.0),
        TEXT,
    );
    response.clicked()
}

fn integration_status_row(ui: &mut egui::Ui, label: &str, value: &str, active: bool) {
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(246, 251, 255))
        .stroke(egui::Stroke::new(1.0, LINE))
        .corner_radius(3)
        .inner_margin(egui::Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(label).strong().color(TEXT));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.colored_label(
                        if active {
                            egui::Color32::from_rgb(34, 145, 92)
                        } else {
                            egui::Color32::from_rgb(126, 143, 157)
                        },
                        value,
                    );
                });
            });
        });
}

fn operation_tabs(ui: &mut egui::Ui, selected: &mut u8, tabs: &[&str]) {
    ui.horizontal(|ui| {
        for (index, label) in tabs.iter().enumerate() {
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(72.0, 28.0), egui::Sense::click());
            let active = *selected == index as u8;
            ui.painter().rect_filled(
                rect,
                2.0,
                if active {
                    egui::Color32::WHITE
                } else {
                    egui::Color32::from_rgb(232, 239, 245)
                },
            );
            if active {
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(rect.left(), rect.bottom() - 3.0),
                        egui::vec2(rect.width(), 3.0),
                    ),
                    0.0,
                    BLUE,
                );
            }
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                *label,
                egui::FontId::proportional(14.0),
                TEXT,
            );
            if response.clicked() {
                *selected = index as u8;
            }
        }
    });
    ui.separator();
}

fn compression_label(level: u8) -> &'static str {
    match level {
        0 => "存储",
        1 => "最快",
        3 => "快速",
        6 => "标准",
        8 => "较好",
        9 => "最好",
        _ => "自定义",
    }
}

fn default_browse_directory() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn strip_windows_verbatim_prefix(path: &str) -> String {
    if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else if let Some(local) = path.strip_prefix(r"\\?\") {
        local.to_owned()
    } else if let Some(local) = path.strip_prefix(r"\??\") {
        local.to_owned()
    } else {
        path.to_owned()
    }
}

pub(crate) fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    if cfg!(windows) {
        strip_windows_verbatim_prefix(&text)
    } else {
        text.into_owned()
    }
}

fn drive_locations() -> Vec<(String, PathBuf)> {
    #[cfg(windows)]
    {
        let mut drives = Vec::new();
        for letter in b'A'..=b'Z' {
            let path = PathBuf::from(format!("{}:\\", char::from(letter)));
            if path.exists() {
                drives.push((format!("本地磁盘 ({})", char::from(letter)), path));
            }
        }
        drives
    }
    #[cfg(not(windows))]
    {
        let mut drives = vec![("文件系统".to_owned(), PathBuf::from("/"))];
        if let Ok(items) = fs::read_dir("/Volumes") {
            for item in items.flatten() {
                let path = item.path();
                if path.is_dir() {
                    drives.push((item.file_name().to_string_lossy().into_owned(), path));
                }
            }
        }
        drives
    }
}

fn file_kind(path: &Path, is_directory: bool) -> String {
    if is_directory {
        return "文件夹".to_owned();
    }
    if let Some(format) = ArchiveFormat::from_path(path) {
        return format!("{} 压缩文件", format.label());
    }
    match path
        .extension()
        .map(|extension| extension.to_string_lossy())
    {
        Some(extension) if extension.eq_ignore_ascii_case("zip") => "ZIP 压缩文件".to_owned(),
        Some(extension) if !extension.is_empty() => format!("{} 文件", extension.to_uppercase()),
        _ => "文件".to_owned(),
    }
}

fn archive_children(items: &[archive::ArchiveListEntry], directory: &str) -> Vec<FileEntry> {
    let prefix = if directory.is_empty() {
        String::new()
    } else {
        format!("{directory}/")
    };
    let mut children = BTreeMap::<String, FileEntry>::new();
    for item in items {
        let Some(rest) = item.path.strip_prefix(&prefix) else {
            continue;
        };
        if rest.is_empty() {
            continue;
        }
        let (name, nested) = match rest.split_once('/') {
            Some((name, _)) => (name, true),
            None => (rest, false),
        };
        let is_directory = nested || item.is_directory;
        let path = PathBuf::from(format!("{prefix}{name}"));
        children
            .entry(name.to_owned())
            .and_modify(|existing| {
                if is_directory {
                    existing.is_directory = true;
                    existing.kind = "文件夹".to_owned();
                    existing.size = None;
                }
            })
            .or_insert_with(|| FileEntry {
                path: path.clone(),
                name: name.to_owned(),
                kind: file_kind(&path, is_directory),
                size: if is_directory { None } else { item.size },
                modified: "-".to_owned(),
                is_directory,
            });
    }
    let mut entries: Vec<_> = children.into_values().collect();
    entries.sort_by(|left, right| {
        right
            .is_directory
            .cmp(&left.is_directory)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    entries
}

fn format_system_time(time: SystemTime) -> String {
    let seconds = match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs() as i64,
        Err(_) => return "-".to_owned(),
    };
    let days = seconds.div_euclid(86_400);
    let seconds_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_date_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = seconds_of_day % 3_600 / 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}")
}

fn civil_date_from_days(days_since_epoch: i64) -> (i64, i64, i64) {
    let shifted = days_since_epoch + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
}

fn suggested_archive_path(
    sources: &[PathBuf],
    fallback_directory: &Path,
    format: ArchiveFormat,
) -> PathBuf {
    let name = if sources.len() == 1 {
        sources[0]
            .file_stem()
            .map(|stem| format!("{}{}", stem.to_string_lossy(), format.extension()))
            .unwrap_or_else(|| format!("新建压缩文件{}", format.extension()))
    } else {
        format!("新建压缩文件{}", format.extension())
    };
    let directory = sources
        .first()
        .and_then(|path| path.parent())
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(fallback_directory);
    directory.join(name)
}

fn archive_base_name(path: &Path) -> String {
    let Some(name) = path.file_name().map(|name| name.to_string_lossy()) else {
        return "解压文件".to_owned();
    };
    let base = match ArchiveFormat::from_path(path) {
        Some(format) if name.to_ascii_lowercase().ends_with(format.extension()) => {
            &name[..name.len() - format.extension().len()]
        }
        _ => name
            .rsplit_once('.')
            .map_or(name.as_ref(), |(stem, _)| stem),
    };
    if base.is_empty() {
        "解压文件".to_owned()
    } else {
        base.to_owned()
    }
}

fn ensure_archive_extension(mut path: PathBuf, format: ArchiveFormat) -> PathBuf {
    if path.extension().is_none() {
        path = PathBuf::from(format!("{}{}", path.display(), format.extension()));
    }
    path
}

fn replace_archive_extension(path: &Path, old: ArchiveFormat, new: ArchiveFormat) -> PathBuf {
    let name = path.to_string_lossy();
    if name.to_ascii_lowercase().ends_with(old.extension()) {
        PathBuf::from(format!(
            "{}{}",
            &name[..name.len() - old.extension().len()],
            new.extension()
        ))
    } else {
        path.to_path_buf()
    }
}

fn is_archive_file(path: &Path) -> bool {
    ArchiveFormat::from_path(path).is_some()
}

fn risky_external_file(path: &Path) -> bool {
    let Some(extension) = path.extension() else {
        return false;
    };
    let extension = extension.to_string_lossy();
    [
        "exe",
        "com",
        "bat",
        "cmd",
        "msi",
        "msp",
        "msix",
        "appx",
        "scr",
        "pif",
        "ps1",
        "psm1",
        "vbs",
        "vbe",
        "js",
        "jse",
        "wsf",
        "wsh",
        "hta",
        "lnk",
        "url",
        "jar",
        "sh",
        "command",
        "desktop",
        "app",
        "applescript",
        "docm",
        "xlsm",
        "pptm",
        "reg",
        "inf",
        "cpl",
        "scf",
    ]
    .iter()
    .any(|candidate| extension.eq_ignore_ascii_case(candidate))
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn install_cjk_font(context: &egui::Context) {
    let candidates = [
        "C:/Windows/Fonts/msyh.ttc",
        "C:/Windows/Fonts/simhei.ttf",
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
    ];
    let Some(font_data) = candidates.iter().find_map(|path| fs::read(path).ok()) else {
        return;
    };

    let mut fonts = egui::FontDefinitions::default();
    let font_name = "system-cjk".to_owned();
    fonts.font_data.insert(
        font_name.clone(),
        Arc::new(egui::FontData::from_owned(font_data)),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        if let Some(fonts_in_family) = fonts.families.get_mut(&family) {
            fonts_in_family.push(font_name.clone());
        }
    }
    context.set_fonts(fonts);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_candidate_does_not_trigger_startup_prompt() {
        assert!(should_show_registration_prompt(true, false, 0, 13));
        assert!(should_show_registration_prompt(true, true, 12, 13));
        assert!(!should_show_registration_prompt(true, true, 13, 13));
        assert!(!should_show_registration_prompt(false, false, 0, 13));
    }

    #[test]
    fn adds_zip_extension_only_when_missing() {
        assert_eq!(
            ensure_archive_extension(PathBuf::from("archive"), ArchiveFormat::Zip),
            PathBuf::from("archive.zip")
        );
        assert_eq!(
            ensure_archive_extension(PathBuf::from("archive.custom"), ArchiveFormat::Zip),
            PathBuf::from("archive.custom")
        );
    }

    #[test]
    fn formats_byte_sizes() {
        assert_eq!(format_bytes(999), "999 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1_572_864), "1.5 MB");
    }

    #[test]
    fn hides_windows_verbatim_prefix_in_address_bar() {
        assert_eq!(
            strip_windows_verbatim_prefix(r"\\?\D:\Work\archive.zip"),
            r"D:\Work\archive.zip"
        );
        assert_eq!(
            strip_windows_verbatim_prefix(r"\\?\UNC\server\share\folder"),
            r"\\server\share\folder"
        );
        assert_eq!(strip_windows_verbatim_prefix(r"D:\Work"), r"D:\Work");
    }

    #[test]
    fn warns_before_running_active_content_from_archive() {
        assert!(risky_external_file(Path::new("inside/installer.EXE")));
        assert!(risky_external_file(Path::new("inside/link.lnk")));
        assert!(!risky_external_file(Path::new("inside/report.pdf")));
    }

    #[test]
    fn folder_tree_lists_only_real_child_directories() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("B-dir")).unwrap();
        fs::create_dir(root.path().join("a-dir")).unwrap();
        fs::write(root.path().join("not-a-folder.txt"), b"file").unwrap();
        let children = shell::child_directories(root.path());
        let names: Vec<_> = children
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["a-dir", "B-dir"]);
    }

    #[test]
    fn puts_shell_archive_next_to_source() {
        let source = PathBuf::from("C:/work/example.txt");
        assert_eq!(
            suggested_archive_path(&[source], Path::new("C:/elsewhere"), ArchiveFormat::Zip),
            PathBuf::from("C:/work/example.zip")
        );
    }

    #[test]
    fn names_extract_folder_for_tar_aliases() {
        assert_eq!(archive_base_name(Path::new("backup.tar.gz")), "backup");
        assert_eq!(archive_base_name(Path::new("backup.tgz")), "backup");
        assert_eq!(archive_base_name(Path::new("sample.rar")), "sample");
    }

    #[test]
    fn archive_browser_shows_only_direct_children() {
        let items = vec![
            archive::ArchiveListEntry {
                path: "folder/a.txt".to_owned(),
                is_directory: false,
                size: Some(3),
                encrypted: false,
            },
            archive::ArchiveListEntry {
                path: "folder/nested/b.txt".to_owned(),
                is_directory: false,
                size: Some(5),
                encrypted: false,
            },
            archive::ArchiveListEntry {
                path: "root.txt".to_owned(),
                is_directory: false,
                size: Some(7),
                encrypted: false,
            },
        ];
        let root = archive_children(&items, "");
        assert_eq!(root.len(), 2);
        assert!(
            root.iter()
                .any(|item| item.name == "folder" && item.is_directory)
        );
        assert!(
            root.iter()
                .any(|item| item.name == "root.txt" && item.size == Some(7))
        );
        let folder = archive_children(&items, "folder");
        assert_eq!(folder.len(), 2);
        assert!(folder.iter().any(|item| item.name == "a.txt"));
        assert!(
            folder
                .iter()
                .any(|item| item.name == "nested" && item.is_directory)
        );
    }

    #[test]
    fn converts_epoch_date() {
        assert_eq!(civil_date_from_days(0), (1970, 1, 1));
        assert_eq!(civil_date_from_days(20_000), (2024, 10, 4));
    }
}
