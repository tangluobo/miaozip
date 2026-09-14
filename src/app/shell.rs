use super::*;

const HEADER_DARK: egui::Color32 = egui::Color32::from_rgb(7, 113, 193);
const HEADER_LIGHT: egui::Color32 = egui::Color32::from_rgb(12, 132, 207);
const HEADER_TEXT: egui::Color32 = egui::Color32::from_rgb(224, 242, 255);
const GRID_HEADER: egui::Color32 = egui::Color32::from_rgb(238, 247, 255);
const SELECTED_ROW: egui::Color32 = egui::Color32::from_rgb(217, 221, 225);

#[derive(Clone)]
struct TreeRow {
    name: String,
    path: Option<PathBuf>,
    icon: u8,
    depth: usize,
    expanded: bool,
    expandable: bool,
}

impl MiaoZipApp {
    pub(super) fn show_shell(&mut self, ui: &mut egui::Ui) {
        self.poll_tree_icons(ui.ctx());
        self.show_title_strip(ui);
        self.show_blue_toolbar(ui);
        self.show_explorer_bar(ui);
        self.show_classic_status(ui);
        self.show_classic_sidebar(ui);
        self.show_classic_list(ui);
    }

    fn show_title_strip(&mut self, root: &mut egui::Ui) {
        egui::Panel::top("haozip_title")
            .exact_size(32.0)
            .frame(egui::Frame::NONE)
            .show_separator_line(false)
            .show(root, |ui| {
                let rect = ui.max_rect();
                let whole_header =
                    egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), 147.0));
                paint_blue_band(&ui.painter().with_clip_rect(rect), whole_header);
                paint_archive_stack(
                    ui.painter(),
                    egui::pos2(rect.left() + 12.0, rect.top() + 13.0),
                    22.0,
                );
                label(
                    ui.painter(),
                    egui::pos2(rect.left() + 40.0, rect.top() + 21.0),
                    &self
                        .opened_archive
                        .as_ref()
                        .and_then(|path| path.file_name())
                        .map(|name| format!("{} - 妙压", name.to_string_lossy()))
                        .unwrap_or_else(|| "此电脑 - 妙压".to_owned()),
                    17.0,
                    HEADER_TEXT,
                    egui::Align2::LEFT_BOTTOM,
                );

