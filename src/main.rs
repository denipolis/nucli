#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(not(all(windows, feature = "split")))]
fn main() {
    nucli::run();
}

#[cfg(all(windows, feature = "split"))]
fn main() {
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    use windows::core::{HSTRING, s};
    let path = std::env::current_exe().expect("executable path").with_file_name("nucli.dll");
    unsafe {
        let library = LoadLibraryW(&HSTRING::from(path.as_path())).expect("nucli.dll");
        let entry = GetProcAddress(library, s!("nucli_main")).expect("nucli_main");
        std::mem::transmute::<unsafe extern "system" fn() -> isize, unsafe extern "C" fn()>(entry)();
    }
}
