// SPDX-FileCopyrightText: Copyright (c) 2026 owu <wqh@live.com>
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsbDeviceDetail {
    pub bus_id: Option<String>,
    pub vid_pid: Option<String>,
    pub instance_id: Option<String>,
    pub serial_number: Option<String>,
    pub manufacturer: Option<String>,
    pub product_name: Option<String>,
    pub class_guid: Option<String>,
    pub hardware_ids: Vec<String>,
}

#[cfg(windows)]
pub use windows_impl::get_device_detail;

#[cfg(not(windows))]
pub async fn get_device_detail(_instance_id: &str) -> Result<Option<UsbDeviceDetail>, String> {
    Err("Not supported on non-Windows platforms".into())
}

#[cfg(windows)]
mod windows_impl {
    use super::UsbDeviceDetail;
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW,
        SetupDiGetDeviceInstanceIdW, SetupDiGetDeviceRegistryPropertyW,
        DIGCF_ALLCLASSES, DIGCF_PRESENT, HDEVINFO, SPDRP_CLASSGUID, SPDRP_DEVICEDESC,
        SPDRP_FRIENDLYNAME, SPDRP_HARDWAREID, SPDRP_MFG, SP_DEVINFO_DATA,
        SETUP_DI_REGISTRY_PROPERTY,
    };
    use scopeguard::defer;

    // Helper to extract a string property from SetupDiGetDeviceRegistryPropertyW
    unsafe fn get_string_property(
        device_info_set: HDEVINFO,
        device_info_data: &mut SP_DEVINFO_DATA,
        property: SETUP_DI_REGISTRY_PROPERTY,
    ) -> Option<String> {
        let mut req_size = 0;
        let mut property_type: u32 = 0;
        
        // First call to get the required size
        let _ = unsafe {
            SetupDiGetDeviceRegistryPropertyW(
                device_info_set,
                device_info_data,
                property,
                Some(&mut property_type),
                None,
                Some(&mut req_size),
            )
        };

        if req_size == 0 {
            return None;
        }

        let mut buffer: Vec<u8> = vec![0; req_size as usize];
        if unsafe { SetupDiGetDeviceRegistryPropertyW(
            device_info_set,
            device_info_data,
            property,
            Some(&mut property_type),
            Some(buffer.as_mut_slice()),
            None,
        ) }.is_ok() {
            // The buffer contains UTF-16 characters
            let u16_slice = unsafe { std::slice::from_raw_parts(
                buffer.as_ptr() as *const u16,
                (req_size as usize) / 2,
            ) };
            
            // Remove null terminators
            let len = u16_slice.iter().position(|&c| c == 0).unwrap_or(u16_slice.len());
            let os_string = OsString::from_wide(&u16_slice[..len]);
            return os_string.into_string().ok();
        }
        
        None
    }

    // Helper to extract multi-string property (like Hardware IDs)
    unsafe fn get_multistring_property(
        device_info_set: HDEVINFO,
        device_info_data: &mut SP_DEVINFO_DATA,
        property: SETUP_DI_REGISTRY_PROPERTY,
    ) -> Vec<String> {
        let mut req_size: u32 = 0;
        let mut property_type: u32 = 0;
        
        let _ = unsafe { SetupDiGetDeviceRegistryPropertyW(
            device_info_set,
            device_info_data,
            property,
            Some(&mut property_type),
            None,
            Some(&mut req_size),
        ) };

        if req_size == 0 {
            return Vec::new();
        }

        let mut buffer: Vec<u8> = vec![0; req_size as usize];
        if unsafe { SetupDiGetDeviceRegistryPropertyW(
            device_info_set,
            device_info_data,
            property,
            Some(&mut property_type),
            Some(buffer.as_mut_slice()),
            None,
        ) }.is_ok() {
            let u16_slice = unsafe { std::slice::from_raw_parts(
                buffer.as_ptr() as *const u16,
                (req_size as usize) / 2,
            ) };
            
            let mut result = Vec::new();
            let mut current = Vec::new();
            
            for &c in u16_slice {
                if c == 0 {
                    if !current.is_empty() {
                        if let Ok(s) = OsString::from_wide(&current).into_string() {
                            result.push(s);
                        }
                        current.clear();
                    }
                } else {
                    current.push(c);
                }
            }
            return result;
        }
        
        Vec::new()
    }

    pub async fn get_device_detail(target_instance_id: &str) -> Result<Option<UsbDeviceDetail>, String> {
        let target_instance_id_upper = target_instance_id.to_uppercase();
        
        let handle = unsafe {
            SetupDiGetClassDevsW(
                None,
                None,
                None,
                DIGCF_ALLCLASSES | DIGCF_PRESENT,
            )
        };

        let handle = match handle {
            Ok(h) => h,
            Err(e) => return Err(format!("Failed to get device info set: {}", e)),
        };

        if handle.is_invalid() {
            return Err("Failed to get device info set (invalid handle)".to_string());
        }

        // Use scopeguard to ensure the handle is destroyed even if we return early
        defer! {
            unsafe {
                let _ = SetupDiDestroyDeviceInfoList(handle);
            }
        }

        let mut index = 0;
        loop {
            let mut dev_info_data = SP_DEVINFO_DATA {
                cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
                ..Default::default()
            };

            unsafe {
                if SetupDiEnumDeviceInfo(handle, index, &mut dev_info_data).is_err() {
                    break; // No more devices
                }
                
                // Get Instance ID
                let mut req_size: u32 = 0;
                let _ = SetupDiGetDeviceInstanceIdW(handle, &dev_info_data, None, Some(&mut req_size));
                
                if req_size > 0 {
                    let mut buffer: Vec<u16> = vec![0; req_size as usize];
                    if SetupDiGetDeviceInstanceIdW(handle, &dev_info_data, Some(buffer.as_mut_slice()), None).is_ok() {
                        let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
                        if let Ok(instance_id) = OsString::from_wide(&buffer[..len]).into_string() {
                            if instance_id.to_uppercase() == target_instance_id_upper {
                                // Match found! Extract properties
                                let mfg_raw = get_string_property(handle, &mut dev_info_data, SPDRP_MFG);
                                let friendly_name = get_string_property(handle, &mut dev_info_data, SPDRP_FRIENDLYNAME);
                                let dev_desc = get_string_property(handle, &mut dev_info_data, SPDRP_DEVICEDESC);
                                let class_guid = get_string_property(handle, &mut dev_info_data, SPDRP_CLASSGUID);
                                let hw_ids = get_multistring_property(handle, &mut dev_info_data, SPDRP_HARDWAREID);
                                
                                let product_name = friendly_name.or(dev_desc.clone());
                                
                                // Try to get the real manufacturer name from the USB Vendor ID database.
                                // SPDRP_MFG returns INF-level strings (e.g. "(Standard USB Host Controller)")
                                // which are not the actual hardware vendor names.
                                let manufacturer = crate::usb::vendor_ids::parse_vid_from_hardware_ids(&hw_ids)
                                    .and_then(|vid| crate::usb::vendor_ids::lookup_vendor(vid))
                                    .map(|s| s.to_string())
                                    .or_else(|| {
                                        // Fall back to SPDRP_MFG, stripping the leading/trailing parens
                                        // that Windows puts around INF-derived names like "(Standard USB Host Controller)"
                                        mfg_raw.map(|s| {
                                            let s = s.trim();
                                            if s.starts_with('(') && s.ends_with(')') {
                                                s[1..s.len()-1].to_string()
                                            } else {
                                                s.to_string()
                                            }
                                        })
                                    });
                                
                                let detail = UsbDeviceDetail {
                                    instance_id: Some(instance_id),
                                    manufacturer,
                                    product_name,
                                    class_guid,
                                    hardware_ids: hw_ids,
                                    bus_id: None,
                                    vid_pid: None,
                                    serial_number: None,
                                };
                                
                                return Ok(Some(detail));
                            }
                        }
                    }
                }
            }
            index += 1;
        }

        Ok(None)
    }
}
