// SPDX-FileCopyrightText: Copyright (c) 2026 owu <wqh@live.com>
// SPDX-License-Identifier: GPL-3.0-only

use std::sync::Arc;
use tokio::sync::Mutex;
use crate::{AppWindow, AppState};

pub fn setup(_app: &AppWindow, _app_handle: slint::Weak<AppWindow>, _app_state: Arc<Mutex<AppState>>) {
    // About page removed - GitHub link now opens directly from sidebar
}
