use crate::state::Manager;
use std::{process::Command, sync::atomic::Ordering, time::Duration};
use tauri::{AppHandle, Emitter, State};

const SHUTDOWN_DELAY_SECONDS: u64 = 30;

#[tauri::command]
pub(crate) fn get_shutdown_when_done(manager: State<'_, Manager>) -> bool {
    manager.shutdown_when_done.load(Ordering::Relaxed)
}

#[tauri::command]
pub(crate) fn set_shutdown_when_done(manager: State<'_, Manager>, enabled: bool) {
    manager.shutdown_when_done.store(enabled, Ordering::Relaxed);
    manager.shutdown_generation.fetch_add(1, Ordering::Relaxed);
    manager.shutdown_scheduled.store(false, Ordering::Relaxed);
}

pub(crate) fn cancel_pending_shutdown(manager: &Manager) {
    manager.shutdown_generation.fetch_add(1, Ordering::Relaxed);
    manager.shutdown_scheduled.store(false, Ordering::Relaxed);
}

pub(crate) async fn schedule_shutdown_if_ready(app: &AppHandle, manager: &Manager) {
    if !manager.shutdown_when_done.load(Ordering::Relaxed) {
        return;
    }

    let all_completed = {
        let tasks = manager.tasks.read().await;
        !tasks.is_empty() && tasks.values().all(|task| task.info.status == "completed")
    };
    if !all_completed
        || manager
            .shutdown_scheduled
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
            .is_err()
    {
        return;
    }

    let generation = manager.shutdown_generation.load(Ordering::Relaxed);
    let app = app.clone();
    let manager = manager.clone();
    tauri::async_runtime::spawn(async move {
        let _ = app.emit("shutdown-scheduled", SHUTDOWN_DELAY_SECONDS);
        tokio::time::sleep(Duration::from_secs(SHUTDOWN_DELAY_SECONDS)).await;

        let still_ready = manager.shutdown_when_done.load(Ordering::Relaxed)
            && manager.shutdown_generation.load(Ordering::Relaxed) == generation
            && {
                let tasks = manager.tasks.read().await;
                !tasks.is_empty() && tasks.values().all(|task| task.info.status == "completed")
            };
        if !still_ready {
            manager.shutdown_scheduled.store(false, Ordering::Relaxed);
            return;
        }

        if let Err(error) = request_system_shutdown() {
            manager.shutdown_scheduled.store(false, Ordering::Relaxed);
            let _ = app.emit("shutdown-error", error);
        }
    });
}

fn request_system_shutdown() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;

        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        Command::new("shutdown")
            .args(["/s", "/t", "0"])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|error| format!("无法关闭 Windows：{error}"))?;
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("osascript")
            .args(["-e", "tell application \"System Events\" to shut down"])
            .spawn()
            .map_err(|error| format!("无法关闭 macOS：{error}"))?;
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("systemctl")
            .arg("poweroff")
            .spawn()
            .map_err(|error| format!("无法关闭计算机：{error}"))?;
    }

    Ok(())
}
