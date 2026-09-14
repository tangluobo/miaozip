#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod archive;
mod integration;
mod optical;
mod system_open;
mod tools;

use eframe::egui;
use std::sync::{Arc, OnceLock};

pub(crate) fn app_icon() -> Arc<egui::IconData> {
    static ICON: OnceLock<Arc<egui::IconData>> = OnceLock::new();
    Arc::clone(ICON.get_or_init(|| {
        Arc::new(egui::IconData {
            rgba: include_bytes!(concat!(env!("OUT_DIR"), "/miaozip-icon.rgba")).to_vec(),
            width: 256,
            height: 256,
        })
    }))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let launch_action = match integration::LaunchAction::from_env() {
        integration::LaunchAction::RegisterIntegration => {
            integration::register_default_candidate()?;
            integration::register_context_menu()?;
            return Ok(());
        }
        integration::LaunchAction::SetDefaultArchives => {
            let remaining = integration::set_default_associations()?;
            if !remaining.is_empty() {
                return Err(format!(
                    "以下格式需要在 Windows 默认应用设置中手动选择妙压：{}",
                    remaining.join("、")
                )
                .into());
            }
            return Ok(());
        }
        integration::LaunchAction::RemoveContextMenu => {
            integration::unregister_context_menu()?;
            return Ok(());
        }
        integration::LaunchAction::AddContext(paths) => {
            match integration::collect_context_selection(paths) {
                Some(paths) => integration::LaunchAction::AddContext(paths),
                None => return Ok(()),
            }
        }
        action => action,
    };
    let context_add = matches!(&launch_action, integration::LaunchAction::AddContext(_));
    let window_icon = app_icon();
    let options = eframe::NativeOptions {
        viewport: if context_add {
            egui::ViewportBuilder::default()
                .with_title("压缩文件 - 妙压")
                .with_icon(window_icon.clone())
                .with_inner_size([630.0, 345.0])
                .with_min_inner_size([630.0, 345.0])
                .with_resizable(false)
        } else {
            egui::ViewportBuilder::default()
                .with_title("此电脑 - 妙压")
                .with_icon(window_icon)
                .with_decorations(false)
                .with_inner_size([1200.0, 753.0])
                .with_min_inner_size([900.0, 560.0])
        },
        centered: true,
        persist_window: false,
        ..Default::default()
    };

    eframe::run_native(
        "MiaoZip",
        options,
        Box::new(move |creation_context| {
            Ok(Box::new(app::MiaoZipApp::new(
                creation_context,
                launch_action,
            )))
        }),
    )?;
    Ok(())
}