                let right = rect.right();
                let controls = [
                    (right - 26.0, 0),
                    (right - 70.0, 1),
                    (right - 112.0, 2),
                    (right - 154.0, 3),
                    (right - 196.0, 4),
                ];
                for (x, action) in controls {
                    let button = egui::Rect::from_min_size(
                        egui::pos2(x - 18.0, rect.top()),
                        egui::vec2(38.0, 32.0),
                    );
                    let response = ui.interact(
                        button,
                        egui::Id::new(("window_control", action)),
                        egui::Sense::click(),
                    );
                    if response.hovered() {
                        ui.painter()
                            .rect_filled(button, 0.0, egui::Color32::from_white_alpha(40));
                    }
                    paint_window_control(ui.painter(), button.center(), action);
                    if response.clicked() {
                        match action {
                            0 => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
                            1 => {
                                let maximized =
                                    ui.input(|i| i.viewport().maximized.unwrap_or(false));
                                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(
                                    !maximized,
                                ));
                            }
                            2 => ui
                                .ctx()
                                .send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
                            3 => self.show_integration_dialog = true,
                            _ => self.show_about_dialog = true,
                        }
                    }
                }
                let drag =
                    egui::Rect::from_min_max(rect.min, egui::pos2(right - 220.0, rect.bottom()));
                if ui
                    .interact(drag, egui::Id::new("title_drag"), egui::Sense::drag())
                    .drag_started()
                {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
            });
    }

    fn show_blue_toolbar(&mut self, root: &mut egui::Ui) {
        egui::Panel::top("haozip_toolbar")
            .exact_size(115.0)
            .frame(egui::Frame::NONE)
            .show_separator_line(false)
            .show(root, |ui| {
                let rect = ui.max_rect();
                let whole_header = egui::Rect::from_min_size(
                    egui::pos2(rect.left(), rect.top() - 32.0),
                    egui::vec2(rect.width(), 147.0),
                );
                paint_blue_band(&ui.painter().with_clip_rect(rect), whole_header);
                let names = ["添加", "解压到", "删除", "密码", "自解压", "工具箱"];
                let enabled = [
                    true,
                    true,
                    !self.selected_paths.is_empty(),
                    true,
                    true,
                    true,
                ];
                for index in 0..6 {
                    let x = rect.left() + 24.0 + index as f32 * 105.0;
                    let hit = egui::Rect::from_min_size(
                        egui::pos2(x, rect.top() + 7.0),
                        egui::vec2(101.0, 104.0),
                    );
                    let response =
                        ui.interact(hit, egui::Id::new(("toolbar", index)), egui::Sense::click());
                    if response.hovered() && enabled[index] && !self.is_running() {
                        ui.painter()
                            .rect_filled(hit, 4.0, egui::Color32::from_white_alpha(28));
                    }
                    paint_large_icon(ui.painter(), egui::pos2(x + 51.0, rect.top() + 43.0), index);
                    let color = if enabled[index] && !self.is_running() {
                        HEADER_TEXT
                    } else {
                        egui::Color32::from_rgb(128, 188, 226)
                    };
                    label(
                        ui.painter(),
                        egui::pos2(x + 51.0, rect.top() + 96.0),
                        names[index],
                        18.0,
                        color,
                        egui::Align2::CENTER_BOTTOM,
                    );
                    if response.clicked() && enabled[index] && !self.is_running() {
                        match index {
                            0 => self.prepare_add_dialog(),
                            1 => self.prepare_extract_dialog(),
                            2 => self.show_delete_dialog = true,
                            3 => {
                                self.status =
                                    JobStatus::Error("ZIP 密码管理功能尚未实现".to_owned())
                            }
                            4 => {
                                self.status =
                                    JobStatus::Error("跨平台自解压模块尚未实现".to_owned())
                            }
                            _ => self.show_toolbox_dialog = true,
                        }
                    }
                }
                if rect.width() > 980.0 {
                    let brand_x = rect.right() - 326.0;
                    label(
                        ui.painter(),
                        egui::pos2(brand_x + 202.0, rect.top() + 72.0),
                        "妙压",
                        41.0,
                        egui::Color32::WHITE,
                        egui::Align2::RIGHT_BOTTOM,
                    );
                    label(
                        ui.painter(),
                        egui::pos2(brand_x + 202.0, rect.top() + 99.0),
                        "压得巧，解得快",
                        19.0,
                        HEADER_TEXT,
                        egui::Align2::RIGHT_BOTTOM,
                    );
                    paint_archive_stack(
                        ui.painter(),
                        egui::pos2(rect.right() - 81.0, rect.top() + 64.0),
                        67.0,
                    );
                }
            });
    }

    fn show_explorer_bar(&mut self, root: &mut egui::Ui) {
        egui::Panel::top("haozip_address")
            .exact_size(40.0)
            .frame(egui::Frame::NONE)
            .show_separator_line(false)
            .show(root, |ui| {
                let rect = ui.max_rect();
                ui.painter()
                    .rect_filled(rect, 0.0, egui::Color32::from_rgb(248, 251, 255));
                ui.painter().line_segment(
                    [
                        egui::pos2(rect.left(), rect.bottom() - 1.0),
                        egui::pos2(rect.right(), rect.bottom() - 1.0),
                    ],
                    egui::Stroke::new(1.0, LINE),
                );
                let left = rect.left();
                for (index, x) in [35.0, 72.0, 106.0, 141.0, 184.0, 216.0]
                    .into_iter()
                    .enumerate()
                {
                    let area = egui::Rect::from_center_size(
                        egui::pos2(left + x, rect.center().y),
                        egui::vec2(29.0, 31.0),
                    );
                    let response = ui.interact(
                        area,
                        egui::Id::new(("navigation", index)),
                        egui::Sense::click(),
                    );
                    let active = match index {
                        0 => self.opened_archive.is_some() || self.history_index > 0,
                        1 => self.history_index + 1 < self.history.len(),
                        3 => self.opened_archive.is_some() || !self.computer_view,
                        _ => true,
                    };
                    paint_navigation_icon(ui.painter(), area.center(), index, active || index == 0);
                    if response.clicked() && active {
                        match index {
                            0 if self.opened_archive.is_some() => self.close_archive(),
                            0 => self.navigate_history(self.history_index - 1),
                            1 => self.navigate_history(self.history_index + 1),
                            2 => self.navigate_computer(),
                            3 => {
                                if self.opened_archive.is_some() {
                                    self.archive_up();
                                } else if self.computer_view {
                                    self.refresh_entries();
                                } else if let Some(parent) = self.current_directory.parent() {
                                    self.navigate_to(parent.to_path_buf());
                                }
                            }
                            4 => self.navigate_computer(),
                            _ if self.opened_archive.is_some() => self.load_archive_listing(),
                            _ => self.refresh_entries(),
                        }
                    }
                }
                let search_w = (rect.width() * 0.235).clamp(185.0, 280.0);
                let address_right = rect.right() - search_w - 57.0;
                let address = egui::Rect::from_min_max(
                    egui::pos2(left + 235.0, rect.top() + 4.0),
                    egui::pos2(address_right, rect.bottom() - 4.0),
                );
                framed_field(ui.painter(), address);
                paint_monitor(
                    ui.painter(),
                    egui::pos2(address.left() + 17.0, address.center().y),
                    19.0,
                );
                let edit = egui::Rect::from_min_max(
                    egui::pos2(address.left() + 36.0, address.top() + 3.0),
                    egui::pos2(address.right() - 20.0, address.bottom() - 2.0),
                );
                let response = ui.put(
                    edit,
                    egui::TextEdit::singleline(&mut self.address_text)
                        .frame(egui::Frame::NONE)
                        .font(egui::FontId::proportional(17.0)),
                );
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if self.address_text.trim() == "此电脑" {
                        self.navigate_computer();
                    } else {
                        self.navigate_to(PathBuf::from(self.address_text.trim()));
                    }
                }
                triangle(
                    ui.painter(),
                    egui::pos2(address.right() - 15.0, address.center().y + 1.0),
                    5.0,
                    BLUE,
                );
                let search = egui::Rect::from_min_max(
                    egui::pos2(address.right() + 6.0, address.top()),
                    egui::pos2(rect.right() - 55.0, address.bottom()),
                );
                framed_field(ui.painter(), search);
                let search_edit = egui::Rect::from_min_max(
                    egui::pos2(search.left() + 6.0, search.top() + 3.0),
                    egui::pos2(search.right() - 27.0, search.bottom() - 2.0),
                );
                ui.put(
                    search_edit,
                    egui::TextEdit::singleline(&mut self.search_text)
                        .frame(egui::Frame::NONE)
                        .hint_text("当前目录查找(支持包内查找)")
                        .font(egui::FontId::proportional(15.0)),
                );
                paint_search(
                    ui.painter(),
                    egui::pos2(search.right() - 16.0, search.center().y),
                    BLUE,
                );
                label(
                    ui.painter(),
                    egui::pos2(rect.right() - 27.0, rect.center().y),
                    "高级",
                    16.0,
                    egui::Color32::from_rgb(107, 135, 163),
                    egui::Align2::CENTER_CENTER,
                );
            });
    }

    fn show_classic_status(&self, root: &mut egui::Ui) {
        egui::Panel::bottom("haozip_status")
            .exact_size(37.0)
            .frame(egui::Frame::NONE)
            .show_separator_line(false)
            .show(root, |ui| {
                let rect = ui.max_rect();
                ui.painter()
                    .rect_filled(rect, 0.0, egui::Color32::from_rgb(239, 248, 255));
                ui.painter().line_segment(
                    [rect.left_top(), rect.right_top()],
                    egui::Stroke::new(1.0, LINE),
                );
                let status = match &self.status {
                    JobStatus::Idle if self.computer_view => format!(
                        "选中 {} 个磁盘分区",
                        usize::from(self.selected_drive.is_some())
                    ),
                    JobStatus::Idle if self.archive_listing_worker.is_some() => {
                        "正在读取压缩包目录…".to_owned()
                    }
                    JobStatus::Idle if self.archive_preview_worker.is_some() => {
                        "正在准备包内文件…".to_owned()
                    }
                    JobStatus::Idle => format!(
                        "{} 个对象    已选择 {} 个",
                        self.entries.len(),
                        if self.opened_archive.is_some() {
                            usize::from(self.selected_archive_item.is_some())
                        } else {
                            self.selected_paths.len()
                        }
                    ),
                    JobStatus::Running {
                        operation,
                        completed,
                        total,
                        current,
                    } => format!("{operation} {completed}/{total}  {current}"),
                    JobStatus::Success(message) | JobStatus::Error(message) => message.clone(),
                };
                label(
                    ui.painter(),
                    egui::pos2(rect.left() + 7.0, rect.center().y),
                    &status,
                    16.0,
                    TEXT,
                    egui::Align2::LEFT_CENTER,
                );
            });
    }

    fn show_classic_sidebar(&mut self, root: &mut egui::Ui) {
        egui::Panel::left("haozip_tree")
            .exact_size(280.0)
            .frame(egui::Frame::NONE)
            .show_separator_line(false)
            .show(root, |ui| {
                let rect = ui.max_rect();
                ui.painter().rect_filled(rect, 0.0, egui::Color32::WHITE);
                ui.painter().line_segment(
                    [rect.right_top(), rect.right_bottom()],
                    egui::Stroke::new(1.0, LINE),
                );
                let header =
                    egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() - 1.0, 35.0));
                panel_heading(ui.painter(), header, "文件夹", false);
                label(
                    ui.painter(),
                    egui::pos2(header.right() - 18.0, header.center().y),
                    "×",
                    23.0,
                    egui::Color32::from_rgb(92, 140, 190),
                    egui::Align2::CENTER_CENTER,
                );
                let tree_height = (rect.height() * 0.49).clamp(200.0, 265.0);
                let tree_end = rect.top() + tree_height;
                let drives = drive_entries();
                let rows = self.folder_tree_rows(&drives);
                let row_h = 27.0;
                let visible = ((tree_end - header.bottom()) / row_h).floor().max(1.0) as usize;
                let max_scroll = rows.len().saturating_sub(visible);
                self.tree_scroll = self.tree_scroll.min(max_scroll);
                if self.reveal_tree_selection {
                    if let Some(index) = rows.iter().position(|row| {
                        (row.icon == 3 && self.computer_view)
                            || (!self.computer_view
                                && row.path.as_ref().is_some_and(|path| {
                                    paths_match_for_tree(path, &self.current_directory)
                                }))
                    }) {
                        if index < self.tree_scroll {
                            self.tree_scroll = index;
                        } else if index >= self.tree_scroll + visible {
                            self.tree_scroll = index + 1 - visible;
                        }
                        self.reveal_tree_selection = false;
                    }
                }
                let tree_rect = egui::Rect::from_min_max(
                    header.left_bottom(),
                    egui::pos2(rect.right() - 17.0, tree_end),
                );
                if ui.rect_contains_pointer(tree_rect) {
                    let delta = ui.input(|i| i.smooth_scroll_delta.y);
                    if delta < -1.0 {
                        self.tree_scroll = (self.tree_scroll + 1).min(max_scroll);
                    }
                    if delta > 1.0 {
                        self.tree_scroll = self.tree_scroll.saturating_sub(1);
                    }
                }
                for (visible_index, tree_row) in rows
                    .into_iter()
                    .skip(self.tree_scroll)
                    .take(visible)
                    .enumerate()
                {
                    let y = header.bottom() + visible_index as f32 * row_h;
                    let row = egui::Rect::from_min_size(
                        egui::pos2(rect.left(), y),
                        egui::vec2(rect.width() - 18.0, row_h),
                    );
                    let selected = (tree_row.icon == 3 && self.computer_view)
                        || (!self.computer_view
                            && tree_row.path.as_ref().is_some_and(|path| {
                                paths_match_for_tree(path, &self.current_directory)
                            }));
                    if selected {
                        ui.painter().rect_filled(
                            row.shrink2(egui::vec2(2.0, 1.0)),
                            0.0,
                            egui::Color32::from_rgb(188, 221, 247),
                        );
                    }
                    let indent = tree_row.depth.min(10) as f32 * 16.0;
                    let arrow_x = row.left() + 13.0 + indent;
                    if tree_row.expandable {
                        paint_tree_arrow(
                            ui.painter(),
                            egui::pos2(arrow_x, row.center().y),
                            tree_row.expanded,
                        );
                    }
                    let icon_x = row.left() + 31.0 + indent;
                    if let Some(path) = tree_row.path.as_ref() {
                        self.paint_system_tree_icon(
                            ui,
                            egui::pos2(icon_x, row.center().y),
                            path,
                            tree_row.icon,
                        );
                    } else {
                        paint_tree_icon(
                            ui.painter(),
                            egui::pos2(icon_x, row.center().y),
                            tree_row.icon,
                        );
                    }
                    let clipped = ui.painter().with_clip_rect(egui::Rect::from_min_max(
                        egui::pos2(icon_x + 13.0, row.top()),
                        row.right_bottom(),
                    ));
                    label(
                        &clipped,
                        egui::pos2(icon_x + 16.0, row.center().y),
                        &tree_row.name,
                        16.0,
                        TEXT,
                        egui::Align2::LEFT_CENTER,
                    );
                    let response = ui.interact(
                        row,
                        egui::Id::new(("tree_row", tree_row.icon, tree_row.path.clone())),
                        egui::Sense::click(),
                    );
                    if response.clicked() {
                        let arrow_clicked = tree_row.expandable
                            && response
                                .interact_pointer_pos()
                                .is_some_and(|position| position.x < icon_x - 7.0);
                        if arrow_clicked || (response.double_clicked() && tree_row.expandable) {
                            if tree_row.icon == 3 {
                                self.computer_tree_expanded = !self.computer_tree_expanded;
                            } else if let Some(path) = tree_row.path {
                                if !self.expanded_tree_paths.insert(path.clone()) {
                                    self.expanded_tree_paths.remove(&path);
                                }
                            }
                            ui.ctx().request_repaint();
                        } else if tree_row.icon == 3 {
                            self.navigate_computer();
                        } else if let Some(path) = tree_row.path
                            && path.is_dir()
                        {
                            self.navigate_to(path);
                        }
                    }
                }
                let scroll_track = egui::Rect::from_min_max(
                    egui::pos2(rect.right() - 17.0, header.bottom()),
                    egui::pos2(rect.right() - 1.0, tree_end),
                );
                ui.painter()
                    .rect_filled(scroll_track, 0.0, egui::Color32::from_rgb(250, 251, 252));
                let up = egui::Rect::from_min_size(scroll_track.min, egui::vec2(16.0, 19.0));
                let down = egui::Rect::from_min_size(
                    egui::pos2(scroll_track.left(), scroll_track.bottom() - 19.0),
                    egui::vec2(16.0, 19.0),
                );
                paint_scroll_arrow(ui.painter(), up.center(), false);
                paint_scroll_arrow(ui.painter(), down.center(), true);
                if ui
                    .interact(up, egui::Id::new("tree_scroll_up"), egui::Sense::click())
                    .clicked()
                {
                    self.tree_scroll = self.tree_scroll.saturating_sub(1);
                }
                if ui
                    .interact(
                        down,
                        egui::Id::new("tree_scroll_down"),
                        egui::Sense::click(),
                    )
                    .clicked()
                {
                    self.tree_scroll = (self.tree_scroll + 1).min(max_scroll);
                }
                if max_scroll > 0 {
                    let thumb_h = ((scroll_track.height() - 35.0) * visible as f32
                        / (visible + max_scroll) as f32)
                        .max(35.0);
                    let y = scroll_track.top()
                        + 18.0
                        + (scroll_track.height() - 36.0 - thumb_h) * self.tree_scroll as f32
                            / max_scroll as f32;
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(
                            egui::pos2(scroll_track.left() + 2.0, y),
                            egui::vec2(12.0, thumb_h),
                        ),
                        2.0,
                        egui::Color32::from_rgb(206, 207, 210),
                    );
                }
                let detail_head = egui::Rect::from_min_max(
                    egui::pos2(rect.left(), tree_end + 6.0),
                    egui::pos2(rect.right() - 1.0, tree_end + 42.0),
                );
                panel_heading(ui.painter(), detail_head, "详细信息", true);
                triangle(
                    ui.painter(),
                    egui::pos2(detail_head.right() - 18.0, detail_head.center().y + 2.0),
                    5.0,
                    BLUE,
                );
                let detail_clip = ui.painter().with_clip_rect(egui::Rect::from_min_max(
                    detail_head.left_bottom(),
                    rect.right_bottom(),
                ));
                for (index, line) in self.sidebar_detail_lines(&drives).iter().enumerate() {
                    label(
                        &detail_clip,
                        egui::pos2(
                            rect.left() + 7.0,
                            detail_head.bottom() + 17.0 + index as f32 * 25.0,
                        ),
                        line,
                        16.0,
                        TEXT,
                        egui::Align2::LEFT_TOP,
                    );
                }
            });
    }

    fn folder_tree_rows(&mut self, drives: &[DriveEntry]) -> Vec<TreeRow> {
        let mut rows = Vec::new();
        let home = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from);
        if let Some(home) = &home {
            let desktop = home.join("Desktop");
            let desktop = if desktop.is_dir() {
                desktop
            } else {
                std::env::var_os("OneDrive")
                    .map(PathBuf::from)
                    .map(|path| path.join("Desktop"))
                    .filter(|path| path.is_dir())
                    .unwrap_or(desktop)
            };
            if desktop.is_dir() {
                self.push_tree_path(&mut rows, "桌面".to_owned(), desktop, 0, 0);
            }
        }
        if let Some(one_drive) = std::env::var_os("OneDrive")
            .map(PathBuf::from)
            .filter(|path| path.is_dir())
        {
            let name = one_drive
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            self.push_tree_path(&mut rows, name, one_drive, 1, 0);
        }
        if let Some(home) = home.filter(|path| path.is_dir()) {
            let name = home
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            self.push_tree_path(&mut rows, name, home, 2, 0);
        }
        rows.push(TreeRow {
            name: "此电脑".to_owned(),
            path: None,
            icon: 3,
            depth: 0,
            expanded: self.computer_tree_expanded,
            expandable: true,
        });
        if self.computer_tree_expanded {
            for drive in drives {
                self.push_tree_path(&mut rows, drive.name.clone(), drive.path.clone(), 4, 1);
            }
        }
        rows
    }

    fn push_tree_path(
        &mut self,
        rows: &mut Vec<TreeRow>,
        name: String,
        path: PathBuf,
        icon: u8,
        depth: usize,
    ) {
        if rows.len() >= 4096 || depth > 24 {
            return;
        }
        let expanded = self.expanded_tree_paths.contains(&path);
        let expandable = self
            .tree_children
            .get(&path)
            .is_none_or(|children| !children.is_empty());
        rows.push(TreeRow {
            name,
            path: Some(path.clone()),
            icon,
            depth,
            expanded,
            expandable,
        });
        if !expanded {
            return;
        }
        let children = self
            .tree_children
            .entry(path.clone())
            .or_insert_with(|| child_directories(&path))
            .clone();
        for child in children {
            let name = child
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            self.push_tree_path(rows, name, child, 5, depth + 1);
        }
    }

    pub(super) fn reveal_tree_path(&mut self, path: &Path) {
        let readable = PathBuf::from(display_path(path));
        for ancestor in readable.ancestors() {
            if ancestor.is_dir() {
                self.expanded_tree_paths.insert(ancestor.to_path_buf());
            }
        }
        self.computer_tree_expanded = true;
        self.reveal_tree_selection = true;
    }

    fn paint_system_tree_icon(
        &mut self,
        ui: &egui::Ui,
        center: egui::Pos2,
        path: &Path,
        fallback: u8,
    ) {
        self.paint_shell_icon(ui, center, ShellIconKey::Real(path.to_path_buf()), fallback);
    }

    fn paint_shell_icon(
        &mut self,
        ui: &egui::Ui,
        center: egui::Pos2,
        key: ShellIconKey,
        fallback: u8,
    ) {
        #[cfg(windows)]
        {
            if let Some(texture) = self.tree_icon_cache.get(&key) {
                let rect = egui::Rect::from_center_size(center, egui::vec2(20.0, 20.0));
                ui.painter().image(
                    texture.id(),
                    rect,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
                return;
            }
            if self.tree_icon_requested.insert(key.clone())
                && let Some(sender) = &self.tree_icon_requests
            {
                let _ = sender.send(key);
            }
        }
        #[cfg(not(windows))]
        let _ = key;
        if fallback == 6 {
            paint_file_icon(ui.painter(), center, false);
        } else {
            paint_tree_icon(ui.painter(), center, fallback);
        }
    }

    pub(super) fn start_tree_icon_loader(&mut self, context: &egui::Context) {
        #[cfg(windows)]
        {
            let (request_sender, request_receiver) = mpsc::channel::<ShellIconKey>();
            let (result_sender, result_receiver) = mpsc::channel();
            let context = context.clone();
            thread::spawn(move || {
                for key in request_receiver {
                    let icon = windows_tree_icons::load(&key);
                    if result_sender.send((key, icon)).is_err() {
                        break;
                    }
                    context.request_repaint();
                }
            });
            self.tree_icon_requests = Some(request_sender);
            self.tree_icon_results = Some(result_receiver);
        }
        #[cfg(not(windows))]
        let _ = context;
    }

    fn poll_tree_icons(&mut self, context: &egui::Context) {
        if let Some(receiver) = &self.tree_icon_results {
            while let Ok((key, icon)) = receiver.try_recv() {
                if let Some(rgba) = icon {
                    let image = egui::ColorImage::from_rgba_unmultiplied([20, 20], &rgba);
                    let texture = context.load_texture(
                        format!("shell-icon:{key:?}"),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    self.tree_icon_cache.insert(key, texture);
                    context.request_repaint();
                }
            }
        }
    }

    fn sidebar_detail_lines(&self, drives: &[DriveEntry]) -> Vec<String> {
        let drive = if self.computer_view {
            self.selected_drive.as_ref()
        } else if self.selected_paths.is_empty() && self.opened_archive.is_none() {
            Some(&self.current_directory)
        } else {
            None
        }
        .and_then(|path| {
            drives
                .iter()
                .find(|drive| paths_match_for_tree(&drive.path, path))
        });
        if let Some(drive) = drive {
            return vec![
                drive.name.clone(),
                "本地磁盘".to_owned(),
                format!("文件系统:  {}", drive.file_system),
                format!(
                    "可用空间:  {}",
                    drive
                        .available
                        .map(format_drive_bytes)
                        .unwrap_or_else(|| "-".to_owned())
                ),
                format!(
                    "总大小:  {}",
                    drive
                        .total
                        .map(format_drive_bytes)
                        .unwrap_or_else(|| "-".to_owned())
                ),
            ];
        }
        if self.computer_view {
            return vec!["此电脑".to_owned(), format!("{} 个磁盘分区", drives.len())];
        }
        if let Some(archive) = &self.opened_archive {
            if let Some(selected) = self.selected_archive_item.as_ref()
                && let Some(entry) = self.entries.iter().find(|entry| &entry.path == selected)
            {
                return vec![
                    entry.name.clone(),
                    entry.kind.clone(),
                    format!(
                        "大小:  {}",
                        entry
                            .size
                            .map(format_bytes)
                            .unwrap_or_else(|| "-".to_owned())
                    ),
                    format!(
                        "所在压缩包:  {}",
                        archive.file_name().unwrap_or_default().to_string_lossy()
                    ),
                ];
            }
            return vec![
                archive
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                "压缩文件".to_owned(),
                format!("包内项目:  {}", self.archive_entries.len()),
                format!("路径:  {}", display_path(archive)),
            ];
        }
        if self.selected_paths.len() > 1 {
            let count = self.selected_paths.len();
            let bytes = self
                .selected_paths
                .iter()
                .filter_map(|path| fs::metadata(path).ok())
                .filter(|metadata| metadata.is_file())
                .map(|metadata| metadata.len())
                .sum::<u64>();
            return vec![
                format!("已选中 {count} 个项目"),
                format!("文件大小合计:  {}", format_bytes(bytes)),
                format!("所在位置:  {}", display_path(&self.current_directory)),
            ];
        }
        let path = self
            .selected_paths
            .first()
            .unwrap_or(&self.current_directory);
        let metadata = fs::metadata(path).ok();
        let is_directory = metadata.as_ref().is_some_and(|metadata| metadata.is_dir());
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| display_path(path));
        let mut lines = vec![name, file_kind(path, is_directory)];
        if let Some(metadata) = metadata {
            if metadata.is_file() {
                lines.push(format!("大小:  {}", format_bytes(metadata.len())));
            }
            if let Ok(modified) = metadata.modified() {
                lines.push(format!("修改时间:  {}", format_system_time(modified)));
            }
        }
        lines.push(format!(
            "位置:  {}",
            display_path(path.parent().unwrap_or(path))
        ));
        lines
    }

    fn show_classic_list(&mut self, root: &mut egui::Ui) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(root, |ui| {
                let rect = ui.max_rect();
                ui.painter().rect_filled(rect, 0.0, egui::Color32::WHITE);
                let gutter = 28.0;
                let content_right = rect.right() - gutter;
                let content_w = content_right - rect.left();
                let widths = if self.computer_view {
                    let name = content_w * 0.34;
                    [
                        name,
                        (content_w - name) / 4.0,
                        (content_w - name) / 4.0,
                        (content_w - name) / 4.0,
                        (content_w - name) / 4.0,
                    ]
                } else {
                    let name = content_w * 0.42;
                    [
                        name,
                        (content_w - name) * 0.25,
                        (content_w - name) * 0.19,
                        (content_w - name) * 0.28,
                        (content_w - name) * 0.28,
                    ]
                };
                let mut bounds = [rect.left(); 6];
                for i in 0..5 {
                    bounds[i + 1] = bounds[i] + widths[i];
                }
                ui.painter().rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(rect.left(), rect.top() + 35.0),
                        egui::pos2(bounds[1], rect.bottom()),
                    ),
                    0.0,
                    egui::Color32::from_rgb(247, 247, 247),
                );
                let header = egui::Rect::from_min_max(
                    rect.min,
                    egui::pos2(content_right, rect.top() + 35.0),
                );
                ui.painter().rect_filled(header, 0.0, GRID_HEADER);
                ui.painter().line_segment(
                    [header.left_bottom(), header.right_bottom()],
                    egui::Stroke::new(1.0, LINE),
                );
                for x in bounds.iter().skip(1) {
                    ui.painter().line_segment(
                        [egui::pos2(*x, rect.top()), egui::pos2(*x, rect.bottom())],
                        egui::Stroke::new(1.0, LINE),
                    );
                }
                let titles = if self.computer_view {
                    ["名称", "类型", "总大小", "可用空间", "文件系统"]
                } else {
                    ["名称", "类型", "大小", "修改时间", ""]
                };
                for i in 0..5 {
                    let align = if i >= 2 && self.computer_view {
                        egui::Align2::RIGHT_CENTER
                    } else {
                        egui::Align2::LEFT_CENTER
                    };
                    let x = if i >= 2 && self.computer_view {
                        bounds[i + 1] - 10.0
                    } else {
                        bounds[i] + if i == 0 { 37.0 } else { 9.0 }
                    };
                    label(
                        ui.painter(),
                        egui::pos2(x, header.center().y),
                        titles[i],
                        16.0,
                        TEXT,
                        align,
                    );
                }
                label(
                    ui.painter(),
                    egui::pos2(bounds[0] + 24.0, header.center().y),
                    "◆",
                    10.0,
                    BLUE,
                    egui::Align2::CENTER_CENTER,
                );
                if self.computer_view {
                    let filter = self.search_text.to_lowercase();
                    let drives: Vec<_> = drive_entries()
                        .into_iter()
                        .filter(|d| filter.is_empty() || d.name.to_lowercase().contains(&filter))
                        .collect();
                    for (index, drive) in drives.into_iter().enumerate() {
                        let row = egui::Rect::from_min_max(
                            egui::pos2(rect.left(), header.bottom() + index as f32 * 31.0),
                            egui::pos2(content_right, header.bottom() + (index + 1) as f32 * 31.0),
                        );
                        if row.bottom() > rect.bottom() {
                            break;
                        }
                        let selected = self.selected_drive.as_ref() == Some(&drive.path);
                        if selected {
                            ui.painter().rect_filled(row, 0.0, SELECTED_ROW);
                        }
                        self.paint_system_tree_icon(
                            ui,
                            egui::pos2(row.left() + 18.0, row.center().y),
                            &drive.path,
                            4,
                        );
                        label(
                            ui.painter(),
                            egui::pos2(bounds[0] + 38.0, row.center().y),
                            &drive.name,
                            17.0,
                            TEXT,
                            egui::Align2::LEFT_CENTER,
                        );
                        label(
                            ui.painter(),
                            egui::pos2(bounds[1] + 9.0, row.center().y),
                            "本地磁盘",
                            16.0,
                            TEXT,
                            egui::Align2::LEFT_CENTER,
                        );
                        let numbers = [drive.total, drive.available];
                        for (column, number) in numbers.into_iter().enumerate() {
                            label(
                                ui.painter(),
                                egui::pos2(bounds[column + 3] - 10.0, row.center().y),
                                &number
                                    .map(format_drive_bytes)
                                    .unwrap_or_else(|| "-".to_owned()),
                                16.0,
                                TEXT,
                                egui::Align2::RIGHT_CENTER,
                            );
                        }
                        label(
                            ui.painter(),
                            egui::pos2(bounds[5] - 10.0, row.center().y),
                            &drive.file_system,
                            16.0,
                            TEXT,
                            egui::Align2::RIGHT_CENTER,
                        );
                        let response = ui.interact(
                            row,
                            egui::Id::new(("drive_row", index)),
                            egui::Sense::click(),
                        );
                        if response.clicked() {
                            self.selected_drive = Some(drive.path.clone());
                        }
                        if response.double_clicked() {
                            self.navigate_to(drive.path);
                        }
                    }
                } else {
                    let filter = self.search_text.to_lowercase();
                    let entries: Vec<_> = self
                        .entries
                        .iter()
                        .filter(|e| filter.is_empty() || e.name.to_lowercase().contains(&filter))
                        .cloned()
                        .collect();
                    let body = egui::Rect::from_min_max(
                        header.left_bottom(),
                        egui::pos2(content_right, rect.bottom()),
                    );
                    let visible = (body.height() / 29.0).floor().max(1.0) as usize;
                    let max_scroll = entries.len().saturating_sub(visible);
                    self.list_scroll = self.list_scroll.min(max_scroll);
                    if ui.rect_contains_pointer(body) {
                        let delta = ui.input(|i| i.smooth_scroll_delta.y);
                        if delta < -1.0 {
                            self.list_scroll = (self.list_scroll + 2).min(max_scroll);
                        }
                        if delta > 1.0 {
                            self.list_scroll = self.list_scroll.saturating_sub(2);
                        }
                    }
                    for (index, entry) in entries
                        .into_iter()
                        .skip(self.list_scroll)
                        .take(visible)
                        .enumerate()
                    {
                        let row = egui::Rect::from_min_max(
                            egui::pos2(rect.left(), body.top() + index as f32 * 29.0),
                            egui::pos2(content_right, body.top() + (index + 1) as f32 * 29.0),
                        );
                        if if self.opened_archive.is_some() {
                            self.selected_archive_item.as_ref() == Some(&entry.path)
                        } else {
                            self.selected_paths.contains(&entry.path)
                        } {
                            ui.painter().rect_filled(row, 0.0, SELECTED_ROW);
                        }
                        let icon_key = if self.opened_archive.is_some() {
                            archive_icon_key(&entry.path, entry.is_directory)
                        } else {
                            ShellIconKey::Real(entry.path.clone())
                        };
                        self.paint_shell_icon(
                            ui,
                            egui::pos2(row.left() + 18.0, row.center().y),
                            icon_key,
                            if entry.is_directory { 5 } else { 6 },
                        );
                        let clipped = ui.painter().with_clip_rect(egui::Rect::from_min_max(
                            row.min,
                            egui::pos2(bounds[1], row.bottom()),
                        ));
                        label(
                            &clipped,
                            egui::pos2(bounds[0] + 38.0, row.center().y),
                            &entry.name,
                            16.0,
                            TEXT,
                            egui::Align2::LEFT_CENTER,
                        );
                        label(
                            ui.painter(),
                            egui::pos2(bounds[1] + 9.0, row.center().y),
                            &entry.kind,
                            15.0,
                            TEXT,
                            egui::Align2::LEFT_CENTER,
                        );
                        label(
                            ui.painter(),
                            egui::pos2(bounds[3] - 10.0, row.center().y),
                            &entry.size.map(format_bytes).unwrap_or_default(),
                            15.0,
                            TEXT,
                            egui::Align2::RIGHT_CENTER,
                        );
                        label(
                            ui.painter(),
                            egui::pos2(bounds[3] + 9.0, row.center().y),
                            &entry.modified,
                            15.0,
                            TEXT,
                            egui::Align2::LEFT_CENTER,
                        );
                        let response = ui.interact(
                            row,
                            egui::Id::new(("file_row", index + self.list_scroll)),
                            egui::Sense::click(),
                        );
                        if response.clicked() {
                            if self.opened_archive.is_some() {
                                self.selected_archive_item = Some(entry.path.clone());
                            } else {
                                let additive =
                                    ui.input(|i| i.modifiers.ctrl || i.modifiers.command);
                                self.toggle_selection(&entry.path, additive);
                            }
                        }
                        if response.double_clicked() {
                            if self.opened_archive.is_some() && entry.is_directory {
                                self.enter_archive_directory(&entry.path);
                            } else if self.opened_archive.is_some() {
                                self.request_open_archive_entry(entry.path);
                            } else if entry.is_directory {
                                self.navigate_to(entry.path);
                            } else if is_archive_file(&entry.path) {
                                self.open_archive(entry.path);
                            } else if let Err(error) = crate::system_open::open_file(&entry.path) {
                                self.status = JobStatus::Error(format!("无法打开文件：{error}"));
                            }
                        }
                    }
                    if let Some(error) = &self.browser_error {
                        label(
                            ui.painter(),
                            egui::pos2(body.left() + 10.0, body.top() + 20.0),
                            error,
                            15.0,
                            egui::Color32::DARK_RED,
                            egui::Align2::LEFT_TOP,
                        );
                    }
                }
            });
    }
}

