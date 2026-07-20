// SPDX-FileCopyrightText: Copyright (c) 2026 owu <wqh@live.com>
// SPDX-License-Identifier: GPL-3.0-only

use std::process::Command;
use std::os::windows::process::CommandExt;
use tracing::{info, warn};

const CREATE_NO_WINDOW: u32 = 0x08000000;

// Get the IP address of the specified distribution, includes retry logic to wait for network readiness
pub fn get_distro_ip(distro_name: &str) -> Result<String, String> {
    info!("Fetching IP for distro: {}", distro_name);

    // Early exit: if the distro is not running, skip the retry loop entirely
    // to avoid blocking the calling thread for up to 30 seconds.
    if !is_distro_running(distro_name) {
        warn!("Distro '{}' is not running, skipping IP fetch.", distro_name);
        return Err(format!("Distro '{}' is not running", distro_name));
    }

    const MAX_ATTEMPTS: u32 = 10;
    let mut last_error = String::new();
    for attempt in 1..=MAX_ATTEMPTS {
        if attempt > 1 {
            info!("Retrying IP fetch for {} (attempt {}/{})", distro_name, attempt, MAX_ATTEMPTS);
            std::thread::sleep(std::time::Duration::from_secs(1));
        }

        let mut candidates: Vec<(String, String)> = Vec::new();

        // Solution 1: ip -4 addr show (Most detailed, provides interface names)
        let output_ip = Command::new("wsl")
            .env("WSL_UTF8", "1")
            .args(&["-d", distro_name, "--", "ip", "-4", "addr", "show"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();

        if let Ok(out) = &output_ip {
            if out.status.success() {
                let stdout = crate::wsl::decoder::decode_output(&out.stdout);
                candidates.extend(parse_ip_addr_output(&stdout));
            } else {
                last_error = format!("ip addr show failed: {}", crate::wsl::decoder::decode_output(&out.stderr).trim());
            }
        }

        // Solution 2 Fallback: ifconfig
        if candidates.is_empty() {
            let output_ifconfig = Command::new("wsl")
                .env("WSL_UTF8", "1")
                .args(&["-d", distro_name, "--", "ifconfig"])
                .creation_flags(CREATE_NO_WINDOW)
                .output();

            if let Ok(out) = &output_ifconfig {
                if out.status.success() {
                    let stdout = crate::wsl::decoder::decode_output(&out.stdout);
                    candidates.extend(parse_ifconfig_output(&stdout));
                } else {
                    last_error = format!("ifconfig failed: {}", crate::wsl::decoder::decode_output(&out.stderr).trim());
                }
            }
        }

        // Solution 3 Fallback: hostname -I
        if candidates.is_empty() {
            let output_hostname = Command::new("wsl")
                .env("WSL_UTF8", "1")
                .args(&["-d", distro_name, "--", "hostname", "-I"])
                .creation_flags(CREATE_NO_WINDOW)
                .output();

            if let Ok(out) = &output_hostname {
                if out.status.success() {
                    let stdout = crate::wsl::decoder::decode_output(&out.stdout);
                    candidates.extend(parse_hostname_i_output(&stdout));
                } else {
                    last_error = format!("hostname -I failed: {}", crate::wsl::decoder::decode_output(&out.stderr).trim());
                }
            }
        }

        if !candidates.is_empty() {
            info!("Found candidate IPs for {} (attempt {}): {:?}", distro_name, attempt, candidates);
            if let Some(best_ip) = select_best_ip(&candidates) {
                info!("Selected best IP: {} for {}", best_ip, distro_name);
                return Ok(best_ip);
            } else {
                last_error = "No valid IPs found among candidates".to_string();
            }
        } else if last_error.is_empty() {
            last_error = "All IP fetch commands failed or returned empty results".to_string();
        }
    }

    Err(format!(
        "Could not find IPv4 address for '{}' after {} attempts. Last error: {}",
        distro_name, MAX_ATTEMPTS, last_error
    ))
}

fn parse_ip_addr_output(stdout: &str) -> Vec<(String, String)> {
    let mut ips = Vec::new();
    let mut current_iface = String::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        
        if let Some(first_char) = line.chars().next() {
            if first_char.is_ascii_digit() && line.contains(": ") {
                let parts: Vec<&str> = line.split(": ").collect();
                if parts.len() >= 2 {
                    current_iface = parts[1].split('@').next().unwrap_or(parts[1]).trim().to_string();
                }
            } else if line.starts_with("inet ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() > 1 {
                    let ip_cidr = parts[1];
                    let ip = ip_cidr.split('/').next().unwrap_or(ip_cidr).to_string();
                    if !current_iface.is_empty() {
                        ips.push((current_iface.clone(), ip));
                    }
                }
            }
        }
    }
    ips
}

