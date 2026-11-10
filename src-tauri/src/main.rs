// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! tpt-view desktop shell. Commands are thin: validation lives in [`ipc`], image work lives in
//! `tpt-view-core`.

mod diagnostics;
mod ipc;

use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use image::{DynamicImage, ImageFormat};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tpt_view_core::batch::{self, Progress};
use tpt_view_core::edit::{EditSession, SaveTarget};
use tpt_view_core::scan::{self, FileEntry};
use tpt_view_core::thumbs::ThumbnailCache;
use tpt_view_core::watch::{DirEvent, DirWatcher};
use tpt_view_core::{ImageBackend, StdBackend};

#[derive(Default)]
struct AppState {
    sessions: Mutex<HashMap<u64, EditSession>>,
    next_session: AtomicU64,
    batch_cancels: Mutex<HashMap<u64, Arc<AtomicBool>>>,
    watch_stop: Mutex<Option<Arc<AtomicBool>>>,
}

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

fn png_data_url(img: &DynamicImage) -> CmdResult<String> {
    let mut buf = Cursor::new(Vec::new());
    img.write_to(&mut buf, ImageFormat::Png).map_err(err)?;
    Ok(format!(
        "data:image/png;base64,{}",
        STANDARD.encode(buf.into_inner())
    ))
}

/// Downscales to fit `max_edge` when it is non-zero; zero means full size.
fn fit(img: DynamicImage, max_edge: u32) -> DynamicImage {
    if max_edge == 0 {
        img
    } else {
        img.thumbnail(max_edge, max_edge)
    }
}

fn thumb_cache(app: &AppHandle, max_edge: u32) -> CmdResult<ThumbnailCache> {
    let base = app.path().app_cache_dir().map_err(err)?;
    ThumbnailCache::new(base.join("thumbs"), max_edge.max(1)).map_err(err)
}

// ---- directory browsing --------------------------------------------------------------------

#[tauri::command]
fn initial_path() -> Option<String> {
    ipc::initial_path_from_args(std::env::args())
}

#[tauri::command]
fn list_directory(dir: String) -> CmdResult<Vec<ipc::EntryDto>> {
    let dir = ipc::validate_directory(&dir)?;
    Ok(scan::scan_dir(&dir)
        .map_err(err)?
        .into_iter()
        .map(ipc::EntryDto::from)
        .collect())
}

#[tauri::command]
fn thumbnail(app: AppHandle, path: String, max_edge: u32) -> CmdResult<String> {
    let path = ipc::validate_image_file(&path)?;
    let cache = thumb_cache(&app, max_edge)?;
    let entry = entry_for(&path)?;
    let thumb = cache.get_or_create(&entry).map_err(err)?;
    let bytes = std::fs::read(thumb.path).map_err(err)?;
    Ok(format!("data:image/png;base64,{}", STANDARD.encode(bytes)))
}

/// Decodes any supported file (RAW or raster) to upright pixels plus its metadata.
fn load_oriented(path: &Path) -> CmdResult<(DynamicImage, tpt_view_core::Metadata)> {
    if tpt_view_core::raw::is_raw_path(path) {
        let developed = tpt_view_core::raw::load_path(path).map_err(err)?;
        return Ok((developed.image, developed.metadata));
    }
    if tpt_view_core::heic::is_heic_path(path) {
        let decoded = tpt_view_core::heic::load_path(path).map_err(err)?;
        return Ok((decoded.image, decoded.metadata));
    }
    let bytes = std::fs::read(path).map_err(err)?;
    let decoded = StdBackend.decode(&bytes).map_err(err)?;
    Ok((decoded.image, decoded.metadata))
}

fn entry_for(path: &Path) -> CmdResult<FileEntry> {
    let meta = std::fs::metadata(path).map_err(err)?;
    let modified_nanos = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    Ok(FileEntry {
        path: path.to_path_buf(),
        size: meta.len(),
        modified_nanos,
    })
}

// ---- viewing -------------------------------------------------------------------------------

#[tauri::command]
fn image_info(path: String) -> CmdResult<ipc::InfoDto> {
    let path = ipc::validate_image_file(&path)?;
    let (width, height, format, meta) = if tpt_view_core::heic::is_heic_path(&path) {
        // HEIF headers can be read without decoding, but the image crate cannot probe them.
        let decoded = tpt_view_core::heic::load_path(&path).map_err(err)?;
        (
            decoded.image.width(),
            decoded.image.height(),
            "HEIF".to_string(),
            decoded.metadata,
        )
    } else if tpt_view_core::raw::is_raw_path(&path) {
        // RAW files are developed to learn their size, so this costs a full decode.
        let developed = tpt_view_core::raw::load_path(&path).map_err(err)?;
        (
            developed.image.width(),
            developed.image.height(),
            "Camera RAW".to_string(),
            developed.metadata,
        )
    } else {
        let bytes = std::fs::read(&path).map_err(err)?;
        let backend = StdBackend;
        let info = backend.probe(&bytes).map_err(err)?;
        let meta = backend.read_metadata(&bytes).map_err(err)?;
        (info.width, info.height, format!("{:?}", info.format), meta)
    };
    Ok(ipc::InfoDto {
        width,
        height,
        format,
        metadata: ipc::MetadataDto {
            orientation: meta.orientation.to_exif(),
            make: meta.make,
            model: meta.model,
            date_time_original: meta.date_time_original,
            gps: meta.gps.map(|g| [g.latitude, g.longitude]),
            title: meta.title,
            description: meta.description,
            creator: meta.creator,
            copyright: meta.copyright,
            keywords: meta.keywords,
        },
    })
}

