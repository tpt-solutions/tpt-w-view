// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! Per-user file-association registry plan (Phase 10).

use std::path::Path;

use tpt_view_core::scan::SUPPORTED_EXTENSIONS;

/// ProgID shared by every association.
pub const PROG_ID: &str = "TptView.Image";
/// Name under which the app registers its capabilities.
pub const APP_NAME: &str = "tpt-view";
const CAPABILITIES_KEY: &str = r"Software\TPT Solutions\tpt-view\Capabilities";
const REGISTERED_APPS: &str = r"Software\RegisteredApplications";

/// A value to write under `HKEY_CURRENT_USER`. An empty `name` means the key's default value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueWrite {
    pub key: String,
    pub name: String,
    pub data: String,
}

/// A registry deletion under `HKEY_CURRENT_USER`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delete {
    /// Remove a key and everything beneath it.
    Key(String),
    /// Remove one named value.
    Value { key: String, name: String },
}

/// Everything a register or unregister operation will touch.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan {
    pub writes: Vec<ValueWrite>,
    pub deletes: Vec<Delete>,
}

/// Rejects executable paths that would break the quoted shell command.
fn validate_exe(exe: &Path) -> Result<String, String> {
    let text = exe.to_string_lossy().into_owned();
    if text.contains('"') || text.contains('\0') {
        return Err(format!(
            "executable path contains an invalid character: {text}"
        ));
    }
    if !text.to_ascii_lowercase().ends_with(".exe") {
        return Err(format!("not an executable path: {text}"));
    }
    Ok(text)
}

fn write(key: impl Into<String>, name: &str, data: impl Into<String>) -> ValueWrite {
    ValueWrite {
        key: key.into(),
        name: name.into(),
        data: data.into(),
    }
}

/// Builds the registration plan for the executable at `exe`.
pub fn register_plan(exe: &Path) -> Result<Plan, String> {
    let exe = validate_exe(exe)?;
    let prog_key = format!(r"Software\Classes\{PROG_ID}");

    let mut plan = Plan::default();
    plan.writes
        .push(write(&prog_key, "", "tpt-view image file"));
    plan.writes.push(write(
        format!(r"{prog_key}\DefaultIcon"),
        "",
        format!("{exe},0"),
    ));
    plan.writes.push(write(
        format!(r"{prog_key}\shell\open"),
        "",
        "Open with tpt-view",
    ));
    plan.writes.push(write(
        format!(r"{prog_key}\shell\open\command"),
        "",
        format!("\"{exe}\" \"%1\""),
    ));

    for ext in SUPPORTED_EXTENSIONS {
        // Offer tpt-view in "Open with" for each extension.
        plan.writes.push(write(
            format!(r"Software\Classes\.{ext}\OpenWithProgids"),
            PROG_ID,
            "",
        ));
        // Advertise the association to Default apps.
        plan.writes.push(write(
            format!(r"{CAPABILITIES_KEY}\FileAssociations"),
            &format!(".{ext}"),
            PROG_ID,
        ));
    }

    plan.writes
        .push(write(CAPABILITIES_KEY, "ApplicationName", APP_NAME));
    plan.writes.push(write(
        CAPABILITIES_KEY,
        "ApplicationDescription",
        "Fast image viewer and batch converter",
    ));
    plan.writes
        .push(write(REGISTERED_APPS, APP_NAME, CAPABILITIES_KEY));
    Ok(plan)
}

/// Builds the plan that reverses [`register_plan`].
pub fn unregister_plan() -> Plan {
    let mut plan = Plan::default();
    plan.deletes
        .push(Delete::Key(format!(r"Software\Classes\{PROG_ID}")));
    plan.deletes
        .push(Delete::Key(r"Software\TPT Solutions\tpt-view".into()));
    plan.deletes.push(Delete::Value {
        key: REGISTERED_APPS.into(),
        name: APP_NAME.into(),
    });
    for ext in SUPPORTED_EXTENSIONS {
        plan.deletes.push(Delete::Value {
            key: format!(r"Software\Classes\.{ext}\OpenWithProgids"),
            name: PROG_ID.into(),
        });
    }
    plan
}

/// Performs `plan` against `HKEY_CURRENT_USER`. Windows only.
#[cfg(windows)]
pub fn apply(plan: &Plan) -> std::io::Result<()> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_ALL_ACCESS};
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for w in &plan.writes {
        let (key, _) = hkcu.create_subkey(&w.key)?;
        key.set_value(&w.name, &w.data)?;
    }
    for d in &plan.deletes {
        match d {
            Delete::Key(path) => match hkcu.delete_subkey_all(path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            },
            Delete::Value { key, name } => {
                if let Ok(k) = hkcu.open_subkey_with_flags(key, KEY_ALL_ACCESS) {
                    match k.delete_value(name) {
                        Ok(()) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                        Err(e) => return Err(e),
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exe() -> &'static Path {
        Path::new(r"C:\Program Files\TPT\tpt-view\tpt-view.exe")
    }

    #[test]
    fn register_plan_quotes_executable_and_handles_spaces() {
        let plan = register_plan(exe()).unwrap();
        let command = plan
            .writes
            .iter()
            .find(|w| w.key.ends_with(r"shell\open\command"))
            .expect("command key");
        assert_eq!(
            command.data,
            r#""C:\Program Files\TPT\tpt-view\tpt-view.exe" "%1""#
        );
    }

    #[test]
    fn every_supported_extension_is_advertised() {
        let plan = register_plan(exe()).unwrap();
        for ext in SUPPORTED_EXTENSIONS {
            assert!(
                plan.writes
                    .iter()
                    .any(|w| w.name == format!(".{ext}") && w.data == PROG_ID),
                "missing FileAssociations entry for {ext}"
            );
            assert!(plan
                .writes
                .iter()
                .any(|w| w.key == format!(r"Software\Classes\.{ext}\OpenWithProgids")));
        }
    }

    #[test]
    fn writes_stay_under_current_user_software_tree() {
        let plan = register_plan(exe()).unwrap();
        for w in &plan.writes {
            assert!(w.key.starts_with(r"Software\"), "{}", w.key);
        }
        for d in unregister_plan().deletes {
            let key = match d {
                Delete::Key(k) => k,
                Delete::Value { key, .. } => key,
            };
            assert!(key.starts_with(r"Software\"), "{key}");
        }
    }

    #[test]
    fn does_not_write_user_default_choice() {
        // Windows blocks direct UserChoice writes; the plan must never attempt one.
        let plan = register_plan(exe()).unwrap();
        assert!(plan.writes.iter().all(|w| !w.key.contains("UserChoice")));
    }

    #[test]
    fn rejects_unsafe_or_non_exe_paths() {
        assert!(register_plan(Path::new(r#"C:\bad"quote\x.exe"#)).is_err());
        assert!(register_plan(Path::new(r"C:\tools\tpt-view.dll")).is_err());
    }

    #[test]
    fn unregister_mirrors_registration_targets() {
        let plan = unregister_plan();
        assert!(plan
            .deletes
            .iter()
            .any(|d| matches!(d, Delete::Key(k) if k.ends_with(PROG_ID))));
        assert!(plan.deletes.iter().any(|d| matches!(
            d,
            Delete::Value { key, name } if key == REGISTERED_APPS && name == APP_NAME
        )));
    }
}