fn parse_ifconfig_output(stdout: &str) -> Vec<(String, String)> {
    let mut ips = Vec::new();
    let mut current_iface = String::new();

    for line in stdout.lines() {
        let line_end_trimmed = line.trim_end();
        if line_end_trimmed.is_empty() { continue; }
        
        if !line.starts_with(' ') && !line.starts_with('\t') {
            let parts: Vec<&str> = line.split(':').collect();
            if !parts.is_empty() {
                current_iface = parts[0].split_whitespace().next().unwrap_or("").to_string();
            }
        } else {
            let line_trimmed = line.trim();
            if line_trimmed.starts_with("inet ") {
                let parts: Vec<&str> = line_trimmed.split_whitespace().collect();
                if parts.len() > 1 {
                    let mut ip = parts[1];
                    if ip.starts_with("addr:") {
                        ip = &ip[5..];
                    }
                    if !current_iface.is_empty() {
                        ips.push((current_iface.clone(), ip.to_string()));
                    }
                }
            }
        }
    }
    ips
}

fn parse_hostname_i_output(stdout: &str) -> Vec<(String, String)> {
    let mut ips = Vec::new();
    for ip in stdout.split_whitespace() {
        ips.push(("unknown".to_string(), ip.to_string()));
    }
    ips
}

fn select_best_ip(ips: &[(String, String)]) -> Option<String> {
    let mut best_ip = None;
    let mut best_score = -1000;

    for (iface, ip) in ips {
        let mut score = 0;
        
        // Ignore loopback completely
        if iface == "lo" || ip.starts_with("127.") {
            continue; 
        }
        
        // Demote docker/bridge interfaces
        if iface.starts_with("docker") || iface.starts_with("br-") || iface.starts_with("veth") {
            score -= 50;
        }
        
        // Promote eth interfaces
        if iface.starts_with("eth") {
            score += 100;
        } else if iface.starts_with("en") {
            score += 80;
        } else if iface.starts_with("wl") {
            score += 60;
        }
        
        // Demote IPs ending in .1, .255, .0 (often gateways or broadcast)
        if ip.ends_with(".1") || ip.ends_with(".255") || ip.ends_with(".0") {
            score -= 30;
        }
        
        // Small penalty for 10.255.x.x which is often used for WSL loopback-like global scopes
        if ip.starts_with("10.255.") {
            score -= 10;
        }

        if score > best_score {
            best_score = score;
            best_ip = Some(ip.clone());
        }
    }
    
    best_ip
}

// Check if the distribution is currently running (fast check, won't start it)
pub fn is_distro_running(distro_name: &str) -> bool {
    let output = Command::new("wsl")
        .env("WSL_UTF8", "1")
        .args(&["-l", "-q", "--running"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    if let Ok(out) = output {
        if out.status.success() {
            let stdout = crate::wsl::decoder::decode_output(&out.stdout);
            return stdout.lines().any(|l| l.trim().eq_ignore_ascii_case(distro_name));
        }
    }
    false
}