fn label(
    painter: &egui::Painter,
    pos: egui::Pos2,
    text: &str,
    size: f32,
    color: egui::Color32,
    align: egui::Align2,
) {
    painter.text(pos, align, text, egui::FontId::proportional(size), color);
}

fn paint_window_control(painter: &egui::Painter, center: egui::Pos2, action: usize) {
    let white = egui::Color32::WHITE;
    let stroke = egui::Stroke::new(2.2, white);
    match action {
        0 => {
            painter.line_segment(
                [
                    center + egui::vec2(-7.0, -7.0),
                    center + egui::vec2(7.0, 7.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    center + egui::vec2(-7.0, 7.0),
                    center + egui::vec2(7.0, -7.0),
                ],
                stroke,
            );
        }
        1 => {
            painter.rect_stroke(
                egui::Rect::from_center_size(center, egui::vec2(13.0, 12.0)),
                0.0,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        2 => {
            painter.line_segment(
                [
                    center + egui::vec2(-7.0, 4.0),
                    center + egui::vec2(7.0, 4.0),
                ],
                stroke,
            );
        }
        3 => {
            painter.line_segment(
                [
                    center + egui::vec2(-8.0, -5.0),
                    center + egui::vec2(8.0, -5.0),
                ],
                stroke,
            );
            painter.add(egui::Shape::convex_polygon(
                vec![
                    center + egui::vec2(-8.0, 0.0),
                    center + egui::vec2(8.0, 0.0),
                    center + egui::vec2(0.0, 7.0),
                ],
                white,
                egui::Stroke::NONE,
            ));
        }
        _ => {
            let bubble = egui::Rect::from_center_size(
                center + egui::vec2(0.0, -2.0),
                egui::vec2(17.0, 12.0),
            );
            painter.rect_filled(bubble, 3.0, white);
            painter.add(egui::Shape::convex_polygon(
                vec![
                    center + egui::vec2(-3.0, 4.0),
                    center + egui::vec2(1.0, 4.0),
                    center + egui::vec2(-2.0, 8.0),
                ],
                white,
                egui::Stroke::NONE,
            ));
            for dx in [-4.0, 0.0, 4.0] {
                painter.circle_filled(center + egui::vec2(dx, -2.0), 1.0, BLUE);
            }
        }
    }
}

fn paint_navigation_icon(painter: &egui::Painter, center: egui::Pos2, index: usize, enabled: bool) {
    let color = if enabled {
        BLUE
    } else {
        egui::Color32::from_rgb(198, 205, 214)
    };
    let stroke = egui::Stroke::new(2.5, color);
    match index {
        0 | 1 => {
            let dir = if index == 0 { -1.0 } else { 1.0 };
            painter.line_segment(
                [
                    center + egui::vec2(-9.0 * dir, 0.0),
                    center + egui::vec2(9.0 * dir, 0.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    center + egui::vec2(9.0 * dir, 0.0),
                    center + egui::vec2(1.0 * dir, -8.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    center + egui::vec2(9.0 * dir, 0.0),
                    center + egui::vec2(1.0 * dir, 8.0),
                ],
                stroke,
            );
        }
        2 | 5 => triangle(painter, center, 5.0, color),
        3 => {
            painter.line_segment(
                [
                    center + egui::vec2(-8.0, 5.0),
                    center + egui::vec2(8.0, 5.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    center + egui::vec2(0.0, 5.0),
                    center + egui::vec2(0.0, -7.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    center + egui::vec2(0.0, -7.0),
                    center + egui::vec2(-5.0, -2.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    center + egui::vec2(0.0, -7.0),
                    center + egui::vec2(5.0, -2.0),
                ],
                stroke,
            );
        }
        _ => {
            for y in [-6.0, 0.0, 6.0] {
                painter.circle_filled(center + egui::vec2(-8.0, y), 1.5, color);
                painter.line_segment(
                    [center + egui::vec2(-4.0, y), center + egui::vec2(10.0, y)],
                    egui::Stroke::new(2.0, color),
                );
            }
        }
    }
}

fn paint_blue_band(painter: &egui::Painter, rect: egui::Rect) {
    let start = (HEADER_DARK.r(), HEADER_DARK.g(), HEADER_DARK.b());
    let end = (3u8, 133u8, 209u8);
    for i in 0..16 {
        let t = (i as f32 + 0.5) / 16.0;
        let blend = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t) as u8;
        let color = egui::Color32::from_rgb(
            blend(start.0, end.0),
            blend(start.1, end.1),
            blend(start.2, end.2),
        );
        let top = rect.top() + rect.height() * i as f32 / 16.0;
        let bottom = rect.top() + rect.height() * (i + 1) as f32 / 16.0;
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(rect.left(), top),
                egui::pos2(rect.right(), bottom),
            ),
            0.0,
            color,
        );
    }
    let width = rect.width();
    painter.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(rect.left() + width * 0.42, rect.top()),
            egui::pos2(rect.left() + width * 0.83, rect.top()),
            egui::pos2(rect.left() + width * 0.76, rect.bottom()),
            egui::pos2(rect.left() + width * 0.37, rect.bottom()),
        ],
        HEADER_LIGHT,
        egui::Stroke::NONE,
    ));
    painter.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(rect.left() + width * 0.59, rect.top()),
            egui::pos2(rect.left() + width * 0.75, rect.top()),
            egui::pos2(rect.left() + width * 0.68, rect.bottom()),
            egui::pos2(rect.left() + width * 0.52, rect.bottom()),
        ],
        egui::Color32::from_rgba_unmultiplied(24, 145, 218, 65),
        egui::Stroke::NONE,
    ));
    painter.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(rect.left() + width * 0.96, rect.top()),
            rect.right_top(),
            rect.right_bottom(),
            egui::pos2(rect.left() + width * 0.86, rect.bottom()),
        ],
        egui::Color32::from_rgb(13, 142, 199),
        egui::Stroke::NONE,
    ));
}

