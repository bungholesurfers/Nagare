//! Publishes Nagare's live state to a memory-mapped file, so other programs can read what Nagare is showing and doing
//! without impersonating it: aegis-nagare reads it to continue near the running Nagare on the lock screen, and a shell
//! mod (the user's SeaShell / PimpMyShell) could read it to react in the taskbar. The byte layout and the seqlock live
//! in `nagare_scene::state`; this module just owns the file and its mapping and writes a `State` into it each frame.
//!
//! The transport is a **file-backed shared mapping**: any program can map the same file read-only and see our writes,
//! with no special privileges and across sessions - which a `Global\` named section can't do from a plain user
//! process. We only ever write into mapped memory (no per-frame disk I/O), and a reader that catches a half-written
//! record retries via the seqlock. If the mapping can't be made, publishing is a silent no-op; it must never affect
//! the app. Never constructed under `--test`.

use std::fs::File;
use std::path::Path;

use memmap2::MmapMut;
use nagare_scene::state::{self, State};

/// Owns the live-state file and its writable mapping. `publish` writes a snapshot into it.
///
/// We keep the `File` handle open for the publisher's whole life, not just to map it: on Windows the backing file is a
/// *temporary, delete-on-close* file (see `open_backing`), so holding the handle is what keeps it in RAM and keeps it
/// present for readers while Nagare runs; it's deleted the moment every handle closes (Nagare exiting or crashing).
pub struct Publisher {
    // Declared before `map` only for tidiness; both drop together when the publisher does.
    _file: Option<File>,
    map: Option<MmapMut>,
}

impl Publisher {
    /// Opens (creating) the live-state file at `path`, sizes it to the record, and maps it writable. On any failure the
    /// publisher is inert (`publish` does nothing) - a live-state file is a convenience, never a requirement.
    pub fn open(path: &Path) -> Publisher {
        match Self::try_map(path) {
            Ok((file, map)) => Publisher { _file: Some(file), map: Some(map) },
            Err(e) => {
                eprintln!("state: live-state channel off ({}): {e}", path.display());
                Publisher { _file: None, map: None }
            }
        }
    }

    fn try_map(path: &Path) -> std::io::Result<(File, MmapMut)> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let file = open_backing(path)?;
        file.set_len(state::SIZE as u64)?;
        // Safety: we created/own this file and only this process writes it; readers map it read-only and tolerate a
        // torn write through the seqlock, so a concurrent reader is not undefined behaviour at the type level.
        let map = unsafe { MmapMut::map_mut(&file)? };
        Ok((file, map))
    }

    /// Writes `s` into the mapping under the seqlock. Cheap (a memory write); does nothing if the mapping is inert.
    pub fn publish(&mut self, s: &State) {
        if let Some(map) = &mut self.map {
            state::write(&mut map[..], s);
        }
    }
}

/// Opens the backing file. On Windows it is created **temporary and delete-on-close**: `FILE_ATTRIBUTE_TEMPORARY` asks
/// Windows to keep the data in RAM and not write it to disk unless memory runs low (for a 512-byte always-mapped file,
/// effectively never - so the live-state channel does not wear the flash), and `FILE_FLAG_DELETE_ON_CLOSE` removes the
/// file when every handle closes (Nagare exiting or crashing), so nothing lingers and "file present" means "running".
/// Readers can still open and map it by path meanwhile (we grant share read/write/delete).
#[cfg(windows)]
fn open_backing(path: &Path) -> std::io::Result<File> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::FromRawHandle;
    use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::CreateFileW;

    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x1 | 0x2 | 0x4;
    const CREATE_ALWAYS: u32 = 2;
    const FILE_ATTRIBUTE_TEMPORARY: u32 = 0x100;
    const FILE_FLAG_DELETE_ON_CLOSE: u32 = 0x0400_0000;

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    // Safety: a standard CreateFileW call; the returned handle is wrapped in a File that owns it.
    let handle: HANDLE = unsafe {
        CreateFileW(
            wide.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ_WRITE_DELETE,
            std::ptr::null(),
            CREATE_ALWAYS,
            FILE_ATTRIBUTE_TEMPORARY | FILE_FLAG_DELETE_ON_CLOSE,
            std::ptr::null_mut(),
        )
    };
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_handle(handle as _) })
}

/// Elsewhere: a plain read/write file (the flash concern and the delete-on-close trick are Windows specifics here).
#[cfg(not(windows))]
fn open_backing(path: &Path) -> std::io::Result<File> {
    std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path)
}
