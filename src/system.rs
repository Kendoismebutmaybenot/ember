//! Clipboard access, Start with Windows and Windows theme lookup.

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::System::Registry::*;
use windows::core::{Error, PCWSTR, Result, w};

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_VALUE: PCWSTR = crate::channel::RUN_VALUE;

pub fn set_clipboard(hwnd: HWND, text: &str) -> Result<()> {
    let data: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    unsafe {
        OpenClipboard(Some(hwnd))?;
        let result = (|| {
            EmptyClipboard()?;
            let mem = GlobalAlloc(GMEM_MOVEABLE, data.len() * 2)?;
            let ptr = GlobalLock(mem) as *mut u16;
            if ptr.is_null() {
                let _ = GlobalFree(Some(mem));
                return Err(Error::from_thread());
            }
            std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
            let _ = GlobalUnlock(mem);
            // SetClipboardData transfers memory ownership only on success.
            if let Err(e) = SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(mem.0))) {
                let _ = GlobalFree(Some(mem));
                return Err(e);
            }
            Ok(())
        })();
        let _ = CloseClipboard();
        result
    }
}

pub fn get_clipboard(hwnd: HWND) -> Option<String> {
    unsafe {
        OpenClipboard(Some(hwnd)).ok()?;
        let text = (|| {
            let handle = GetClipboardData(CF_UNICODETEXT.0 as u32).ok()?;
            let mem = HGLOBAL(handle.0);
            let ptr = GlobalLock(mem) as *const u16;
            if ptr.is_null() {
                return None;
            }
            // Other apps may omit the terminating NUL. Limit reads to the allocated block.
            let units = std::slice::from_raw_parts(ptr, GlobalSize(mem) / 2);
            let len = units.iter().position(|&u| u == 0).unwrap_or(units.len());
            let text = String::from_utf16_lossy(&units[..len]);
            let _ = GlobalUnlock(mem);
            Some(text)
        })();
        let _ = CloseClipboard();
        text
    }
}

pub fn autostart_enabled() -> bool {
    unsafe { RegGetValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE, RRF_RT_REG_SZ, None, None, None).is_ok() }
}

pub fn set_autostart(enabled: bool) -> Result<()> {
    unsafe {
        if enabled {
            let exe = std::env::current_exe().map_err(|e| Error::new(windows::core::HRESULT(-1), e.to_string()))?;
            let scoop = std::env::var_os("SCOOP").map(std::path::PathBuf::from);
            let exe = crate::install::autostart_path(&exe, scoop.as_deref());
            let command: Vec<u16> = format!("\"{}\"", exe.display()).encode_utf16().chain(Some(0)).collect();
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                RUN_VALUE,
                REG_SZ.0,
                Some(command.as_ptr().cast()),
                (command.len() * 2) as u32,
            )
            .ok()
        } else {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE).ok()
        }
    }
}

/// Points an existing Start with Windows entry at this program when the file
/// it names is gone, as after `wisp.exe` became `ember.exe` or the folder moved.
/// Without an entry it does nothing, so it never turns autostart on.
pub fn repair_autostart() {
    let mut buffer = [0u16; 1024];
    let mut size = (buffer.len() * 2) as u32;
    let found = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            RUN_VALUE,
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
        .is_ok()
    };
    if !found {
        return;
    }
    let command = String::from_utf16_lossy(&buffer[..(size as usize / 2)]);
    if !std::path::Path::new(command_path(&command)).is_file()
        && let Err(error) = set_autostart(true)
    {
        eprintln!("Start with Windows: {error}");
    }
}

/// The program in a Run command such as `"C:\Apps\ember.exe" --flag`.
fn command_path(command: &str) -> &str {
    let command = command.trim_end_matches('\0').trim();
    match command.strip_prefix('"') {
        Some(rest) => rest.split('"').next().unwrap_or(rest),
        None => command.split(' ').next().unwrap_or(command),
    }
}

pub fn system_uses_light_theme() -> bool {
    let mut value = 0u32;
    let mut size = size_of::<u32>() as u32;
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut value as *mut u32).cast()),
            Some(&mut size),
        )
    };
    result.is_ok() && value != 0
}

#[cfg(test)]
mod tests {
    use super::command_path;

    #[test]
    fn reads_the_program_from_a_run_command() {
        assert_eq!(
            command_path("\"C:\\Apps\\Ember\\wisp.exe\"\0"),
            r"C:\Apps\Ember\wisp.exe"
        );
        assert_eq!(
            command_path(r#""C:\Program Files\Ember\ember.exe" --tray"#),
            r"C:\Program Files\Ember\ember.exe"
        );
        assert_eq!(command_path(r"C:\Apps\ember.exe --tray"), r"C:\Apps\ember.exe");
    }
}