fn panel_heading(painter: &egui::Painter, rect: egui::Rect, title: &str, border: bool) {
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(235, 246, 255));
    if border {
        painter.line_segment(
            [rect.left_top(), rect.right_top()],
            egui::Stroke::new(1.0, LINE),
        );
    }
    label(
        painter,
        egui::pos2(rect.left() + 9.0, rect.center().y),
        title,
        17.0,
        TEXT,
        egui::Align2::LEFT_CENTER,
    );
}

fn paths_match_for_tree(left: &Path, right: &Path) -> bool {
    if cfg!(windows) {
        display_path(left)
            .trim_end_matches(['\\', '/'])
            .eq_ignore_ascii_case(display_path(right).trim_end_matches(['\\', '/']))
    } else {
        left == right
    }
}

fn archive_icon_key(path: &Path, is_directory: bool) -> ShellIconKey {
    if is_directory {
        ShellIconKey::VirtualFolder
    } else {
        let extension = path
            .extension()
            .map(|part| format!(".{}", part.to_string_lossy().to_lowercase()))
            .unwrap_or_default();
        ShellIconKey::VirtualFile(extension)
    }
}

pub(super) fn child_directories(parent: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(parent) else {
        return Vec::new();
    };
    let mut children: Vec<_> = entries
        .flatten()
        .filter(|entry| {
            entry
                .file_type()
                .is_ok_and(|kind| kind.is_dir() && !kind.is_symlink())
        })
        .map(|entry| entry.path())
        .collect();
    children.sort_by_key(|path| {
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase()
    });
    children
}

