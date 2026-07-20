// SPDX-FileCopyrightText: Copyright (c) 2026 owu <wqh@live.com>
// SPDX-License-Identifier: GPL-3.0-only

use std::sync::Arc;
use tokio::sync::Mutex;
use slint::ComponentHandle;
use crate::{AppWindow, AppState, UsbDevice, AppI18n};
use tracing::{info, error};
use std::future::Future;
use serde_json::json;
use crate::utils::system::copy_to_clipboard;

// Convert backend USB device model to Slint model
fn to_slint_device(dev: &crate::usb::UsbDeviceModel, usb_config: &crate::config::models::UsbConfigFile) -> UsbDevice {
    let full_instance_id = dev.instance_id.clone().unwrap_or_default();
    let sn_part = full_instance_id.split('\\').last().unwrap_or("").to_string();
    let is_real_sn = !sn_part.contains('&') && !sn_part.is_empty();
    let persisted_guid = dev.persisted_guid.clone().unwrap_or_default();

    // Support state inference for versions/commands that don't provide a 'State' field in JSON
    let mut status = if dev.state.contains("Attached") {
        "Attached"
    } else if dev.state.contains("Shared") {
        "Shared"
    } else {
        "NotShared"
    };

    if status == "NotShared" {
        if dev.stub_instance_id.is_some() || dev.client_ip_address.is_some() {
            status = "Attached";
        } else if dev.persisted_guid.is_some() {
            status = "Shared";
        }
    }

    // Support extracting VID/PID from InstanceId if they are missing (common in some JSON versions)
    let mut vid = dev.vid.clone();
    let mut pid = dev.pid.clone();
    if (vid.is_none() || pid.is_none()) && full_instance_id.contains("VID_") {
        if let Some(v_start) = full_instance_id.find("VID_") {
            if full_instance_id.len() >= v_start + 8 {
                vid = Some(full_instance_id[v_start + 4..v_start + 8].to_lowercase());
            }
        }
        if let Some(p_start) = full_instance_id.find("PID_") {
            if full_instance_id.len() >= p_start + 8 {
                pid = Some(full_instance_id[p_start + 4..p_start + 8].to_lowercase());
            }
        }
    }

    let vid_pid = format!("{}:{}", vid.as_deref().unwrap_or(""), pid.as_deref().unwrap_or(""));

    // Determine if boot-attach is enabled in usb.toml
    let is_boot = usb_config.usb.get(&vid_pid)
        .map(|d| d.boot_attach)
        .unwrap_or(false);

    // boot-attach reflects the config value regardless of device bind/attach status
    let is_boot_active = is_boot;

    // Determine if auto-attach is enabled in usb.toml
    let is_auto_attach = usb_config.usb.get(&vid_pid)
        .map(|d| d.auto_attach)
        .unwrap_or(false);

    // Check if this device has force-bind enabled
    let is_force_bind = usb_config.usb.get(&vid_pid)
        .map(|d| d.force_bind)
        .unwrap_or(false);

    UsbDevice {
        bus_id: dev.bus_id.clone().unwrap_or_default().into(),
        description: {
            let desc = dev.description.clone().unwrap_or_default();
            let chars: Vec<char> = desc.chars().collect();
            if chars.len() > 80 {
                format!("{}...", chars[..80].iter().collect::<String>()).into()
            } else {
                desc.into()
            }
        },
        instance_id: dev.instance_id.clone().unwrap_or_default().into(),
        sn: sn_part.into(),
        is_real_sn,
        persisted_id: persisted_guid.into(),
        client_ip: dev.client_ip_address.clone().unwrap_or_default().into(),
        stub_id: dev.stub_instance_id.clone().unwrap_or_default().into(),
        is_forced: dev.is_forced,
        vid_pid: vid_pid.into(),
        boot_attach: is_boot_active,
        auto_attach: is_auto_attach,
        status: status.into(),
        is_attached: status == "Attached",
        attached_distro: slint::SharedString::default(),
        force_bind: is_force_bind,
    }
}

