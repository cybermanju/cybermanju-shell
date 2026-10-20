//! GPUi Shell - A Wayland status bar built with GPUI.
//!
//! Subsequent ordinary invocations open the existing launcher's input. Cosmetic
//! imports require the primary instance; --watch-appearance-profile additionally
//! follows an explicitly supplied file, never browser storage or credentials.

use crate::ipc::IpcSubscriber;
use assets::Assets;
use gpui_platform::application;
use tracing_subscriber::EnvFilter;

mod args;
mod bar;
pub mod config;
pub mod control_center;
pub mod dock;
mod frame_metrics;
mod frame_telemetry;
pub mod icons;
mod ipc;
pub mod keybinds;
pub mod launcher;
pub mod notification;
pub mod osd;
mod panel;
// Parser/lifecycle prototype only; not a live protocol subscription.
#[cfg(test)]
mod presentation_contract;
pub mod state;

#[tokio::main]
async fn main() {
    #[cfg(not(target_os = "linux"))]
    compile_error!("This application requires a Linux system with Wayland.");

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    let args = args::Args::parse();
    let appearance_path = match args
        .appearance_profile
        .as_deref()
        .map(std::path::absolute)
        .transpose()
    {
        Ok(path) => path,
        Err(error) => {
            eprintln!("Cannot resolve explicit appearance path: {error:#}");
            std::process::exit(1);
        }
    };
    let mut appearance = match appearance_path
        .as_deref()
        .map(config::appearance::AppearanceProfile::load)
        .transpose()
    {
        Ok(profile) => profile,
        Err(error) => {
            eprintln!("Appearance profile rejected: {error:#}");
            std::process::exit(1);
        }
    };
    if let Some(profile) = &appearance
        && let Err(error) = profile.project_theme(&ui::Theme::default())
    {
        eprintln!("Appearance token projection failed: {error:#}");
        std::process::exit(1);
    }

    // Renderer-free, no watcher, writes or secondary-instance commands.
    if args.validate {
        if let Some(profile) = &appearance {
            println!("{}", profile.report());
        }
        if args.watch_appearance_profile {
            println!("Live-file watch requested; validation only, no watcher or UI was started.");
        }
        std::process::exit(if config::validate() { 0 } else { 1 });
    }

    let Some(ipc) = IpcSubscriber::init(&args) else {
        if appearance.is_some() {
            eprintln!(
                "Appearance profile was NOT applied; import requires an available primary shell instance."
            );
            std::process::exit(1);
        }
        return;
    };
    if std::env::var_os("WAYLAND_DISPLAY").is_none_or(|display| display.is_empty()) {
        eprintln!(
            "GPUi Shell requires WAYLAND_DISPLAY from a running Wayland session (Hyprland, Niri or Mango)."
        );
        drop(ipc);
        std::process::exit(1);
    }

    // Register before rereading: saves between initial validation and watch setup
    // are covered by the reread, and later changes remain queued for the UI task.
    let appearance_watch = if args.watch_appearance_profile {
        let path = appearance_path.expect("argument parser requires an explicit profile");
        let rx = match services::FileWatcher::try_watch(path.clone()) {
            Ok(rx) => rx,
            Err(error) => {
                eprintln!("Cannot watch appearance profile: {error:#}");
                drop(ipc);
                std::process::exit(1);
            }
        };
        appearance = match config::appearance::AppearanceProfile::load(&path) {
            Ok(profile) => Some(profile),
            Err(error) => {
                eprintln!(
                    "Appearance profile changed during watch setup and was rejected: {error:#}"
                );
                drop(ipc);
                std::process::exit(1);
            }
        };
        Some((path, rx))
    } else {
        None
    };

    let services = match state::init_services().await {
        Ok(services) => services,
        Err(error) => {
            tracing::error!("Failed to initialize compositor services: {error:#}");
            eprintln!("GPUi Shell could not connect to a supported compositor: {error:#}");
            drop(ipc);
            std::process::exit(1);
        }
    };
    let app = application().with_assets(Assets {});
    app.run(move |cx| {
        config::Config::init(cx);
        if let Some(profile) = appearance {
            tracing::info!("{}", profile.report());
            if let Err(error) = config::appearance_live::AppearanceOverlay::install(profile, cx) {
                tracing::error!("Appearance token projection failed: {error:#}");
                cx.quit();
                return;
            }
        }
        ui::init(cx);
        state::AppState::init(services, cx);
        keybinds::register(cx);
        bar::init(cx);
        dock::init(cx);
        notification::init(cx);
        osd::init(cx);
        launcher::init(cx);
        config::report_load_errors(cx);
        if let Some((path, rx)) = appearance_watch {
            config::appearance_live::AppearanceOverlay::start_watch(path, rx, cx);
        }
        ipc.start(cx);
    });
}
