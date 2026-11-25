#[cfg(windows)]
pub struct Instance(windows::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(windows)]
pub fn acquire() -> Result<Option<Instance>, String> {
    use windows::{
        core::{w, PCWSTR},
        Win32::{
            Foundation::{GetLastError, ERROR_ALREADY_EXISTS},
            System::Threading::CreateMutexW,
            UI::WindowsAndMessaging::{FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE},
        },
    };

    unsafe {
        let handle = CreateMutexW(None, false, w!("Local\\getScreenShare-tauri"))
            .map_err(|_| "Não foi possível iniciar o controle de instância única.".to_string())?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = windows::Win32::Foundation::CloseHandle(handle);
            if let Ok(window) = FindWindowW(PCWSTR::null(), w!("getScreenShare")) {
                let _ = ShowWindow(window, SW_RESTORE);
                let _ = SetForegroundWindow(window);
            }
            return Ok(None);
        }
        Ok(Some(Instance(handle)))
    }
}

#[cfg(unix)]
pub struct Instance {
    _lock: std::fs::File,
    socket_path: std::path::PathBuf,
}

#[cfg(unix)]
impl Drop for Instance {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

#[cfg(unix)]
static ACTIVATION_SOCKET: std::sync::OnceLock<std::os::unix::net::UnixDatagram> =
    std::sync::OnceLock::new();

#[cfg(unix)]
pub fn acquire() -> Result<Option<Instance>, String> {
    use std::{
        fs::OpenOptions,
        os::{fd::AsRawFd, unix::net::UnixDatagram},
        path::PathBuf,
        thread,
        time::Duration,
    };

    let directory = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = directory.join(format!("getscreenshare-{}.lock", unsafe {
        libc::geteuid()
    }));
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|_| "Não foi possível iniciar o controle de instância única.".to_string())?;
    let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if result != 0 {
        if let Ok(socket) = UnixDatagram::unbound() {
            for _ in 0..10 {
                if socket.send_to(b"show", path.with_extension("sock")).is_ok() {
                    break;
                }
                thread::sleep(Duration::from_millis(50));
            }
        }
        return Ok(None);
    }
    let socket_path = path.with_extension("sock");
    let _ = std::fs::remove_file(&socket_path);
    let socket = UnixDatagram::bind(&socket_path)
        .map_err(|_| "Não foi possível iniciar o controle da janela.".to_string())?;
    socket
        .set_read_timeout(Some(Duration::from_millis(500)))
        .map_err(|_| "Não foi possível iniciar o controle da janela.".to_string())?;
    ACTIVATION_SOCKET
        .set(socket)
        .map_err(|_| "O controle da janela já está ativo.".to_string())?;
    Ok(Some(Instance {
        _lock: file,
        socket_path,
    }))
}

#[cfg(unix)]
pub fn listen_for_activation(on_activate: impl Fn() + Send + 'static) {
    use std::thread;

    if let Some(socket) = ACTIVATION_SOCKET.get() {
        if let Ok(listener) = socket.try_clone() {
            thread::spawn(move || {
                let mut buffer = [0u8; 16];
                loop {
                    match listener.recv(&mut buffer) {
                        Ok(size) if &buffer[..size] == b"show" => on_activate(),
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                || error.kind() == std::io::ErrorKind::TimedOut => {}
                        Err(_) => break,
                        _ => {}
                    }
                }
            });
        }
    }
}

#[cfg(windows)]
pub fn listen_for_activation(_on_activate: impl Fn() + Send + 'static) {}