/// Oriented, optionally downscaled PNG of an image file for display.
#[tauri::command]
fn preview(path: String, max_edge: u32) -> CmdResult<String> {
    let path = ipc::validate_image_file(&path)?;
    let (image, _) = load_oriented(&path)?;
    png_data_url(&fit(image, max_edge))
}

// ---- editing -------------------------------------------------------------------------------

#[tauri::command]
fn edit_open(state: State<'_, AppState>, path: String) -> CmdResult<u64> {
    let path = ipc::validate_image_file(&path)?;
    let (image, _) = load_oriented(&path)?;
    let id = state.next_session.fetch_add(1, Ordering::Relaxed) + 1;
    let session = EditSession::new(image, Some(path));
    state.sessions.lock().map_err(err)?.insert(id, session);
    Ok(id)
}

fn with_session<T>(
    state: &State<'_, AppState>,
    id: u64,
    f: impl FnOnce(&mut EditSession) -> CmdResult<T>,
) -> CmdResult<T> {
    let mut sessions = state.sessions.lock().map_err(err)?;
    let session = sessions.get_mut(&id).ok_or("unknown edit session")?;
    f(session)
}

#[tauri::command]
fn edit_apply(state: State<'_, AppState>, id: u64, op: ipc::OpDto) -> CmdResult<()> {
    let op: tpt_view_core::edit::Op = op.into();
    with_session(&state, id, |s| s.apply(&op).map_err(err))
}

#[tauri::command]
fn edit_undo(state: State<'_, AppState>, id: u64) -> CmdResult<bool> {
    with_session(&state, id, |s| Ok(s.undo()))
}

#[tauri::command]
fn edit_redo(state: State<'_, AppState>, id: u64) -> CmdResult<bool> {
    with_session(&state, id, |s| Ok(s.redo()))
}

#[tauri::command]
fn edit_preview(state: State<'_, AppState>, id: u64, max_edge: u32) -> CmdResult<String> {
    with_session(&state, id, |s| {
        let img = DynamicImage::ImageRgba8(s.current().clone());
        png_data_url(&fit(img, max_edge))
    })
}

#[tauri::command]
fn edit_save(
    state: State<'_, AppState>,
    id: u64,
    target: ipc::SaveDto,
    format: String,
    quality: u8,
) -> CmdResult<String> {
    let format = tpt_view_core::batch::OutputFormat::parse(&format).map_err(err)?;
    let image_format = match format {
        tpt_view_core::batch::OutputFormat::Jpeg => ImageFormat::Jpeg,
        tpt_view_core::batch::OutputFormat::Png => ImageFormat::Png,
        tpt_view_core::batch::OutputFormat::WebP => ImageFormat::WebP,
        tpt_view_core::batch::OutputFormat::Tiff => ImageFormat::Tiff,
        tpt_view_core::batch::OutputFormat::Bmp => ImageFormat::Bmp,
        tpt_view_core::batch::OutputFormat::Avif => ImageFormat::Avif,
    };
    let target = match target {
        ipc::SaveDto::SaveAs { path } => SaveTarget::SaveAs(ipc::validate_output_file(&path)?),
        ipc::SaveDto::Overwrite { confirmed } => SaveTarget::Overwrite { confirmed },
    };
    with_session(&state, id, |s| {
        let written: PathBuf = s.save(&target, image_format, quality).map_err(err)?;
        Ok(written.to_string_lossy().into_owned())
    })
}

#[tauri::command]
fn edit_close(state: State<'_, AppState>, id: u64) -> CmdResult<()> {
    state.sessions.lock().map_err(err)?.remove(&id);
    Ok(())
}

// ---- batch ---------------------------------------------------------------------------------

#[derive(Serialize, Clone)]
struct BatchProgressOut {
    job_id: u64,
    done: usize,
    total: usize,
}

