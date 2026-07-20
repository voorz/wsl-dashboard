// SPDX-FileCopyrightText: Copyright (c) 2026 owu <wqh@live.com>
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};

// Scheduler task data model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerTask {
    // Unique task identifier, e.g. "MyTask" (without split suffix)
    pub task_id: String,
    // Display name
    pub display_name: String,
    // Original cron expression, e.g. "0 9 1,15 * *"
    pub cron_expr: String,
    // Command or script path to execute
    pub command: String,
    // Command arguments
    pub arguments: String,
    // Whether enabled (true when all associated tasks are enabled)
    pub enabled: bool,
    // Current status: "Ready", "Running", "Disabled", "Queued"
    pub status: String,
    // Last run time, format: "2026-06-27 10:30"
    pub last_run: String,
    // Next run time, format: "2026-06-27 11:00"
    pub next_run: String,
    // Human-readable trigger summary, e.g. "Monthly on 1st at 09:00"
    pub trigger_summary: String,
    // Whether to run with admin privileges (UAC)
    pub admin_mode: bool,
    // Task creator (extracted from XML Author field)
    pub author: String,
}

// Raw record from query results (for internal processing)
#[derive(Debug, Clone)]
pub struct RawSchtasksRecord {
    // Full task path, e.g. "\WSLDashboard\UserTasks\MyTask_1"
    pub task_name: String,
    // Extracted task ID, e.g. "MyTask" (without suffix)
    pub task_id: String,
    pub status: String,
    pub last_run: String,
    pub next_run: String,
    // Command to execute
    pub command: String,
}

// Task creation parameters
#[derive(Debug, Clone)]
pub struct CreateTaskParams {
    // Task display name
    pub display_name: String,
    // Command to execute
    pub command: String,
    // Command arguments
    pub arguments: String,
    // Cron expression
    pub cron_expr: String,
    // Whether to run with admin privileges (UAC)
    pub admin_mode: bool,
}

// Parsed cron fields
#[derive(Debug, Clone)]
pub struct CronFields {
    pub minutes: Vec<i32>,
    pub hours: Vec<i32>,
    pub days_of_month: Vec<i32>,
    pub months: Vec<i32>,
    pub days_of_week: Vec<i32>,
}

// Parse result
#[derive(Debug, Clone)]
pub struct CronParsed {
    pub fields: CronFields,
    // Whether it can be directly mapped to a single schtasks command
    pub is_simple: bool,
    // Converted schtasks parameters
    pub schtasks: SchtasksParams,
}

// schtasks command parameters
#[derive(Debug, Clone)]
pub struct SchtasksParams {
    // SC parameter: MINUTE / HOURLY / DAILY / WEEKLY / MONTHLY
    pub sc: String,
    // MO parameter: interval value
    pub mo: Option<String>,
    // ST parameter: start time HH:MM
    pub st: Option<String>,
    // D parameter: day or weekday
    pub d: Option<String>,
    // M parameter: month
    pub m: Option<String>,
}