fn paint_tree_arrow(painter: &egui::Painter, center: egui::Pos2, expanded: bool) {
    let points = if expanded {
        vec![
            center + egui::vec2(-4.0, -2.0),
            center + egui::vec2(4.0, -2.0),
            center + egui::vec2(0.0, 3.0),
        ]
    } else {
        vec![
            center + egui::vec2(-2.0, -4.0),
            center + egui::vec2(3.0, 0.0),
            center + egui::vec2(-2.0, 4.0),
        ]
    };
    painter.add(egui::Shape::convex_polygon(
        points,
        egui::Color32::from_rgb(115, 130, 144),
        egui::Stroke::NONE,
    ));
}

fn paint_scroll_arrow(painter: &egui::Painter, center: egui::Pos2, downward: bool) {
    let points = if downward {
        vec![
            center + egui::vec2(-4.0, -2.0),
            center + egui::vec2(4.0, -2.0),
            center + egui::vec2(0.0, 3.0),
        ]
    } else {
        vec![
            center + egui::vec2(-4.0, 2.0),
            center + egui::vec2(4.0, 2.0),
            center + egui::vec2(0.0, -3.0),
        ]
    };
    painter.add(egui::Shape::convex_polygon(
        points,
        egui::Color32::from_rgb(91, 111, 128),
        egui::Stroke::NONE,
    ));
}