#[tauri::command]
async fn batch_run(
    app: AppHandle,
    state: State<'_, AppState>,
    job_id: u64,
    job: ipc::BatchDto,
) -> CmdResult<Vec<ipc::ReportDto>> {
    let job = ipc::to_batch_job(job)?;
    let cancel = Arc::new(AtomicBool::new(false));
    state
        .batch_cancels
        .lock()
        .map_err(err)?
        .insert(job_id, cancel.clone());

    let emitter = app.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        batch::run(&job, &cancel, |Progress { done, total }| {
            let _ = emitter.emit(
                "batch-progress",
                BatchProgressOut {
                    job_id,
                    done,
                    total,
                },
            );
        })
    })
    .await
    .map_err(err)?;

    state.batch_cancels.lock().map_err(err)?.remove(&job_id);
    let reports = outcome.map_err(err)?;
    Ok(reports.iter().map(ipc::report_dto).collect())
}

#[tauri::command]
fn batch_cancel(state: State<'_, AppState>, job_id: u64) -> CmdResult<bool> {
    let map = state.batch_cancels.lock().map_err(err)?;
    match map.get(&job_id) {
        Some(flag) => {
            flag.store(true, Ordering::Relaxed);
            Ok(true)
        }
        None => Ok(false),
    }
}

// ---- directory watching --------------------------------------------------------------------

#[derive(Serialize, Clone)]
struct DirChangeOut {
    kind: &'static str,
    path: String,
}

#[derive(Serialize, Clone)]
struct DirChangesOut {
    dir: String,
    changes: Vec<DirChangeOut>,
}

#[tauri::command]
fn watch_directory(app: AppHandle, state: State<'_, AppState>, dir: String) -> CmdResult<()> {
    let dir = ipc::validate_directory(&dir)?;
    let watcher = DirWatcher::start(&dir).map_err(err)?;

    // Stop any previous watcher thread before starting a new one.
    let stop = Arc::new(AtomicBool::new(false));
    if let Some(previous) = state.watch_stop.lock().map_err(err)?.replace(stop.clone()) {
        previous.store(true, Ordering::Relaxed);
    }

    let dir_label = dir.to_string_lossy().into_owned();
    std::thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            let events = watcher.wait(Duration::from_millis(250));
            if events.is_empty() {
                continue;
            }
            let changes = events
                .into_iter()
                .map(|ev| {
                    let kind = match ev {
                        DirEvent::Added(_) => "added",
                        DirEvent::Modified(_) => "modified",
                        DirEvent::Removed(_) => "removed",
                    };
                    DirChangeOut {
                        kind,
                        path: ev.path().to_string_lossy().into_owned(),
                    }
                })
                .collect();
            let _ = app.emit(
                "dir-changed",
                DirChangesOut {
                    dir: dir_label.clone(),
                    changes,
                },
            );
        }
    });
    Ok(())
}

#[tauri::command]
fn watch_stop(state: State<'_, AppState>) -> CmdResult<()> {
    if let Some(flag) = state.watch_stop.lock().map_err(err)?.take() {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}

// ---- diagnostics and maintenance -----------------------------------------------------------

#[tauri::command]
fn crash_logging_enabled(app: AppHandle) -> CmdResult<bool> {
    let dir = app.path().app_log_dir().map_err(err)?;
    Ok(diagnostics::is_enabled(&dir))
}

#[tauri::command]
fn set_crash_logging(app: AppHandle, enabled: bool) -> CmdResult<()> {
    let dir = app.path().app_log_dir().map_err(err)?;
    diagnostics::set_enabled(&dir, enabled).map_err(err)
}

/// Deletes every cached thumbnail. Returns how many files were removed.
#[tauri::command]
fn clear_thumbnail_cache(app: AppHandle) -> CmdResult<usize> {
    let dir = app.path().app_cache_dir().map_err(err)?.join("thumbs");
    if !dir.is_dir() {
        return Ok(0);
    }
    let mut removed = 0;
    for item in std::fs::read_dir(&dir).map_err(err)? {
        let path = item.map_err(err)?.path();
        let is_cache_file = path.extension().is_some_and(|e| e == "png" || e == "tmp");
        if is_cache_file && std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Crash logs go to the app log directory, and only when the user has opted in.
            if let Ok(dir) = app.path().app_log_dir() {
                diagnostics::install(dir);
            }
            Ok(())
        })
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            initial_path,
            list_directory,
            thumbnail,
            image_info,
            preview,
            edit_open,
            edit_apply,
            edit_undo,
            edit_redo,
            edit_preview,
            edit_save,
            edit_close,
            batch_run,
            batch_cancel,
            watch_directory,
            watch_stop,
            crash_logging_enabled,
            set_crash_logging,
            clear_thumbnail_cache,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tpt-view");
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;

    #[test]
    fn fit_respects_zero_as_full_size() {
        let img = DynamicImage::ImageRgba8(RgbaImage::new(300, 100));
        assert_eq!(fit(img.clone(), 0).width(), 300);
        assert_eq!(fit(img, 60).width(), 60);
    }

    #[test]
    fn png_data_url_has_expected_prefix() {
        let url = png_data_url(&DynamicImage::ImageRgba8(RgbaImage::new(2, 2))).unwrap();
        assert!(url.starts_with("data:image/png;base64,iVBOR"));
    }
}
