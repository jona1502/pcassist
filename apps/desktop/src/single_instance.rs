//! Single-instance guard via named pipe `\\.\pipe\pcassist-<user>`.
//! Erste Instanz erzeugt die Pipe (FIRST_PIPE_INSTANCE); weitere Starts senden "open".

use assistant_core::Command;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::os::windows::io::FromRawHandle;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;
use windows_sys::Win32::Foundation::{
    GetLastError, ERROR_NO_DATA, ERROR_PIPE_CONNECTED, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_INBOUND};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
};

pub enum Role {
    /// Erste Instanz; Commands anderer Starts kommen ueber den Receiver.
    Primary(Receiver<Command>),
    /// Laufende Instanz wurde benachrichtigt; beenden.
    Secondary,
}

pub fn pipe_name() -> String {
    let user = std::env::var("USERNAME").unwrap_or_default();
    format!(r"\\.\pipe\pcassist-{user}")
}

pub fn acquire() -> Role {
    acquire_named(&pipe_name())
}

fn acquire_named(name: &str) -> Role {
    let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    // SAFETY: wide is NUL-terminated and outlives the call; null security attrs = defaults.
    let handle = unsafe {
        CreateNamedPipeW(
            wide.as_ptr(),
            PIPE_ACCESS_INBOUND | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            0,
            256,
            0,
            std::ptr::null(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        notify_running(name);
        return Role::Secondary;
    }
    let (tx, rx) = channel();
    let raw = handle as usize;
    std::thread::spawn(move || {
        let h = raw as windows_sys::Win32::Foundation::HANDLE;
        loop {
            // SAFETY: h is a valid pipe handle owned by this thread for the process lifetime.
            let ok = unsafe { ConnectNamedPipe(h, std::ptr::null_mut()) };
            // Client kann schon verbunden (und wieder weg) sein; Daten bleiben lesbar.
            let connected = ok != 0
                || matches!(
                    unsafe { GetLastError() },
                    ERROR_PIPE_CONNECTED | ERROR_NO_DATA
                );
            if connected {
                // ManuallyDrop: Handle bleibt fuer die naechste Verbindung offen.
                let file = std::mem::ManuallyDrop::new(unsafe { File::from_raw_handle(h as _) });
                let mut line = String::new();
                let _ = BufReader::new(&*file).read_line(&mut line);
                if line.trim() == "open" && tx.send(Command::OpenPanel).is_err() {
                    return;
                }
            }
            unsafe { DisconnectNamedPipe(h) };
        }
    });
    Role::Primary(rx)
}

fn notify_running(name: &str) {
    for _ in 0..20 {
        if let Ok(mut f) = OpenOptions::new().write(true).open(name) {
            let _ = f.write_all(b"open\n");
            return;
        }
        // Pipe kurz busy (zwischen Verbindungen): erneut versuchen.
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_acquire_notifies_primary() {
        let name = format!(r"\\.\pipe\pcassist-test-{}", std::process::id());
        let Role::Primary(rx) = acquire_named(&name) else {
            panic!("expected primary");
        };
        assert!(matches!(acquire_named(&name), Role::Secondary));
        let cmd = rx.recv_timeout(Duration::from_secs(3)).unwrap();
        assert!(matches!(cmd, Command::OpenPanel));
    }
}