fn framed_field(painter: &egui::Painter, rect: egui::Rect) {
    painter.rect_filled(rect, 0.0, egui::Color32::WHITE);
    painter.rect_stroke(
        rect,
        0.0,
        egui::Stroke::new(1.0, egui::Color32::from_rgb(190, 207, 224)),
        egui::StrokeKind::Inside,
    );
}

fn triangle(painter: &egui::Painter, center: egui::Pos2, r: f32, color: egui::Color32) {
    painter.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(center.x - r, center.y - r * 0.4),
            egui::pos2(center.x + r, center.y - r * 0.4),
            egui::pos2(center.x, center.y + r * 0.6),
        ],
        color,
        egui::Stroke::NONE,
    ));
}

fn paint_search(painter: &egui::Painter, center: egui::Pos2, color: egui::Color32) {
    painter.circle_stroke(
        egui::pos2(center.x - 3.0, center.y - 3.0),
        6.0,
        egui::Stroke::new(2.5, color),
    );
    painter.line_segment(
        [
            egui::pos2(center.x + 2.0, center.y + 2.0),
            egui::pos2(center.x + 9.0, center.y + 9.0),
        ],
        egui::Stroke::new(3.0, color),
    );
}

fn paint_archive_stack(painter: &egui::Painter, center: egui::Pos2, size: f32) {
    let s = size;
    let p = |x: f32, y: f32| egui::pos2(center.x + x * s, center.y + y * s);
    let quad = |points: [(f32, f32); 4], color| {
        painter.add(egui::Shape::convex_polygon(
            points.into_iter().map(|(x, y)| p(x, y)).collect(),
            color,
            egui::Stroke::NONE,
        ));
    };
    painter.rect_filled(
        egui::Rect::from_min_max(p(-0.48, 0.44), p(0.49, 0.49)),
        2.0,
        egui::Color32::from_black_alpha(55),
    );
    // HaoZip's archive mark is a stack of three solid volumes, not three loose stripes.
    quad(
        [(-0.40, -0.48), (0.39, -0.48), (0.47, -0.22), (-0.48, -0.22)],
        egui::Color32::from_rgb(38, 166, 210),
    );
    quad(
        [(-0.48, -0.22), (0.47, -0.22), (0.47, -0.07), (-0.48, -0.07)],
        egui::Color32::from_rgb(17, 137, 192),
    );
    quad(
        [(-0.48, -0.07), (0.47, -0.07), (0.49, 0.19), (-0.49, 0.19)],
        egui::Color32::from_rgb(57, 174, 74),
    );
    quad(
        [(-0.49, 0.19), (0.49, 0.19), (0.48, 0.43), (-0.48, 0.43)],
        egui::Color32::from_rgb(218, 93, 43),
    );
    painter.line_segment(
        [p(-0.47, 0.16), p(0.47, 0.16)],
        egui::Stroke::new((s * 0.035).max(1.0), egui::Color32::from_rgb(34, 133, 50)),
    );
    painter.line_segment(
        [p(-0.47, 0.40), p(0.47, 0.40)],
        egui::Stroke::new((s * 0.035).max(1.0), egui::Color32::from_rgb(167, 62, 31)),
    );
    quad(
        [(-0.14, -0.46), (0.12, -0.46), (0.13, 0.43), (-0.13, 0.43)],
        egui::Color32::from_rgb(209, 153, 79),
    );
    painter.rect_filled(
        egui::Rect::from_min_max(p(-0.075, -0.46), p(0.055, 0.43)),
        0.0,
        egui::Color32::from_rgb(244, 201, 119),
    );
    painter.rect_filled(
        egui::Rect::from_min_max(p(-0.16, -0.09), p(0.16, 0.10)),
        (s * 0.025).max(1.0),
        egui::Color32::from_rgb(121, 71, 34),
    );
    painter.rect_filled(
        egui::Rect::from_min_max(p(-0.095, -0.055), p(0.095, 0.065)),
        (s * 0.01).max(1.0),
        egui::Color32::from_rgb(242, 204, 132),
    );
}

fn paint_large_icon(painter: &egui::Painter, center: egui::Pos2, index: usize) {
    let pale = egui::Color32::from_rgb(220, 241, 251);
    let white = egui::Color32::from_rgb(244, 251, 255);
    let dark_blue = egui::Color32::from_rgb(31, 126, 183);
    match index {
        0 | 1 => {
            let box_rect = egui::Rect::from_center_size(
                egui::pos2(center.x, center.y + 7.0),
                egui::vec2(52.0, 43.0),
            );
            painter.rect_filled(
                box_rect.translate(egui::vec2(1.0, 3.0)),
                1.0,
                egui::Color32::from_black_alpha(40),
            );
            painter.rect_filled(box_rect, 1.0, egui::Color32::from_rgb(55, 164, 214));
            painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(box_rect.left(), box_rect.top() + 24.0),
                    egui::vec2(52.0, 8.0),
                ),
                0.0,
                egui::Color32::from_rgb(66, 175, 67),
            );
            painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(box_rect.left(), box_rect.top() + 33.0),
                    egui::vec2(52.0, 10.0),
                ),
                0.0,
                egui::Color32::from_rgb(213, 102, 45),
            );
            painter.rect_filled(
                egui::Rect::from_center_size(
                    egui::pos2(center.x, center.y + 12.0),
                    egui::vec2(11.0, 31.0),
                ),
                1.0,
                egui::Color32::from_rgb(169, 90, 41),
            );
            painter.rect_filled(
                egui::Rect::from_center_size(
                    egui::pos2(center.x, center.y + 11.0),
                    egui::vec2(6.0, 12.0),
                ),
                1.0,
                egui::Color32::from_rgb(232, 193, 129),
            );
            painter.rect_filled(
                egui::Rect::from_center_size(
                    egui::pos2(center.x, center.y + 11.0),
                    egui::vec2(3.0, 7.0),
                ),
                0.0,
                egui::Color32::from_rgb(124, 68, 40),
            );
            if index == 0 {
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        egui::pos2(center.x - 13.0, center.y - 24.0),
                        egui::pos2(center.x + 13.0, center.y - 24.0),
                        egui::pos2(center.x, center.y - 4.0),
                    ],
                    white,
                    egui::Stroke::NONE,
                ));
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(center.x - 5.0, center.y - 36.0),
                        egui::pos2(center.x + 5.0, center.y - 22.0),
                    ),
                    0.0,
                    white,
                );
            } else {
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        egui::pos2(center.x - 14.0, center.y - 23.0),
                        egui::pos2(center.x + 14.0, center.y - 23.0),
                        egui::pos2(center.x, center.y - 37.0),
                    ],
                    white,
                    egui::Stroke::NONE,
                ));
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(center.x - 5.0, center.y - 23.0),
                        egui::pos2(center.x + 5.0, center.y - 6.0),
                    ),
                    0.0,
                    white,
                );
            }
        }
        2 => {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    center + egui::vec2(-20.0, -15.0),
                    center + egui::vec2(20.0, -15.0),
                    center + egui::vec2(16.0, 25.0),
                    center + egui::vec2(-16.0, 25.0),
                ],
                pale,
                egui::Stroke::NONE,
            ));
            painter.add(egui::Shape::convex_polygon(
                vec![
                    center + egui::vec2(-20.0, -15.0),
                    center + egui::vec2(-12.0, -15.0),
                    center + egui::vec2(-9.0, 23.0),
                    center + egui::vec2(-16.0, 25.0),
                ],
                egui::Color32::from_rgb(186, 218, 235),
                egui::Stroke::NONE,
            ));
            for (rx, ry, y, color) in [
                (23.0, 5.0, -17.0, egui::Color32::from_rgb(72, 183, 230)),
                (19.0, 2.7, -18.0, egui::Color32::from_rgb(164, 220, 244)),
            ] {
                painter.add(egui::Shape::convex_polygon(
                    (0..24)
                        .map(|step| {
                            let angle = std::f32::consts::TAU * step as f32 / 24.0;
                            center + egui::vec2(rx * angle.cos(), y + ry * angle.sin())
                        })
                        .collect(),
                    color,
                    egui::Stroke::NONE,
                ));
            }
            painter.rect_filled(
                egui::Rect::from_min_max(
                    center + egui::vec2(-23.0, -18.0),
                    center + egui::vec2(23.0, -11.0),
                ),
                3.0,
                egui::Color32::from_rgb(61, 174, 225),
            );
            label(
                painter,
                egui::pos2(center.x, center.y + 7.0),
                "♻",
                23.0,
                egui::Color32::from_rgb(40, 162, 218),
                egui::Align2::CENTER_CENTER,
            );
        }
        3 => {
            painter.circle_stroke(
                egui::pos2(center.x, center.y - 13.0),
                17.0,
                egui::Stroke::new(7.0, egui::Color32::from_rgb(181, 218, 241)),
            );
            painter.rect_filled(
                egui::Rect::from_min_max(
                    center + egui::vec2(-17.0, -13.0),
                    center + egui::vec2(17.0, 1.0),
                ),
                0.0,
                egui::Color32::from_rgb(10, 127, 202),
            );
            painter.rect_filled(
                egui::Rect::from_center_size(
                    egui::pos2(center.x, center.y + 10.0),
                    egui::vec2(47.0, 35.0),
                ),
                2.0,
                pale,
            );
            painter.rect_filled(
                egui::Rect::from_min_max(
                    center + egui::vec2(-23.0, -7.0),
                    center + egui::vec2(-18.0, 27.0),
                ),
                0.0,
                egui::Color32::from_rgb(189, 224, 242),
            );
            painter.circle_filled(egui::pos2(center.x, center.y + 5.0), 4.0, dark_blue);
            painter.rect_filled(
                egui::Rect::from_center_size(
                    egui::pos2(center.x, center.y + 12.0),
                    egui::vec2(3.0, 11.0),
                ),
                0.0,
                dark_blue,
            );
        }
        4 => {
            painter.rect_filled(
                egui::Rect::from_center_size(
                    center + egui::vec2(0.0, 12.0),
                    egui::vec2(53.0, 37.0),
                )
                .translate(egui::vec2(1.0, 2.0)),
                0.0,
                egui::Color32::from_black_alpha(35),
            );
            painter.rect_filled(
                egui::Rect::from_center_size(
                    egui::pos2(center.x, center.y + 10.0),
                    egui::vec2(53.0, 36.0),
                ),
                1.0,
                egui::Color32::from_rgb(82, 181, 224),
            );
            painter.rect_filled(
                egui::Rect::from_min_max(
                    center + egui::vec2(-26.0, 19.0),
                    center + egui::vec2(26.0, 28.0),
                ),
                0.0,
                egui::Color32::from_rgb(42, 151, 202),
            );
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(center.x - 30.0, center.y - 9.0),
                    egui::pos2(center.x - 15.0, center.y - 20.0),
                    egui::pos2(center.x + 15.0, center.y - 20.0),
                    egui::pos2(center.x + 30.0, center.y - 9.0),
                ],
                egui::Color32::from_rgb(184, 226, 247),
                egui::Stroke::NONE,
            ));
            painter.add(egui::Shape::convex_polygon(
                vec![
                    center + egui::vec2(-30.0, -9.0),
                    center + egui::vec2(-15.0, -9.0),
                    center + egui::vec2(-9.0, 1.0),
                    center + egui::vec2(-29.0, 1.0),
                ],
                egui::Color32::from_rgb(133, 205, 239),
                egui::Stroke::NONE,
            ));
            painter.add(egui::Shape::convex_polygon(
                vec![
                    center + egui::vec2(15.0, -9.0),
                    center + egui::vec2(30.0, -9.0),
                    center + egui::vec2(29.0, 1.0),
                    center + egui::vec2(9.0, 1.0),
                ],
                egui::Color32::from_rgb(133, 205, 239),
                egui::Stroke::NONE,
            ));
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(center.x - 16.0, center.y - 24.0),
                    egui::pos2(center.x + 16.0, center.y - 24.0),
                    egui::pos2(center.x, center.y - 39.0),
                ],
                white,
                egui::Stroke::NONE,
            ));
        }
        _ => {
            painter.rect_stroke(
                egui::Rect::from_center_size(
                    egui::pos2(center.x, center.y - 15.0),
                    egui::vec2(26.0, 15.0),
                ),
                4.0,
                egui::Stroke::new(3.0, egui::Color32::from_rgb(36, 67, 77)),
                egui::StrokeKind::Inside,
            );
            painter.rect_filled(
                egui::Rect::from_center_size(center + egui::vec2(1.0, 9.0), egui::vec2(53.0, 40.0)),
                2.0,
                egui::Color32::from_black_alpha(37),
            );
            painter.rect_filled(
                egui::Rect::from_center_size(
                    egui::pos2(center.x, center.y + 8.0),
                    egui::vec2(53.0, 40.0),
                ),
                2.0,
                white,
            );
            painter.rect_filled(
                egui::Rect::from_min_max(
                    center + egui::vec2(-25.0, 12.0),
                    center + egui::vec2(25.0, 26.0),
                ),
                0.0,
                pale,
            );
            painter.rect_filled(
                egui::Rect::from_center_size(
                    egui::pos2(center.x, center.y - 2.0),
                    egui::vec2(53.0, 5.0),
                ),
                0.0,
                egui::Color32::from_rgb(142, 190, 209),
            );
            painter.rect_filled(
                egui::Rect::from_center_size(
                    egui::pos2(center.x, center.y + 5.0),
                    egui::vec2(10.0, 14.0),
                ),
                1.0,
                egui::Color32::from_rgb(94, 176, 46),
            );
            painter.rect_filled(
                egui::Rect::from_center_size(center + egui::vec2(0.0, 0.0), egui::vec2(3.0, 6.0)),
                1.0,
                egui::Color32::from_rgb(185, 225, 135),
            );
        }
    }
}

