use super::*;
use crate::tools::{self, RenameMode};

impl MiaoZipApp {
    pub(super) fn poll_zip_repair_worker(&mut self) {
        let Some(receiver) = self.zip_repair_worker.as_ref() else {
            return;
        };
        match receiver.try_recv() {
            Ok(Ok(result)) => {
                self.zip_repair_worker = None;
                self.zip_repair_result = Some(result);
                self.zip_test_error = None;
                self.refresh_entries();
            }
            Ok(Err(error)) => {
                self.zip_repair_worker = None;
                self.zip_repair_result = None;
                self.zip_test_error = Some(error);
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.zip_repair_worker = None;
                self.zip_test_error = Some("修复任务意外终止".to_owned());
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }

    pub(super) fn poll_zip_test_worker(&mut self) {
        let Some(receiver) = self.zip_test_worker.as_ref() else {
            return;
        };
        match receiver.try_recv() {
            Ok(Ok(result)) => {
                self.zip_test_worker = None;
                self.zip_test_result = Some(result);
                self.zip_test_error = None;
            }
            Ok(Err(error)) => {
                self.zip_test_worker = None;
                self.zip_test_result = None;
                self.zip_test_error = Some(error);
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.zip_test_worker = None;
                self.zip_test_error = Some("测试任务意外终止".to_owned());
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }

    pub(super) fn show_zip_test_tool_dialog(&mut self, context: &egui::Context) {
        if !self.show_zip_test_dialog {
            return;
        }
        let close = show_native_popup(
            context,
            "miaozip_zip_test_tool",
            "ZIP 测试与修复 - 妙压",
            [610.0, 380.0],
            |ui, close| {
                operation_heading(ui, "ZIP 测试与修复", "逐项校验 · 从可读条目重建新包");
                ui.label("压缩包：");
                ui.horizontal(|ui| {
                    let path = self
                        .zip_test_path
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_default();
                    ui.add_sized([480.0, 26.0], egui::Label::new(path).truncate());
                    if ui.button("浏览...").clicked()
                        && let Some(path) = FileDialog::new()
                            .add_filter("ZIP 压缩包", &["zip"])
                            .pick_file()
                    {
                        self.zip_test_path = Some(path);
                        self.zip_test_result = None;
                        self.zip_repair_result = None;
                        self.zip_test_error = None;
                    }
                });
                ui.add_space(8.0);
                if ui
                    .add_enabled(
                        self.zip_test_path.is_some()
                            && self.zip_test_worker.is_none()
                            && self.zip_repair_worker.is_none(),
                        egui::Button::new("开始测试"),
                    )
                    .clicked()
                    && let Some(path) = self.zip_test_path.clone()
                {
                    let (sender, receiver) = mpsc::channel();
                    self.zip_test_worker = Some(receiver);
                    self.zip_test_result = None;
                    self.zip_test_error = None;
                    let repaint = context.clone();
                    thread::spawn(move || {
                        let result =
                            tools::test_zip_archive(&path).map_err(|error| format!("{error:#}"));
                        let _ = sender.send(result);
                        repaint.request_repaint();
                    });
                }
                if ui
                    .add_enabled(
                        self.zip_test_path.is_some()
                            && self.zip_repair_worker.is_none()
                            && self.zip_test_worker.is_none(),
                        egui::Button::new("尝试修复可读条目"),
                    )
                    .clicked()
                    && let Some(path) = self.zip_test_path.clone()
                {
                    let (sender, receiver) = mpsc::channel();
                    self.zip_repair_worker = Some(receiver);
                    self.zip_repair_result = None;
                    self.zip_test_error = None;
                    let repaint = context.clone();
                    thread::spawn(move || {
                        let result =
                            tools::repair_readable_zip(&path).map_err(|error| format!("{error:#}"));
                        let _ = sender.send(result);
                        repaint.request_repaint();
                    });
                }
                if self.zip_test_worker.is_some() {
                    ui.spinner();
                    ui.weak("正在校验 ZIP 条目…");
                }
                if self.zip_repair_worker.is_some() {
                    ui.spinner();
                    ui.weak("正在重建可读条目…");
                }
                if let Some(result) = &self.zip_test_result {
                    ui.add_space(8.0);
                    ui.group(|ui| {
                        ui.label(
                            egui::RichText::new("✓  测试通过")
                                .strong()
                                .color(egui::Color32::from_rgb(42, 146, 88)),
                        );
                        ui.label(format!(
                            "条目：{}  ·  解压后大小：{} 字节",
                            result.entries, result.unpacked_bytes
                        ));
                    });
                }
                if let Some(error) = &self.zip_test_error {
                    ui.colored_label(egui::Color32::DARK_RED, error);
                }
                if let Some(result) = &self.zip_repair_result {
                    ui.group(|ui| {
                        ui.label(egui::RichText::new("已生成恢复包").strong().color(BLUE));
                        ui.label(format!(
                            "保留 {} 项，跳过 {} 项",
                            result.recovered, result.skipped
                        ));
                        ui.label(result.output.display().to_string());
                    });
                }
                ui.add_space(6.0);
                ui.weak(
                    "只支持 ZIP；中央目录不可读、丢失内容或超过大小限制时无法恢复。原包始终保留。",
                );
                if ui.button("关闭").clicked() {
                    *close = true;
                }
            },
        );
        self.show_zip_test_dialog = !close;
    }

    pub(super) fn poll_image_convert_worker(&mut self) {
        let Some(receiver) = self.image_convert_worker.as_ref() else {
            return;
        };
        match receiver.try_recv() {
            Ok(Ok(count)) => {
                self.image_convert_worker = None;
                self.image_convert_feedback = Some(format!("已转换 {count} 张图片，原文件未修改"));
                self.refresh_entries();
            }
            Ok(Err(error)) => {
                self.image_convert_worker = None;
                self.image_convert_feedback = Some(format!("转换失败：{error}"));
                self.refresh_entries();
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.image_convert_worker = None;
                self.image_convert_feedback = Some("图片转换任务意外终止".to_owned());
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }

    pub(super) fn show_image_convert_tool_dialog(&mut self, context: &egui::Context) {
        if !self.show_image_convert_dialog {
            return;
        }
        let close = show_native_popup(
            context,
            "miaozip_image_convert_tool",
            "图片转换 - 妙压",
            [690.0, 500.0],
            |ui, close| {
                operation_heading(ui, "图片转换", "批量输出新格式 · 保留原图");
                ui.label(egui::RichText::new("目标格式").strong());
                ui.horizontal(|ui| {
                    for format in [
                        ImageOutputFormat::Png,
                        ImageOutputFormat::Jpeg,
                        ImageOutputFormat::WebP,
                        ImageOutputFormat::Bmp,
                    ] {
                        ui.radio_value(&mut self.image_output_format, format, format.label());
                    }
                });
                ui.add_space(7.0);
                ui.horizontal(|ui| {
                    if ui.button("添加图片...").clicked()
                        && let Some(paths) = FileDialog::new()
                            .add_filter("图片", &["png", "jpg", "jpeg", "bmp", "gif", "webp"])
                            .pick_files()
                    {
                        for path in paths {
                            if !self.image_convert_items.contains(&path) {
                                self.image_convert_items.push(path);
                            }
                        }
                    }
                    if ui.button("清空列表").clicked() {
                        self.image_convert_items.clear();
                    }
                    ui.weak(format!("{} 张", self.image_convert_items.len()));
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height(235.0)
                    .show(ui, |ui| {
                        for path in &self.image_convert_items {
                            ui.label(path.display().to_string());
                        }
                    });
                if let Some(feedback) = &self.image_convert_feedback {
                    ui.colored_label(BLUE, feedback);
                }
                if self.image_convert_worker.is_some() {
                    ui.spinner();
                    ui.weak("正在转换图片…");
                }
                ui.add_space(6.0);
                ui.weak("输出为“原名.converted.新扩展名”，不覆盖现有文件；GIF 动图只转换首帧。");
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.image_convert_items.is_empty()
                                && self.image_convert_worker.is_none(),
                            egui::Button::new(
                                egui::RichText::new("开始转换").color(egui::Color32::WHITE),
                            )
                            .fill(BLUE),
                        )
                        .clicked()
                    {
                        let paths = self.image_convert_items.clone();
                        let format = self.image_output_format;
                        let (sender, receiver) = mpsc::channel();
                        self.image_convert_worker = Some(receiver);
                        self.image_convert_feedback = None;
                        let repaint = context.clone();
                        thread::spawn(move || {
                            let result = tools::convert_images(&paths, format)
                                .map_err(|error| format!("{error:#}"));
                            let _ = sender.send(result);
                            repaint.request_repaint();
                        });
                    }
                    if ui.button("关闭").clicked() {
                        *close = true;
                    }
                });
            },
        );
        self.show_image_convert_dialog = !close;
    }

    pub(super) fn poll_hash_worker(&mut self) {
        let Some(receiver) = self.hash_worker.as_ref() else {
            return;
        };
        match receiver.try_recv() {
            Ok(Ok(result)) => {
                self.hash_worker = None;
                self.hash_result = Some(result);
                self.hash_error = None;
            }
            Ok(Err(error)) => {
                self.hash_worker = None;
                self.hash_result = None;
                self.hash_error = Some(error);
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.hash_worker = None;
                self.hash_error = Some("校验任务意外终止".to_owned());
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }

    pub(super) fn show_hash_tool_dialog(&mut self, context: &egui::Context) {
        if !self.show_hash_dialog {
            return;
        }
        let close = show_native_popup(
            context,
            "miaozip_hash_tool",
            "MD5 校验 - 妙压",
            [610.0, 370.0],
            |ui, close| {
                operation_heading(ui, "MD5 校验", "计算文件指纹并与已知值对照");
                ui.label("文件：");
                ui.horizontal(|ui| {
                    let path = self
                        .hash_path
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_default();
                    ui.add_sized([480.0, 26.0], egui::Label::new(path).truncate());
                    if ui.button("浏览...").clicked()
                        && let Some(path) = FileDialog::new().pick_file()
                    {
                        self.hash_path = Some(path);
                        self.hash_result = None;
                        self.hash_error = None;
                    }
                });
                ui.add_space(6.0);
                if ui
                    .add_enabled(
                        self.hash_path.is_some() && self.hash_worker.is_none(),
                        egui::Button::new("计算 MD5 / SHA-256"),
                    )
                    .clicked()
                    && let Some(path) = self.hash_path.clone()
                {
                    let (sender, receiver) = mpsc::channel();
                    self.hash_worker = Some(receiver);
                    self.hash_result = None;
                    self.hash_error = None;
                    let repaint = context.clone();
                    thread::spawn(move || {
                        let result = tools::hash_file(&path).map_err(|error| format!("{error:#}"));
                        let _ = sender.send(result);
                        repaint.request_repaint();
                    });
                }
                if self.hash_worker.is_some() {
                    ui.spinner();
                    ui.weak("正在读取文件并计算校验值…");
                }
                if let Some(result) = &self.hash_result {
                    ui.add_space(7.0);
                    ui.group(|ui| {
                        ui.set_min_width(ui.available_width() - 12.0);
                        ui.label(egui::RichText::new("MD5").strong().color(BLUE));
                        ui.monospace(&result.md5);
                        ui.label(egui::RichText::new("SHA-256").strong().color(BLUE));
                        ui.monospace(&result.sha256);
                    });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label("已知校验值：");
                        ui.add_sized(
                            [440.0, 25.0],
                            egui::TextEdit::singleline(&mut self.hash_expected),
                        );
                    });
                    let expected = self.hash_expected.trim().to_ascii_lowercase();
                    if !expected.is_empty() {
                        let valid = expected == result.md5 || expected == result.sha256;
                        ui.colored_label(
                            if valid {
                                egui::Color32::from_rgb(35, 143, 89)
                            } else {
                                egui::Color32::from_rgb(185, 62, 58)
                            },
                            if valid {
                                "校验值一致"
                            } else {
                                "校验值不一致"
                            },
                        );
                    }
                }
                if let Some(error) = &self.hash_error {
                    ui.colored_label(egui::Color32::DARK_RED, error);
                }
                ui.add_space(5.0);
                ui.weak("MD5 仅用于兼容旧校验值；安全敏感场景请比较 SHA-256。");
                if ui.button("关闭").clicked() {
                    *close = true;
                }
            },
        );
        self.show_hash_dialog = !close;
    }

    pub(super) fn show_rename_tool_dialog(&mut self, context: &egui::Context) {
        if !self.show_rename_dialog {
            return;
        }
        let close = show_native_popup(
            context,
            "miaozip_rename_tool",
            "批量文件改名 - 妙压",
            [770.0, 555.0],
            |ui, close| {
                operation_heading(ui, "批量文件改名", "编号、替换、添加前后缀 · 执行前先预览");
                ui.horizontal(|ui| {
                    for (mode, label) in [
                        (RenameMode::Numbered, "编号"),
                        (RenameMode::Replace, "替换"),
                        (RenameMode::Affix, "添加 / 删除"),
                    ] {
                        if ui
                            .selectable_value(&mut self.rename_options.mode, mode, label)
                            .changed()
                        {
                            self.rename_confirm = false;
                        }
                    }
                });
                ui.add_space(6.0);
                match self.rename_options.mode {
                    RenameMode::Numbered => {
                        ui.horizontal(|ui| {
                            ui.label("命名规则：");
                            if ui
                                .add_sized(
                                    [260.0, 25.0],
                                    egui::TextEdit::singleline(&mut self.rename_options.pattern),
                                )
                                .changed()
                            {
                                self.rename_confirm = false;
                            }
                            ui.label("起始编号：");
                            if ui
                                .add(
                                    egui::DragValue::new(&mut self.rename_options.start)
                                        .range(0..=999_999),
                                )
                                .changed()
                            {
                                self.rename_confirm = false;
                            }
                        });
                        ui.weak("规则中的 {n} 会替换为序号，原扩展名保持不变。");
                    }
                    RenameMode::Replace => {
                        ui.horizontal(|ui| {
                            ui.label("查找：");
                            if ui
                                .add_sized(
                                    [260.0, 25.0],
                                    egui::TextEdit::singleline(&mut self.rename_options.find),
                                )
                                .changed()
                            {
                                self.rename_confirm = false;
                            }
                            ui.label("替换为：");
                            if ui
                                .add_sized(
                                    [260.0, 25.0],
                                    egui::TextEdit::singleline(
                                        &mut self.rename_options.replacement,
                                    ),
                                )
                                .changed()
                            {
                                self.rename_confirm = false;
                            }
                        });
                        ui.weak("仅替换文件名主体，不修改扩展名或文件内容。");
                    }
                    RenameMode::Affix => {
                        ui.horizontal(|ui| {
                            ui.label("前缀：");
                            if ui
                                .add_sized(
                                    [260.0, 25.0],
                                    egui::TextEdit::singleline(&mut self.rename_options.prefix),
                                )
                                .changed()
                            {
                                self.rename_confirm = false;
                            }
                            ui.label("后缀：");
                            if ui
                                .add_sized(
                                    [260.0, 25.0],
                                    egui::TextEdit::singleline(&mut self.rename_options.suffix),
                                )
                                .changed()
                            {
                                self.rename_confirm = false;
                            }
                        });
                        ui.weak("删除已有字符请使用“替换”标签并把替换内容留空。");
                    }
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("添加文件...").clicked()
                        && let Some(paths) = FileDialog::new().pick_files()
                    {
                        for path in paths {
                            if !self.rename_items.contains(&path) {
                                self.rename_items.push(path);
                            }
                        }
                        self.rename_confirm = false;
                    }
                    if ui.button("添加文件夹...").clicked()
                        && let Some(path) = FileDialog::new().pick_folder()
                        && !self.rename_items.contains(&path)
                    {
                        self.rename_items.push(path);
                        self.rename_confirm = false;
                    }
                    if ui.button("清空列表").clicked() {
                        self.rename_items.clear();
                        self.rename_confirm = false;
                    }
                    ui.weak(format!("{} 项", self.rename_items.len()));
                });
                ui.separator();
                let plan = tools::preview_rename(&self.rename_items, &self.rename_options);
                ui.label(egui::RichText::new("预览：原名称  →  新名称").strong());
                egui::ScrollArea::vertical()
                    .max_height(235.0)
                    .show(ui, |ui| match &plan {
                        Ok(plan) => {
                            for item in plan {
                                let from = item
                                    .source
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy();
                                let to = item
                                    .target
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy();
                                ui.horizontal(|ui| {
                                    ui.add_sized(
                                        [310.0, 18.0],
                                        egui::Label::new(from.as_ref()).truncate(),
                                    );
                                    ui.label("→");
                                    ui.add_sized(
                                        [310.0, 18.0],
                                        egui::Label::new(to.as_ref()).truncate(),
                                    );
                                });
                            }
                        }
                        Err(error) => {
                            ui.colored_label(
                                egui::Color32::from_rgb(174, 86, 46),
                                error.to_string(),
                            );
                        }
                    });
                if let Some(feedback) = &self.rename_feedback {
                    ui.colored_label(BLUE, feedback);
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if self.rename_confirm {
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("确认执行重命名")
                                        .color(egui::Color32::WHITE),
                                )
                                .fill(BLUE),
                            )
                            .clicked()
                            && let Ok(plan) = &plan
                        {
                            match tools::execute_rename(plan) {
                                Ok(count) => {
                                    self.rename_feedback = Some(format!("已重命名 {count} 项"));
                                    self.rename_items.clear();
                                    self.refresh_entries();
                                }
                                Err(error) => {
                                    self.rename_feedback = Some(format!("重命名失败：{error:#}"))
                                }
                            }
                            self.rename_confirm = false;
                        }
                        if ui.button("返回预览").clicked() {
                            self.rename_confirm = false;
                        }
                    } else if ui
                        .add_enabled(plan.is_ok(), egui::Button::new("开始重命名"))
                        .clicked()
                    {
                        self.rename_feedback = None;
                        self.rename_confirm = true;
                    }
                    if ui.button("关闭").clicked() {
                        *close = true;
                    }
                });
            },
        );
        self.show_rename_dialog = !close;
    }

    pub(super) fn show_text_replace_tool_dialog(&mut self, context: &egui::Context) {
        if !self.show_text_replace_dialog {
            return;
        }
        let close = show_native_popup(
            context,
            "miaozip_text_replace_tool",
            "批量字符替换 - 妙压",
            [750.0, 530.0],
            |ui, close| {
                operation_heading(
                    ui,
                    "批量字符替换",
                    "预览命中次数 · 结果写入新文件，保留原件",
                );
                ui.horizontal(|ui| {
                    ui.label("查找：");
                    if ui
                        .add_sized(
                            [300.0, 25.0],
                            egui::TextEdit::singleline(&mut self.text_find),
                        )
                        .changed()
                    {
                        self.text_replace_preview.clear();
                    }
                    ui.label("替换为：");
                    if ui
                        .add_sized(
                            [300.0, 25.0],
                            egui::TextEdit::singleline(&mut self.text_replacement),
                        )
                        .changed()
                    {
                        self.text_replace_preview.clear();
                    }
                });
                ui.add_space(5.0);
                ui.weak("仅处理不超过 32 MiB 的 UTF-8 普通文本；输出为文件名.replaced.扩展名。");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("添加文件...").clicked()
                        && let Some(paths) = FileDialog::new().pick_files()
                    {
                        for path in paths {
                            if !self.text_replace_items.contains(&path) {
                                self.text_replace_items.push(path);
                            }
                        }
                        self.text_replace_preview.clear();
                    }
                    if ui.button("清空列表").clicked() {
                        self.text_replace_items.clear();
                        self.text_replace_preview.clear();
                    }
                    ui.weak(format!("{} 个文本文件", self.text_replace_items.len()));
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height(250.0)
                    .show(ui, |ui| {
                        if self.text_replace_preview.is_empty() {
                            for path in &self.text_replace_items {
                                ui.label(path.display().to_string());
                            }
                        } else {
                            for item in &self.text_replace_preview {
                                ui.horizontal(|ui| {
                                    ui.add_sized(
                                        [500.0, 18.0],
                                        egui::Label::new(item.source.display().to_string())
                                            .truncate(),
                                    );
                                    ui.label(format!(
                                        "{} 处 → {}",
                                        item.matches,
                                        item.output
                                            .file_name()
                                            .unwrap_or_default()
                                            .to_string_lossy()
                                    ));
                                });
                            }
                        }
                    });
                if let Some(feedback) = &self.text_replace_feedback {
                    ui.colored_label(BLUE, feedback);
                }
                ui.add_space(7.0);
                ui.horizontal(|ui| {
                    if ui.button("生成预览").clicked() {
                        match tools::preview_text_replace(&self.text_replace_items, &self.text_find)
                        {
                            Ok(plan) => {
                                self.text_replace_feedback = Some(format!(
                                    "预览完成，共命中 {} 处",
                                    plan.iter().map(|item| item.matches).sum::<usize>()
                                ));
                                self.text_replace_preview = plan;
                            }
                            Err(error) => {
                                self.text_replace_preview.clear();
                                self.text_replace_feedback = Some(format!("预览失败：{error:#}"));
                            }
                        }
                    }
                    if ui
                        .add_enabled(
                            !self.text_replace_preview.is_empty(),
                            egui::Button::new(
                                egui::RichText::new("写入新文件").color(egui::Color32::WHITE),
                            )
                            .fill(BLUE),
                        )
                        .clicked()
                    {
                        match tools::execute_text_replace(
                            &self.text_replace_preview,
                            &self.text_find,
                            &self.text_replacement,
                        ) {
                            Ok(count) => {
                                self.text_replace_feedback =
                                    Some(format!("已生成 {count} 个新文件，原文件未修改"));
                                self.text_replace_preview.clear();
                                self.refresh_entries();
                            }
                            Err(error) => {
                                self.text_replace_feedback = Some(format!("替换失败：{error:#}"))
                            }
                        }
                    }
                    if ui.button("关闭").clicked() {
                        *close = true;
                    }
                });
            },
        );
        self.show_text_replace_dialog = !close;
    }
}