// Helper: run an async USB operation on a background thread, then update UI.
// This prevents the UI thread from freezing during external command execution.
fn spawn_usb_task<F, Fut>(ah: slint::Weak<AppWindow>, task_fn: F)
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<(), String>> + Send + 'static,
{
    tokio::spawn(async move {
        // Run the async task
        let result = task_fn().await;
        
        let ah_inner = ah.clone();
        let _ = slint::invoke_from_event_loop(move || {
            match result {
                Ok(()) => {
                    if let Some(app) = ah_inner.upgrade() {
                        app.invoke_refresh_usb(true);
                    }
                }
                Err(e) => {
                    // Check for cancellation:
                    // 1. Windows HRESULT 0x800704C7 (The operation was canceled by the user)
                    // 2. Generic "cancel" string from underlying tools/OS
                    let is_canceled = e.contains("0x800704C7") || e.to_lowercase().contains("cancel");

                    if is_canceled {
                        info!("USB operation canceled by user: {}", e);
                        if let Some(app) = ah_inner.upgrade() {
                            app.set_usb_loading(false);
                        }
                    } else if e == "no_wsl2_running" {
                        // Special case: no WSL 2 distro is running.
                        // We show a message dialog instead of turning the whole view into an error state.
                        if let Some(app) = ah_inner.upgrade() {
                            app.set_usb_loading(false);
                            // Set common message dialog properties
                            let msg = crate::ui::data::get_i18n_text("usb.no_wsl2_running");
                            app.set_current_message(msg.into());
                            app.set_show_message_dialog(true);
                        }
                    } else if e.starts_with("usb_attach_failed:") {
                        // usbipd attach command failed with output containing "error".
                        // Show a message dialog with the command as a clickable link
                        // that the user can click to copy and run in an administrator PowerShell.
                        error!("USB attach failed with error output: {}", e);
                        if let Some(app) = ah_inner.upgrade() {
                            app.set_usb_loading(false);
                            // Parse the error to extract command text
                            let body = e.trim_start_matches("usb_attach_failed:");
                            let cmd_text = if let Some(pos) = body.find("\n\n") {
                                body[..pos].trim_start_matches("COMMAND:").to_string()
                            } else {
                                body.to_string()
                            };

                            let i18n = app.global::<crate::AppI18n>();
                            let hint = i18n.invoke_t("usb.attach_failed_hint".into(), slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(vec![slint::SharedString::default()]))));

                            app.set_current_message(hint.into());
                            app.set_current_message_link(cmd_text.into());
                            app.set_current_message_url("clipboard".into());
                            app.set_show_message_dialog(true);
                        }
                    } else {
                        error!("USB operation failed: {}", e);
                        if let Some(app) = ah_inner.upgrade() {
                            app.set_usb_loading(false);
                            // Try to translate the error if it looks like a key, otherwise show raw
                            let translated = if e.contains('.') || e == "not_found" {
                                crate::ui::data::get_i18n_text(&format!("usb.{}", e))
                            } else {
                                e.clone()
                            };
                            app.set_usb_error(translated.into());
                        }
                    }
                }
            }
        });
    });
}