fn paint_monitor(painter: &egui::Painter, center: egui::Pos2, width: f32) {
    let frame = egui::Rect::from_center_size(
        egui::pos2(center.x, center.y - 2.0),
        egui::vec2(width, width * 0.72),
    );
    painter.rect_filled(frame, 1.0, egui::Color32::from_rgb(86, 117, 136));
    painter.rect_filled(
        frame.shrink(width * 0.10),
        0.0,
        egui::Color32::from_rgb(63, 170, 220),
    );
    painter.add(egui::Shape::convex_polygon(
        vec![
            frame.left_top() + egui::vec2(2.0, 2.0),
            frame.right_top() + egui::vec2(-2.0, 2.0),
            frame.left_bottom() + egui::vec2(2.0, -3.0),
        ],
        egui::Color32::from_white_alpha(42),
        egui::Stroke::NONE,
    ));
    painter.line_segment(
        [
            egui::pos2(center.x, center.y + 5.0),
            egui::pos2(center.x, center.y + 9.0),
        ],
        egui::Stroke::new(2.0, egui::Color32::from_rgb(105, 115, 126)),
    );
    painter.line_segment(
        [
            egui::pos2(center.x - 8.0, center.y + 9.0),
            egui::pos2(center.x + 8.0, center.y + 9.0),
        ],
        egui::Stroke::new(2.0, egui::Color32::from_rgb(105, 115, 126)),
    );
}

fn paint_disk(painter: &egui::Painter, center: egui::Pos2, width: f32) {
    let rect = egui::Rect::from_center_size(center, egui::vec2(width, width * 0.5));
    painter.rect_filled(rect, 1.0, egui::Color32::from_rgb(202, 214, 220));
    painter.rect_filled(
        egui::Rect::from_min_max(
            rect.min + egui::vec2(1.0, 1.0),
            rect.max - egui::vec2(1.0, 3.0),
        ),
        1.0,
        egui::Color32::from_rgb(239, 243, 245),
    );
    painter.rect_filled(
        egui::Rect::from_min_size(
            egui::pos2(rect.left(), rect.bottom() - 3.0),
            egui::vec2(width, 3.0),
        ),
        0.0,
        egui::Color32::from_rgb(76, 94, 99),
    );
    painter.line_segment(
        [
            rect.left_top() + egui::vec2(2.0, 2.0),
            rect.right_top() + egui::vec2(-2.0, 2.0),
        ],
        egui::Stroke::new(1.0, egui::Color32::WHITE),
    );
    painter.circle_filled(
        egui::pos2(rect.right() - 3.0, rect.bottom() - 2.0),
        1.6,
        egui::Color32::from_rgb(55, 171, 91),
    );
}

fn paint_tree_icon(painter: &egui::Painter, center: egui::Pos2, index: u8) {
    match index {
        0 | 3 => paint_monitor(painter, center, 20.0),
        1 => {
            painter.circle_filled(
                egui::pos2(center.x + 2.0, center.y + 4.0),
                8.0,
                egui::Color32::from_rgb(28, 121, 192),
            );
            painter.circle_filled(
                egui::pos2(center.x - 5.0, center.y + 2.0),
                5.0,
                egui::Color32::from_rgb(42, 147, 207),
            );
            painter.circle_filled(
                egui::pos2(center.x + 2.0, center.y - 2.0),
                7.0,
                egui::Color32::from_rgb(42, 147, 207),
            );
            painter.circle_filled(
                egui::pos2(center.x + 8.0, center.y + 3.0),
                5.0,
                egui::Color32::from_rgb(42, 147, 207),
            );
            painter.line_segment(
                [
                    center + egui::vec2(-8.0, 6.0),
                    center + egui::vec2(9.0, 6.0),
                ],
                egui::Stroke::new(1.5, egui::Color32::from_rgb(111, 191, 232)),
            );
        }
        2 => {
            painter.circle_filled(
                egui::pos2(center.x, center.y - 5.0),
                5.5,
                egui::Color32::from_rgb(91, 76, 61),
            );
            painter.circle_filled(
                egui::pos2(center.x, center.y - 3.5),
                4.4,
                egui::Color32::from_rgb(222, 182, 136),
            );
            painter.rect_filled(
                egui::Rect::from_center_size(
                    egui::pos2(center.x, center.y + 5.0),
                    egui::vec2(13.0, 10.0),
                ),
                3.0,
                egui::Color32::from_rgb(68, 151, 133),
            );
        }
        4 => paint_disk(painter, center, 20.0),
        _ => paint_file_icon(painter, center, true),
    }
}

