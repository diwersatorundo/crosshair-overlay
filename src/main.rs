#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod autostart;
#[cfg(windows)]
mod logging;
#[cfg(windows)]
mod overlay;
#[cfg(windows)]
mod shared;

#[cfg(windows)]
fn main() {
    use std::sync::Arc;

    overlay::enable_dpi_awareness();
    logging::install_panic_hook();

    let _instance_mutex = unsafe {
        use windows::core::w;
        use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS};
        use windows::Win32::System::Threading::CreateMutexW;

        match CreateMutexW(None, true, w!("Local\\CrosshairOverlaySingleInstance")) {
            Ok(handle) => {
                if GetLastError() == ERROR_ALREADY_EXISTS {
                    let _ = CloseHandle(handle);
                    show_message(
                        "Crosshair Overlay уже запущен.\nПроверьте панель задач.",
                        false,
                    );
                    return;
                }
                Some(handle)
            }
            Err(err) => {
                logging::log(
                    "WARN",
                    format!("Не удалось создать mutex единственного экземпляра: {err}"),
                );
                None
            }
        }
    };

    let (config, load_warning) = crosshair_core::config::load();
    logging::log(
        "INFO",
        format!(
            "Starting Crosshair Overlay {} (config: {}, log: {})",
            env!("CARGO_PKG_VERSION"),
            crosshair_core::config::config_path().display(),
            logging::log_path().display()
        ),
    );
    if let Some(warning) = &load_warning {
        logging::log("WARN", warning);
    }

    let shared = shared::Shared::new(config.clone());
    overlay::spawn(shared.clone());

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Crosshair Overlay")
            .with_inner_size([1080.0, 720.0])
            .with_min_inner_size([920.0, 620.0])
            .with_icon(Arc::new(app::app_icon())),
        centered: true,
        ..Default::default()
    };

    let result = eframe::run_native(
        "Crosshair Overlay",
        options,
        Box::new(move |cc| {
            Ok(Box::new(app::CrosshairApp::new(
                cc,
                shared,
                config,
                load_warning,
            )))
        }),
    );

    if let Err(err) = result {
        let message = format!(
            "Не удалось запустить окно настроек:\n{err}\n\nПодробности в {}",
            logging::log_path().display()
        );
        logging::log("ERROR", &message);
        show_message(&message, true);
    }
}

#[cfg(windows)]
fn show_message(message: &str, error: bool) {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, MB_ICONERROR, MB_ICONINFORMATION, MB_OK,
    };

    let text: Vec<u16> = message.encode_utf16().chain(std::iter::once(0)).collect();
    let caption: Vec<u16> = "Crosshair Overlay"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let icon = if error {
        MB_ICONERROR
    } else {
        MB_ICONINFORMATION
    };
    unsafe {
        MessageBoxW(
            None,
            PCWSTR(text.as_ptr()),
            PCWSTR(caption.as_ptr()),
            MB_OK | icon,
        );
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Crosshair Overlay is a Windows-only application.");
}