pub fn setup(app: &AppWindow, app_handle: slint::Weak<AppWindow>, app_state: Arc<Mutex<AppState>>) {
    // Refresh USB device list — runs usbipd on background thread
    let ah = app_handle.clone();
    let as_ptr = app_state.clone();
    app.on_refresh_usb(move |is_manual| {
        let reason = if is_manual { "manual trigger" } else { "periodic trigger" };
        let is_visible = if let Some(app) = ah.upgrade() {
            app.get_is_window_visible()
        } else {
            true
        };
        
        if !crate::ui::data::should_refresh_usb(reason, is_visible) {
            return;
        }
        let ah = ah.clone();
        let as_ptr = as_ptr.clone();
        info!("USB refresh requested");

        // 1) Immediately show loading state (non-blocking, UI thread)
        if let Some(app) = ah.upgrade() {
            app.set_usb_loading(true);
            app.set_usb_error(slint::SharedString::default());
        }

        // 2) Run blocking command on tokio thread pool
        let ah_bg = ah.clone();
        tokio::spawn(async move {
            // Get current USB config from usb.toml
            let usb_config = {
                let state = as_ptr.lock().await;
                state.config_manager.get_usb_config()
            };
            // First check version
            let version_res = crate::usb::UsbManager::get_version().await;
            
            let (version, is_outdated) = match version_res {
                Ok(v) => {
                    // Check if version is < 4.0.0
                    let is_old = if let Some(first_char) = v.chars().next() {
                         first_char < '4' && first_char.is_ascii_digit()
                    } else {
                         false
                    };
                    (v, is_old)
                }
                Err(_) => (String::new(), false),
            };

            // If we couldn't even get the version, it's probably not installed
            if version.is_empty() {
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = ah_bg.upgrade() {
                        app.set_usb_loading(false);
                        app.set_usb_error("not_found".into());
                    }
                });
                return;
            }

            // Version is OK, or at least we found the tool
            let result = crate::usb::UsbManager::list_devices().await;

            // 3) Return to UI thread to update state
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(app) = ah_bg.upgrade() {
                    app.set_usbipd_version(version.into());
                    app.set_is_usbipd_outdated(is_outdated);
                    app.set_usb_loading(false);
                    
                    match result {
                        Ok(mut devices) => {
                            // Sort devices by BusId naturally (e.g., 1-1 before 1-10)
                            devices.sort_by(|a, b| {
                                let a_bus = a.bus_id.as_deref().unwrap_or("");
                                let b_bus = b.bus_id.as_deref().unwrap_or("");
                                
                                let a_parts: Vec<&str> = a_bus.split(|c| c == '-' || c == '.').collect();
                                let b_parts: Vec<&str> = b_bus.split(|c| c == '-' || c == '.').collect();
                                
                                for (p1, p2) in a_parts.iter().zip(b_parts.iter()) {
                                    let n1 = p1.parse::<u32>();
                                    let n2 = p2.parse::<u32>();
                                    
                                    match (n1, n2) {
                                        (Ok(num1), Ok(num2)) => {
                                            if num1 != num2 { return num1.cmp(&num2); }
                                        }
                                        _ => {
                                            if p1 != p2 { return p1.cmp(p2); }
                                        }
                                    }
                                }
                                a_parts.len().cmp(&b_parts.len())
                            });

                            let slint_devices: Vec<UsbDevice> = devices.iter()
                                .filter(|d| d.bus_id.is_some())
                                .map(|d| to_slint_device(d, &usb_config))
                                .collect();
                            let model = std::rc::Rc::new(slint::VecModel::from(slint_devices));
                            app.set_usb_devices(model.into());
                            info!("USB device list refreshed: {} devices found (sorted by BusId)", devices.len());
                        }
                        Err(e) => {
                            error!("Failed to list USB devices: {}", e);
                            if e == "cmd_not_found" {
                                app.set_usb_error("not_found".into());
                            } else {
                                app.set_usb_error(e.into());
                            }
                        }
                    }
                }
            });
        });
    });

    // Bind
    let ah = app_handle.clone();
    let as_ptr = app_state.clone();
    app.on_usb_bind(move |bus_id, vid_pid| {
        let bus_id = bus_id.to_string();
        let vid_pid = vid_pid.to_string();
        let as_ptr = as_ptr.clone();
        let ah = ah.clone();
        info!("USB bind requested for {} (vid_pid={})", bus_id, vid_pid);
        if let Some(app) = ah.upgrade() { app.set_usb_loading(true); }
        spawn_usb_task(ah.clone(), move || async move {
            let force = {
                let state = as_ptr.lock().await;
                state.config_manager.is_force_bind(&vid_pid)
            };
            crate::usb::UsbManager::bind(&bus_id, force).await
        });
    });

    // Unbind
    let ah = app_handle.clone();
    app.on_usb_unbind(move |bus_id| {
        let bus_id = bus_id.to_string();
        info!("USB unbind requested for {}", bus_id);
        if let Some(app) = ah.upgrade() { app.set_usb_loading(true); }
        spawn_usb_task(ah.clone(), move || async move {
            crate::usb::UsbManager::unbind(&bus_id).await
        });
    });

    // Attach
    let ah = app_handle.clone();
    let as_ptr = app_state.clone();
    app.on_usb_attach(move |bus_id, vid_pid| {
        let bus_id = bus_id.to_string();
        let vid_pid = vid_pid.to_string();
        let as_ptr = as_ptr.clone();
        let ah = ah.clone();
        info!("USB attach requested for {} (vid_pid={})", bus_id, vid_pid);
        if let Some(app) = ah.upgrade() { app.set_usb_loading(true); }
        spawn_usb_task(ah.clone(), move || async move {
            let (force, auto_attach, is_outdated) = {
                let state = as_ptr.lock().await;
                let force = state.config_manager.is_force_bind(&vid_pid);
                let auto_attach = state.config_manager.get_usb_config().usb.get(&vid_pid)
                    .map(|d| d.auto_attach)
                    .unwrap_or(false);
                let is_outdated = if let Some(app) = ah.upgrade() {
                    app.get_is_usbipd_outdated()
                } else {
                    false
                };
                (force, auto_attach, is_outdated)
            };
            crate::usb::UsbManager::attach(&bus_id, "", force, auto_attach, is_outdated).await
        });
    });

    // Save device settings (force-bind + boot-attach + auto_attach toggles)
    let ah = app_handle.clone();
    let as_ptr = app_state.clone();
    app.on_usb_save_device_settings(move |bus_id, vid_pid, force_bind, boot_attach, auto_attach| {
        let bus_id = bus_id.to_string();
        let vid_pid = vid_pid.to_string();
        let as_ptr = as_ptr.clone();
        let ah = ah.clone();
        info!("USB save device settings: bus_id={}, vid_pid={}, force_bind={}, boot_attach={}, auto_attach={}", bus_id, vid_pid, force_bind, boot_attach, auto_attach);
        tokio::spawn(async move {
            let result = {
                let mut state = as_ptr.lock().await;
                // Toggle force_bind if needed
                let current_fb = state.config_manager.is_force_bind(&vid_pid);
                if current_fb != force_bind {
                    if let Err(e) = state.config_manager.toggle_force_bind(&vid_pid) {
                        error!("Failed to toggle force-bind for {}: {}", vid_pid, e);
                    }
                }
                // Set boot_attach explicitly (with bus_id for scheduler)
                if let Err(e) = state.config_manager.set_usb_boot_attach(&vid_pid, &bus_id, boot_attach) {
                    error!("Failed to set boot-attach for {}: {}", vid_pid, e);
                }
                // Set auto_attach explicitly
                state.config_manager.set_usb_auto_attach(&vid_pid, auto_attach)
            };
            match result {
                Ok(_) => {
                    info!("Device settings saved for {}", vid_pid);

                    // If boot_attach is enabled, ensure the Windows scheduled task is registered
                    if boot_attach {
                        if !crate::network::scheduler::check_task_exists() {
                            info!("Boot-attach enabled but scheduled task not found, registering with elevation...");
                            match crate::network::scheduler::register_task_with_elevation() {
                                Ok(_) => {
                                    info!("Successfully registered boot-attach scheduled task via elevation.");
                                    let ah_toast = ah.clone();
                                    let _ = slint::invoke_from_event_loop(move || {
                                        if let Some(app) = ah_toast.upgrade() {
                                            app.set_task_status_text("Boot-attach task scheduled".into());
                                            app.set_task_status_visible(true);
                                            let ah_hide = ah_toast.clone();
                                            slint::Timer::single_shot(std::time::Duration::from_secs(3), move || {
                                                if let Some(app_h) = ah_hide.upgrade() {
                                                    app_h.set_task_status_visible(false);
                                                }
                                            });
                                        }
                                    });
                                }
                                Err(e) => {
                                    error!("Failed to register boot-attach scheduled task: {}", e);
                                    let ah_toast = ah.clone();
                                    let _ = slint::invoke_from_event_loop(move || {
                                        if let Some(app) = ah_toast.upgrade() {
                                            let err_msg = if e.contains("denied") || e.contains("Access") {
                                                "Boot-attach: UAC permission denied".to_string()
                                            } else {
                                                format!("Boot-attach: failed to schedule - {}", e)
                                            };
                                            app.set_task_status_text(err_msg.into());
                                            app.set_task_status_visible(true);
                                            let ah_hide = ah_toast.clone();
                                            slint::Timer::single_shot(std::time::Duration::from_secs(5), move || {
                                                if let Some(app_h) = ah_hide.upgrade() {
                                                    app_h.set_task_status_visible(false);
                                                }
                                            });
                                        }
                                    });
                                }
                            }
                        } else {
                            info!("Boot-attach scheduled task already exists, skipping registration.");
                        }
                    }

                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(app) = ah.upgrade() {
                            app.invoke_refresh_usb(true);
                        }
                    });
                }
                Err(e) => error!("Failed to save device settings: {}", e),
            }
        });
    });

    // Detach
    let ah = app_handle.clone();
    app.on_usb_detach(move |bus_id| {
        let bus_id = bus_id.to_string();
        info!("USB detach requested for {}", bus_id);
        if let Some(app) = ah.upgrade() { app.set_usb_loading(true); }
        spawn_usb_task(ah.clone(), move || async move {
            crate::usb::UsbManager::detach(&bus_id).await
        });
    });

    // Copy Info
    let ah = app_handle.clone();
    app.on_usb_copy_info(move |device| {
        let json_data = json!({
            "BusId": device.bus_id.as_str(),
            "ClientIPAddress": if device.client_ip.is_empty() { serde_json::Value::Null } else { json!(device.client_ip.as_str()) },
            "Description": device.description.as_str(),
            "InstanceId": device.instance_id.as_str(),
            "IsForced": device.is_forced,
            "PersistedGuid": if device.persisted_id.is_empty() { serde_json::Value::Null } else { json!(device.persisted_id.as_str()) },
            "StubInstanceId": if device.stub_id.is_empty() { serde_json::Value::Null } else { json!(device.stub_id.as_str()) },
        });

        let json_str = serde_json::to_string_pretty(&json_data).unwrap_or_default();
        match copy_to_clipboard(&json_str) {
            Ok(_) => {
                if let Some(app) = ah.upgrade() {
                    let msg = app.global::<AppI18n>().invoke_t("usb.copy_success".into(), slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(vec![device.bus_id.clone()]))));
                    app.set_task_status_text(msg);
                    app.set_task_status_visible(true);
                    
                    // Auto-hide toast after 3 seconds
                    let ah_inner = ah.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(app) = ah_inner.upgrade() {
                                app.set_task_status_visible(false);
                            }
                        });
                    });
                }
            }
            Err(e) => {
                error!("Failed to copy USB info to clipboard: {}", e);
            }
        }
    });

    // Device Info
    let ah = app_handle.clone();
    app.on_usb_device_info(move |device| {
        let ah_inner = ah.clone();
        let instance_id = device.instance_id.to_string();
        let bus_id = device.bus_id.to_string();
        // vid_pid and serial_number come from the device model, not SetupAPI
        let vid_pid = device.vid_pid.to_string();
        // Serial number is the last path component of instance_id (e.g. USB\VID_xxx\SERIAL)
        let serial_number = instance_id.split('\\').last().unwrap_or("").to_string();
        // Only treat it as a real serial number if it doesn't look like a hardware-generated ID
        let serial_number = if serial_number.contains('&') { String::new() } else { serial_number };
        
        // Check if the device is bound
        let status = device.status.to_string();
        if status != "NotShared" {
            if let Some(app) = ah_inner.upgrade() {
                let msg = crate::ui::data::get_i18n_text("usb.info_unbind_hint");
                app.set_current_message(msg.into());
                app.set_show_message_dialog(true);
            }
            return;
        }
        
        tokio::spawn(async move {
            match crate::usb::UsbManager::get_device_detail(&instance_id).await {
                Ok(Some(detail)) => {
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(app) = ah_inner.upgrade() {
                            let ui_detail = crate::UsbDeviceDetailInfo {
                                bus_id: bus_id.clone().into(),
                                vid_pid: vid_pid.clone().into(),
                                serial_number: serial_number.clone().into(),
                                manufacturer: detail.manufacturer.clone().unwrap_or_default().into(),
                                product_name: detail.product_name.clone().unwrap_or_default().into(),
                                class_guid: detail.class_guid.clone().unwrap_or_default().into(),
                                hardware_ids: detail.hardware_ids.clone().join("\n").into(),
                                instance_id: detail.instance_id.clone().unwrap_or_default().into(),
                            };
                            
                            tracing::debug!(
                                "USB Device Info: bus_id='{}', vid_pid='{}', serial='{}', manufacturer='{}', product='{}', class='{}', instance='{}', hardware_ids={:?}",
                                bus_id, vid_pid, serial_number, 
                                detail.manufacturer.unwrap_or_default(), 
                                detail.product_name.unwrap_or_default(), 
                                detail.class_guid.unwrap_or_default(), 
                                detail.instance_id.unwrap_or_default(),
                                detail.hardware_ids
                            );
                            
                            app.set_usb_device_info_data(ui_detail);
                            app.set_show_usb_device_info(true);
                        }
                    });
                }
                Ok(None) => {
                    error!("Device detail not found for instance_id: {}", instance_id);
                }
                Err(e) => {
                    error!("Failed to get device detail for {}: {}", instance_id, e);
                }
            }
        });
    });
}
