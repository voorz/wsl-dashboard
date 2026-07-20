// SPDX-FileCopyrightText: Copyright (c) 2026 owu <wqh@live.com>
// SPDX-License-Identifier: GPL-3.0-only

use slint::Image;
use tracing::trace;
use std::collections::HashMap;
use std::sync::Mutex;
use once_cell::sync::Lazy;

#[derive(Clone)]
pub enum IconData {
    Svg(&'static [u8]),
}

// Implement Send/Sync for IconData to allow caching in a static Mutex
// &'static [u8] is already Send/Sync.
unsafe impl Send for IconData {}
unsafe impl Sync for IconData {}

static ICON_CACHE: Lazy<Mutex<HashMap<String, IconData>>> = Lazy::new(|| Mutex::new(HashMap::new()));
static DYNAMIC_ICON_MAP: Lazy<Mutex<HashMap<String, &'static str>>> = Lazy::new(|| Mutex::new(HashMap::new()));
static PROBED_DISTROS: Lazy<Mutex<std::collections::HashSet<String>>> = Lazy::new(|| Mutex::new(std::collections::HashSet::new()));
static PENDING_PROBES: Lazy<Mutex<std::collections::HashSet<String>>> = Lazy::new(|| Mutex::new(std::collections::HashSet::new()));

pub fn is_distro_probed(name: &str) -> bool {
    let probed = PROBED_DISTROS.lock().unwrap().contains(name);
    let pending = PENDING_PROBES.lock().unwrap().contains(name);
    probed || pending
}

pub fn mark_distro_probed(name: String) {
    PENDING_PROBES.lock().unwrap().remove(&name);
    PROBED_DISTROS.lock().unwrap().insert(name);
}

pub fn start_probing(name: String) -> bool {
    let mut pending = PENDING_PROBES.lock().unwrap();
    if pending.contains(&name) || PROBED_DISTROS.lock().unwrap().contains(&name) {
        false
    } else {
        pending.insert(name);
        true
    }
}



pub fn get_initial(name: &str) -> String {
    name.chars().next().unwrap_or('?').to_uppercase().to_string()
}

pub fn map_name_to_icon_key(name: &str) -> Option<&'static str> {
    // 1. Check dynamic map first
    {
        let dynamic_map = DYNAMIC_ICON_MAP.lock().unwrap();
        if let Some(key) = dynamic_map.get(name) {
            return Some(key);
        }
    }

    // 2. Static mapping based on name
    let lower_name = name.to_lowercase();
    if lower_name.contains("ubuntu") { Some("ubuntu") }
    else if lower_name.contains("debian") { Some("debian") }
    else if lower_name.contains("kali") { Some("kali") }
    else if lower_name.contains("fedora") || lower_name.contains("fed") { Some("fedora") }
    else if lower_name.contains("opensuse") { Some("opensuse") }
    else if lower_name.contains("suse") { Some("suse") }
    else if lower_name.contains("arch") { Some("arch") }
    else if lower_name.contains("mint") { Some("mint") }
    else if lower_name.contains("alpine") { Some("alpine") }
    else if lower_name.contains("manjaro") { Some("manjaro") }
    else if lower_name.contains("pop") { Some("pop") }
    else if lower_name.contains("centos") { Some("centos") }
    else if lower_name.contains("alma") { Some("alma") }
    else if lower_name.contains("rocky") { Some("rocky") }
    else if lower_name.contains("oracle") || lower_name == "ol" { Some("oracle") }
    else if lower_name.contains("gentoo") { Some("gentoo") }
    else if lower_name.contains("zorin") { Some("zorin") }
    else if lower_name.contains("nix") { Some("nix") }
    else if lower_name.contains("amazon") { Some("amazon") }
    else if lower_name.contains("cachy") { Some("cachy") }
    else if lower_name.contains("redhat") || lower_name.contains("red hat") { Some("redhat") }
    else if lower_name.contains("slackware") { Some("slackware") }
    else if lower_name.contains("void") { Some("void") }
    else { None }
}

pub fn add_dynamic_mapping(distro_name: String, icon_key: &'static str) {
    let mut dynamic_map = DYNAMIC_ICON_MAP.lock().unwrap();
    dynamic_map.insert(distro_name, icon_key);
}

