// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! Windows platform integration (Phase 10).
//!
//! File association is built in two steps:
//!
//! 1. [`file_assoc::register_plan`] / [`file_assoc::unregister_plan`] describe the per-user
//!    registry writes as plain data. Everything here is pure and testable on any platform.
//! 2. [`file_assoc::apply`] (Windows only) performs the writes in `HKEY_CURRENT_USER`.
//!
//! The plan registers tpt-view as a *capable* application. It does not set the default image
//! handler: since Windows 8 that choice belongs to the user, who makes it in Settings → Default
//! apps. Writing `UserChoice` directly is blocked by the OS and is not attempted.

pub mod file_assoc;