fn paint_file_icon(painter: &egui::Painter, center: egui::Pos2, directory: bool) {
    if directory {
        painter.rect_filled(
            egui::Rect::from_min_max(
                center + egui::vec2(-9.0, -8.0),
                center + egui::vec2(-1.0, -3.0),
            ),
            1.0,
            egui::Color32::from_rgb(231, 177, 70),
        );
        painter.rect_filled(
            egui::Rect::from_min_max(
                center + egui::vec2(-10.0, -5.0),
                center + egui::vec2(10.0, 7.0),
            ),
            1.0,
            egui::Color32::from_rgb(245, 190, 69),
        );
        painter.line_segment(
            [
                center + egui::vec2(-9.0, -3.0),
                center + egui::vec2(9.0, -3.0),
            ],
            egui::Stroke::new(1.0, egui::Color32::from_rgb(255, 222, 138)),
        );
    } else {
        painter.rect_filled(
            egui::Rect::from_center_size(center, egui::vec2(15.0, 18.0)),
            1.0,
            egui::Color32::from_rgb(218, 231, 242),
        );
        painter.line_segment(
            [
                center + egui::vec2(-4.0, -3.0),
                center + egui::vec2(4.0, -3.0),
            ],
            egui::Stroke::new(1.0, egui::Color32::from_rgb(112, 166, 198)),
        );
    }
}

fn format_drive_bytes(bytes: u64) -> String {
    format!("{:.2} GB", bytes as f64 / 1_073_741_824.0)
}

pub(super) fn drive_entries() -> Vec<DriveEntry> {
    drive_locations()
        .into_iter()
        .map(|(label, path)| {
            let (available, total, file_system) = drive_info(&path);
            let name = if cfg!(windows) && path.to_string_lossy().starts_with("C:") {
                "Windows  (C:)".to_owned()
            } else {
                label
            };
            DriveEntry {
                path,
                name,
                total,
                available,
                file_system,
            }
        })
        .collect()
}

#[cfg(windows)]
fn drive_info(path: &Path) -> (Option<u64>, Option<u64>, String) {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn GetDiskFreeSpaceExW(
            directory: *const u16,
            available: *mut u64,
            total: *mut u64,
            free: *mut u64,
        ) -> i32;
        fn GetVolumeInformationW(
            root: *const u16,
            volume: *mut u16,
            volume_size: u32,
            serial: *mut u32,
            max_name: *mut u32,
            flags: *mut u32,
            file_system: *mut u16,
            file_system_size: u32,
        ) -> i32;
    }
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut available = 0u64;
    let mut total = 0u64;
    let mut free = 0u64;
    let mut file_system = [0u16; 32];
    // Windows fills these output buffers for the drive root named by `wide`.
    let metrics_ok =
        unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut available, &mut total, &mut free) != 0 };
    let fs_ok = unsafe {
        GetVolumeInformationW(
            wide.as_ptr(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            file_system.as_mut_ptr(),
            file_system.len() as u32,
        ) != 0
    };
    let fs_name = if fs_ok {
        String::from_utf16_lossy(
            &file_system[..file_system
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(file_system.len())],
        )
    } else {
        "-".to_owned()
    };
    (
        metrics_ok.then_some(available),
        metrics_ok.then_some(total),
        fs_name,
    )
}

#[cfg(not(windows))]
fn drive_info(_path: &Path) -> (Option<u64>, Option<u64>, String) {
    (None, None, "-".to_owned())
}

#[cfg(windows)]
mod windows_tree_icons {
    use super::ShellIconKey;
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    const SIZE: usize = 20;

    #[repr(C)]
    struct ShellFileInfo {
        icon: *mut c_void,
        icon_index: i32,
        attributes: u32,
        display_name: [u16; 260],
        type_name: [u16; 80],
    }

    #[repr(C)]
    struct BitmapInfoHeader {
        size: u32,
        width: i32,
        height: i32,
        planes: u16,
        bit_count: u16,
        compression: u32,
        image_size: u32,
        x_pixels_per_meter: i32,
        y_pixels_per_meter: i32,
        colors_used: u32,
        colors_important: u32,
    }

    #[repr(C)]
    struct BitmapInfo {
        header: BitmapInfoHeader,
        colors: [u32; 1],
    }

    #[link(name = "Shell32")]
    unsafe extern "system" {
        fn SHGetFileInfoW(
            path: *const u16,
            attributes: u32,
            info: *mut ShellFileInfo,
            info_size: u32,
            flags: u32,
        ) -> usize;
    }

    #[link(name = "User32")]
    unsafe extern "system" {
        fn DrawIconEx(
            device: *mut c_void,
            x: i32,
            y: i32,
            icon: *mut c_void,
            width: i32,
            height: i32,
            step: u32,
            brush: *mut c_void,
            flags: u32,
        ) -> i32;
        fn DestroyIcon(icon: *mut c_void) -> i32;
    }

    #[link(name = "Gdi32")]
    unsafe extern "system" {
        fn CreateCompatibleDC(device: *mut c_void) -> *mut c_void;
        fn CreateDIBSection(
            device: *mut c_void,
            info: *const BitmapInfo,
            usage: u32,
            pixels: *mut *mut c_void,
            section: *mut c_void,
            offset: u32,
        ) -> *mut c_void;
        fn SelectObject(device: *mut c_void, object: *mut c_void) -> *mut c_void;
        fn DeleteObject(object: *mut c_void) -> i32;
        fn DeleteDC(device: *mut c_void) -> i32;
        fn GdiFlush() -> i32;
    }

    pub(super) fn load(key: &ShellIconKey) -> Option<Vec<u8>> {
        let (path, attributes, flags) = match key {
            ShellIconKey::Real(path) => (path.clone(), 0, 0x100 | 0x1 | 0x20),
            ShellIconKey::VirtualFile(extension) => {
                (Path::new(extension).to_path_buf(), 0x80, 0x100 | 0x1 | 0x10)
            }
            ShellIconKey::VirtualFolder => {
                (Path::new("folder").to_path_buf(), 0x10, 0x100 | 0x1 | 0x10)
            }
        };
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        // Shell extraction can block on cloud/network folders; callers run this on a worker.
        let mut info: ShellFileInfo = unsafe { std::mem::zeroed() };
        let found = unsafe {
            SHGetFileInfoW(
                wide.as_ptr(),
                attributes,
                &mut info,
                size_of::<ShellFileInfo>() as u32,
                flags,
            )
        };
        if found == 0 || info.icon.is_null() {
            return None;
        }
        let result = unsafe { draw_icon(info.icon) };
        unsafe { DestroyIcon(info.icon) };
        result
    }

    unsafe fn draw_icon(icon: *mut c_void) -> Option<Vec<u8>> {
        let device = unsafe { CreateCompatibleDC(std::ptr::null_mut()) };
        if device.is_null() {
            return None;
        }
        let info = BitmapInfo {
            header: BitmapInfoHeader {
                size: size_of::<BitmapInfoHeader>() as u32,
                width: SIZE as i32,
                height: -(SIZE as i32),
                planes: 1,
                bit_count: 32,
                compression: 0,
                image_size: 0,
                x_pixels_per_meter: 0,
                y_pixels_per_meter: 0,
                colors_used: 0,
                colors_important: 0,
            },
            colors: [0],
        };
        let mut pixels: *mut c_void = std::ptr::null_mut();
        let bitmap =
            unsafe { CreateDIBSection(device, &info, 0, &mut pixels, std::ptr::null_mut(), 0) };
        if bitmap.is_null() || pixels.is_null() {
            unsafe { DeleteDC(device) };
            return None;
        }
        let old = unsafe { SelectObject(device, bitmap) };
        let mut result = None;
        if !old.is_null() {
            unsafe { std::ptr::write_bytes(pixels, 0, SIZE * SIZE * 4) };
            let drawn = unsafe {
                DrawIconEx(
                    device,
                    0,
                    0,
                    icon,
                    SIZE as i32,
                    SIZE as i32,
                    0,
                    std::ptr::null_mut(),
                    0x3,
                )
            };
            if drawn != 0 {
                unsafe { GdiFlush() };
                let bgra =
                    unsafe { std::slice::from_raw_parts(pixels as *const u8, SIZE * SIZE * 4) };
                let mut rgba = Vec::with_capacity(SIZE * SIZE * 4);
                for pixel in bgra.chunks_exact(4) {
                    let alpha = pixel[3];
                    let straight = |channel: u8| {
                        if alpha == 0 || alpha == 255 {
                            channel
                        } else {
                            ((channel as u32 * 255) / alpha as u32).min(255) as u8
                        }
                    };
                    rgba.extend_from_slice(&[
                        straight(pixel[2]),
                        straight(pixel[1]),
                        straight(pixel[0]),
                        alpha,
                    ]);
                }
                result = Some(rgba);
            }
            unsafe { SelectObject(device, old) };
        }
        unsafe {
            DeleteObject(bitmap);
            DeleteDC(device);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expanding_tree_reads_real_nested_folders() {
        let root = tempfile::tempdir().unwrap();
        let parent = root.path().join("parent");
        let child = parent.join("child");
        fs::create_dir_all(&child).unwrap();
        let mut app = MiaoZipApp::default();
        let mut rows = Vec::new();
        app.push_tree_path(&mut rows, "parent".to_owned(), parent.clone(), 5, 0);
        assert_eq!(rows.len(), 1);
        app.expanded_tree_paths.insert(parent.clone());
        rows.clear();
        app.push_tree_path(&mut rows, "parent".to_owned(), parent, 5, 0);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].path.as_ref(), Some(&child));
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires a running Windows desktop Shell"]
    fn native_windows_folder_icon_is_renderable() {
        let rgba = windows_tree_icons::load(&ShellIconKey::Real(std::env::temp_dir())).unwrap();
        assert_eq!(rgba.len(), 20 * 20 * 4);
        assert!(rgba.chunks_exact(4).any(|pixel| pixel[3] != 0));
        let document =
            windows_tree_icons::load(&ShellIconKey::VirtualFile(".txt".to_owned())).unwrap();
        assert_eq!(document.len(), 20 * 20 * 4);
    }
}
