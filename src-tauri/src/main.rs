#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod db;
mod elevation;
mod models;
mod paths;
mod robocopy;
mod runner;
mod scheduler;
mod secrets;
mod win;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WindowEvent};
use tauri_plugin_notification::NotificationExt;

fn arg_after(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Modos sem janela
    if let Some(id) = arg_after(&args, "--run") {
        let trigger = arg_after(&args, "--trigger");
        std::process::exit(runner::run_backup(&id, trigger.as_deref()));
    }
    if let Some(f) = arg_after(&args, "--restore") {
        std::process::exit(runner::run_restore(&f));
    }
    if let Some(f) = arg_after(&args, "--admin-op") {
        std::process::exit(elevation::admin_main(&f));
    }

    let start_minimized = args.iter().any(|a| a == "--minimized");
    run_gui(start_minimized);
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn notify_loop(app: AppHandle) {
    std::thread::spawn(move || {
        let mut first = true;
        loop {
            if let Ok(c) = db::open() {
                let _ = db::fix_stale_runs(&c);
                if let Ok(runs) = db::take_unnotified(&c) {
                    let enabled = db::get_setting(&c, "notifications").ok().flatten().as_deref() != Some("0");
                    if !first && enabled {
                        for r in runs {
                            let (title, icon) = match r.status.as_str() {
                                "success" => ("Backup concluído", "✅"),
                                "warning" => ("Backup concluído com avisos", "⚠️"),
                                "canceled" => ("Backup interrompido", "⏹️"),
                                _ => ("O backup FALHOU", "❌"),
                            };
                            let _ = app
                                .notification()
                                .builder()
                                .title(format!("{icon} {title}: {}", r.routine_name))
                                .body(r.message.clone().unwrap_or_default())
                                .show();
                        }
                    }
                }
            }
            first = false;
            std::thread::sleep(std::time::Duration::from_secs(4));
        }
    });
}

fn run_gui(start_minimized: bool) {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .manage(commands::AppState::default())
        .setup(move |app| {
            let open = MenuItem::with_id(app, "open", "Abrir RoboBackup", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Sair (os backups agendados continuam funcionando)", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &quit])?;
            let mut tray = TrayIconBuilder::with_id("main-tray")
                .tooltip("RoboBackup - Cópias de segurança")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, ev| match ev.id().as_ref() {
                    "open" => show_main(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, ev| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                        show_main(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            if !start_minimized {
                show_main(app.handle());
            }
            notify_loop(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Fechar a janela só esconde: o programa fica na bandeja.
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::setup_computer,
            commands::repair_tasks,
            commands::list_routines,
            commands::get_routine,
            commands::check_tasks,
            commands::save_routine,
            commands::set_routine_enabled,
            commands::delete_routine,
            commands::run_now,
            commands::cancel_run,
            commands::list_runs,
            commands::active_runs,
            commands::get_run,
            commands::read_log,
            commands::test_destination,
            commands::list_drives,
            commands::known_folders,
            commands::list_versions,
            commands::backup_location,
            commands::open_path,
            commands::start_restore,
            commands::get_settings,
            commands::save_settings,
            commands::unlock,
            commands::lock,
            commands::set_pin,
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o RoboBackup");
}