pub fn get_display_name(key: Option<&str>) -> String {
    match key {
        Some("ubuntu") => "Ubuntu".to_string(),
        Some("debian") => "Debian".to_string(),
        Some("kali") => "Kali Linux".to_string(),
        Some("fedora") => "Fedora".to_string(),
        Some("opensuse") => "openSUSE".to_string(),
        Some("suse") => "SUSE".to_string(),
        Some("arch") => "Arch Linux".to_string(),
        Some("mint") => "Linux Mint".to_string(),
        Some("alpine") => "Alpine Linux".to_string(),
        Some("manjaro") => "Manjaro".to_string(),
        Some("pop") => "Pop!_OS".to_string(),
        Some("centos") => "CentOS".to_string(),
        Some("alma") => "AlmaLinux".to_string(),
        Some("rocky") => "Rocky Linux".to_string(),
        Some("oracle") => "Oracle Linux".to_string(),
        Some("gentoo") => "Gentoo".to_string(),
        Some("zorin") => "Zorin OS".to_string(),
        Some("nix") => "NixOS".to_string(),
        Some("amazon") => "Amazon Linux".to_string(),
        Some("cachy") => "CachyOS".to_string(),
        Some("redhat") => "Red Hat".to_string(),
        Some("slackware") => "Slackware".to_string(),
        Some("void") => "Void Linux".to_string(),
        _ => "".to_string(),
    }
}

#[allow(dead_code)]
pub fn load_icon(key: &str) -> Option<Image> {
    load_icon_data(key).and_then(|data| load_image_from_data(key.to_string(), data))
}

thread_local! {
    static SLINT_IMAGE_CACHE: std::cell::RefCell<HashMap<String, Image>> = std::cell::RefCell::new(HashMap::new());
}

pub fn load_image_from_data(key: String, data: IconData) -> Option<Image> {
    SLINT_IMAGE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(img) = cache.get(&key) {
            return Some(img.clone());
        }

        let img = match data {
            IconData::Svg(svg) => Image::load_from_svg_data(svg).ok(),
        };

        if let Some(ref i) = img {
            cache.insert(key, i.clone());
        }
        img
    })
}

pub fn load_icon_data(key: &str) -> Option<IconData> {
    {
        let cache = ICON_CACHE.lock().unwrap();
        if let Some(data) = cache.get(key) {
            return Some(data.clone());
        }
    }

    trace!("load_icon_data: Cache miss for key '{}', loading from disk/assets", key);

    let data = match key {
        "alma" => Some(IconData::Svg(include_bytes!("../../assets/icons/alma.svg"))),
        "alpine" => Some(IconData::Svg(include_bytes!("../../assets/icons/alpine.svg"))),
        "amazon" => Some(IconData::Svg(include_bytes!("../../assets/icons/amazon.svg"))),
        "arch" => Some(IconData::Svg(include_bytes!("../../assets/icons/arch.svg"))),
        "cachy" => Some(IconData::Svg(include_bytes!("../../assets/icons/cachy.svg"))),
        "centos" => Some(IconData::Svg(include_bytes!("../../assets/icons/centos.svg"))),
        "debian" => Some(IconData::Svg(include_bytes!("../../assets/icons/debian.svg"))),
        "fedora" => Some(IconData::Svg(include_bytes!("../../assets/icons/fedora.svg"))),
        "gentoo" => Some(IconData::Svg(include_bytes!("../../assets/icons/gentoo.svg"))),
        "kali" => Some(IconData::Svg(include_bytes!("../../assets/icons/kali.svg"))),
        "manjaro" => Some(IconData::Svg(include_bytes!("../../assets/icons/manjaro.svg"))),
        "mint" => Some(IconData::Svg(include_bytes!("../../assets/icons/mint.svg"))),
        "nix" => Some(IconData::Svg(include_bytes!("../../assets/icons/nix.svg"))),
        "opensuse" => Some(IconData::Svg(include_bytes!("../../assets/icons/opensuse.svg"))),
        "oracle" => Some(IconData::Svg(include_bytes!("../../assets/icons/oracle.svg"))),
        "pop" => Some(IconData::Svg(include_bytes!("../../assets/icons/pop.svg"))),
        "redhat" => Some(IconData::Svg(include_bytes!("../../assets/icons/redhat.svg"))),
        "rocky" => Some(IconData::Svg(include_bytes!("../../assets/icons/rocky.svg"))),
        "slackware" => Some(IconData::Svg(include_bytes!("../../assets/icons/slackware.svg"))),
        "suse" => Some(IconData::Svg(include_bytes!("../../assets/icons/suse.svg"))),
        "ubuntu" => Some(IconData::Svg(include_bytes!("../../assets/icons/ubuntu.svg"))),
        "void" => Some(IconData::Svg(include_bytes!("../../assets/icons/void.svg"))),
        "zorin" => Some(IconData::Svg(include_bytes!("../../assets/icons/zorin.svg"))),
        _ => None,
    };

    if let Some(d) = &data {
        let mut cache = ICON_CACHE.lock().unwrap();
        cache.insert(key.to_string(), d.clone());
    }
    data
}
