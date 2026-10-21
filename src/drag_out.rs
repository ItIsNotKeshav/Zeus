//! Files carried out of the window go on to the system's own drag and drop,
//! so a photo or a document can be dropped in Explorer, on the desktop, or in
//! another app.
//!
//! egui and winit can take files dropped on the window but cannot start a
//! drag of their own, so the interface asks for one ([`request`]) when a
//! carried message with downloaded files leaves the window, and the shell
//! starts it on the next frame with the window at hand ([`perform`]). Only
//! Windows does so far, through the shell's own data object for the files
//! (which also draws their thumbnails under the pointer); elsewhere
//! [`supported`] is false and a carried message stays in the window.

use std::path::PathBuf;
use std::sync::Mutex;

static PENDING: Mutex<Option<Vec<PathBuf>>> = Mutex::new(None);

/// Whether files can be dragged out of the window here.
pub fn supported() -> bool {
    cfg!(windows)
}

/// Asks for the files to be handed to the system's drag and drop.
pub fn request(files: Vec<PathBuf>) {
    *PENDING.lock().unwrap_or_else(|p| p.into_inner()) = Some(files);
}

/// Starts the drag asked for, if any. On Windows this returns once the
/// files are dropped or the drag is abandoned.
pub fn perform(frame: &eframe::Frame) {
    let Some(files) = PENDING.lock().unwrap_or_else(|p| p.into_inner()).take() else {
        return;
    };
    let files: Vec<PathBuf> = files.into_iter().filter(|file| file.is_file()).collect();
    if files.is_empty() {
        return;
    }
    platform::start(frame, &files);
}

#[cfg(windows)]
mod platform {
    use std::path::PathBuf;

    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::System::Com::IDataObject;
    use windows::Win32::System::Ole::{DROPEFFECT_COPY, IDropSource};
    use windows::Win32::UI::Shell::Common::ITEMIDLIST;
    use windows::Win32::UI::Shell::{
        BHID_DataObject, ILCreateFromPathW, ILFree, IShellItemArray,
        SHCreateShellItemArrayFromIDLists, SHDoDragDrop,
    };
    use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_LBUTTONUP};
    use windows::core::HSTRING;

    pub fn start(frame: &eframe::Frame, files: &[PathBuf]) {
        let Ok(handle) = frame.window_handle() else {
            return;
        };
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return;
        };
        let hwnd = HWND(handle.hwnd.get() as *mut _);
        // SAFETY: shell calls on the window's own thread, which winit set up
        // for OLE to take files dropped on the window; each id list is freed
        // once the array holding copies of them is made.
        unsafe {
            let lists: Vec<*mut ITEMIDLIST> = files
                .iter()
                .map(|file| ILCreateFromPathW(&HSTRING::from(file.as_path())))
                .filter(|list| !list.is_null())
                .collect();
            let constant: Vec<*const ITEMIDLIST> =
                lists.iter().map(|list| list.cast_const()).collect();
            let data =
                SHCreateShellItemArrayFromIDLists(&constant).and_then(|items: IShellItemArray| {
                    items.BindToHandler::<_, IDataObject>(None, &BHID_DataObject)
                });
            for list in lists {
                ILFree(Some(list.cast_const()));
            }
            match data {
                // Without a drop source of ours, the shell supplies its own.
                Ok(data) => {
                    if let Err(error) =
                        SHDoDragDrop(Some(hwnd), &data, None::<&IDropSource>, DROPEFFECT_COPY)
                    {
                        log::warn!("the drag out of the window failed: {error}");
                    }
                }
                Err(error) => log::warn!("could not offer the files to drag: {error}"),
            }
            // The drag's own loop took the button's release. Give it back,
            // so winit lets go of the pointer and egui ends its drag.
            let _ = PostMessageW(Some(hwnd), WM_LBUTTONUP, WPARAM(0), LPARAM(0));
        }
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn start(_frame: &eframe::Frame, _files: &[std::path::PathBuf]) {}
}
