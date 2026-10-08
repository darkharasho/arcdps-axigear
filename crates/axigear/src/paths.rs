#![cfg(windows)]
//! Where the DLL lives, and `<dll dir>/axigear/` for our data files.

use std::path::PathBuf;

use once_cell::sync::Lazy;

pub fn dll_dir() -> Option<PathBuf> {
    static DLL_DIR: Lazy<Option<PathBuf>> = Lazy::new(resolve_dll_dir);
    DLL_DIR.clone()
}

/// `<exe dir>/addons/axigear`, i.e. next to the DLL; the GW2 folder if the DLL can't be located.
pub fn data_dir() -> PathBuf {
    dll_dir()
        .or_else(|| std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("addons"))))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("axigear")
}

fn resolve_dll_dir() -> Option<PathBuf> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HMODULE;
    use windows::Win32::System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS};
    let mut hmod = HMODULE::default();
    let anchor = resolve_dll_dir as *const () as *const u16;
    unsafe {
        GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, PCWSTR(anchor), &mut hmod).ok()?;
    }
    let mut buf = [0u16; 32768];
    let len = unsafe { GetModuleFileNameW(Some(hmod), &mut buf) } as usize;
    if len == 0 {
        return None;
    }
    PathBuf::from(String::from_utf16(&buf[..len]).ok()?).parent().map(|p| p.to_path_buf())
}
