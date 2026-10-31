//! Shared by the app and the `makora-bridge` binary (included there via `#[path]`), so it may
//! only depend on `std`, `interprocess` and `windows-sys`.

use std::io::{self, Read};

use interprocess::local_socket::Name;

pub const HOST_NAME: &str = "dev.phucvinh57.makora";
pub const MAX_FRAME_BYTES: u32 = 64 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub enum Frame {
    Message(Vec<u8>),
    /// Larger than [`MAX_FRAME_BYTES`]; its payload was read and discarded to stay in sync.
    Oversized,
}

/// Reads one native-messaging frame: a native-endian `u32` length, then that many bytes.
/// `Ok(None)` is a clean end of stream between frames; a stream cut mid-frame is an error.
pub fn read_frame(reader: &mut impl Read) -> io::Result<Option<Frame>> {
    let mut prefix = [0u8; 4];
    let mut filled = 0;
    while filled < prefix.len() {
        match reader.read(&mut prefix[filled..]) {
            Ok(0) if filled == 0 => return Ok(None),
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(n) => filled += n,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            Err(err) => return Err(err),
        }
    }
    let len = u32::from_ne_bytes(prefix);
    if len > MAX_FRAME_BYTES {
        let skipped = io::copy(&mut reader.take(u64::from(len)), &mut io::sink())?;
        if skipped < u64::from(len) {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        return Ok(Some(Frame::Oversized));
    }
    let mut payload = vec![0u8; len as usize];
    reader.read_exact(&mut payload)?;
    Ok(Some(Frame::Message(payload)))
}

/// Where the app listens for the bridge. Per user: a socket in a private directory on Unix, a
/// pipe named after the user's SID on Windows.
#[cfg(unix)]
pub fn socket_dir() -> io::Result<std::path::PathBuf> {
    socket_dir_from(|key| std::env::var_os(key).filter(|value| !value.is_empty()))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "neither XDG_RUNTIME_DIR nor HOME is set"))
}

#[cfg(unix)]
pub fn socket_dir_from(var: impl Fn(&str) -> Option<std::ffi::OsString>) -> Option<std::path::PathBuf> {
    use std::path::PathBuf;

    if let Some(runtime) = var("XDG_RUNTIME_DIR") {
        return Some(PathBuf::from(runtime).join("makora"));
    }
    let home = PathBuf::from(var("HOME")?);
    let data = if cfg!(target_os = "macos") {
        home.join("Library/Application Support")
    } else {
        var("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".local/share"))
    };
    Some(data.join(HOST_NAME).join("bridge"))
}

#[cfg(unix)]
pub fn socket_name() -> io::Result<Name<'static>> {
    use interprocess::local_socket::{GenericFilePath, ToFsName};

    Ok(socket_dir()?.join("bridge.sock").to_fs_name::<GenericFilePath>()?.into_owned())
}

#[cfg(windows)]
pub fn socket_name() -> io::Result<Name<'static>> {
    use interprocess::local_socket::{GenericNamespaced, ToNsName};

    Ok(format!("{HOST_NAME}-{}", user_sid()?).to_ns_name::<GenericNamespaced>()?.into_owned())
}

#[cfg(windows)]
pub fn user_sid() -> io::Result<String> {
    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE};
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows_sys::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut len = 0u32;
        GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut len);
        let mut buffer = vec![0u64; (len as usize).div_ceil(8)];
        let ok = GetTokenInformation(token, TokenUser, buffer.as_mut_ptr().cast(), len, &mut len);
        let token_error = io::Error::last_os_error();
        CloseHandle(token);
        if ok == 0 {
            return Err(token_error);
        }
        let user = &*buffer.as_ptr().cast::<TOKEN_USER>();
        let mut wide: *mut u16 = std::ptr::null_mut();
        if ConvertSidToStringSidW(user.User.Sid, &mut wide) == 0 {
            return Err(io::Error::last_os_error());
        }
        let len = (0..).take_while(|&i| *wide.add(i) != 0).count();
        let sid = String::from_utf16_lossy(std::slice::from_raw_parts(wide, len));
        LocalFree(wide.cast());
        Ok(sid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn framed(payload: &[u8]) -> Vec<u8> {
        let mut bytes = (payload.len() as u32).to_ne_bytes().to_vec();
        bytes.extend_from_slice(payload);
        bytes
    }

    #[test]
    fn reads_frames_in_order_then_clean_eof() {
        let mut input = framed(b"{\"a\":1}");
        input.extend(framed(b"{}"));
        let mut reader = input.as_slice();

        assert_eq!(read_frame(&mut reader).unwrap(), Some(Frame::Message(b"{\"a\":1}".to_vec())));
        assert_eq!(read_frame(&mut reader).unwrap(), Some(Frame::Message(b"{}".to_vec())));
        assert_eq!(read_frame(&mut reader).unwrap(), None);
    }

    #[test]
    fn accepts_a_frame_of_exactly_the_cap() {
        let payload = vec![b'x'; MAX_FRAME_BYTES as usize];
        let input = framed(&payload);

        assert_eq!(read_frame(&mut input.as_slice()).unwrap(), Some(Frame::Message(payload)));
    }

    #[test]
    fn skips_an_oversized_frame_and_keeps_reading() {
        let mut input = framed(&vec![b'x'; MAX_FRAME_BYTES as usize + 1]);
        input.extend(framed(b"{}"));
        let mut reader = input.as_slice();

        assert_eq!(read_frame(&mut reader).unwrap(), Some(Frame::Oversized));
        assert_eq!(read_frame(&mut reader).unwrap(), Some(Frame::Message(b"{}".to_vec())));
    }

    #[test]
    fn truncated_prefix_is_an_error() {
        let err = read_frame(&mut [7u8, 0].as_slice()).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn truncated_payload_is_an_error() {
        let mut input = framed(b"{\"a\":1}");
        input.truncate(input.len() - 2);

        let err = read_frame(&mut input.as_slice()).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn truncated_oversized_payload_is_an_error() {
        let mut input = framed(&vec![b'x'; MAX_FRAME_BYTES as usize + 1]);
        input.truncate(100);

        let err = read_frame(&mut input.as_slice()).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[cfg(unix)]
    #[test]
    fn socket_dir_prefers_the_runtime_dir() {
        let dir = socket_dir_from(|key| match key {
            "XDG_RUNTIME_DIR" => Some("/run/user/1000".into()),
            "HOME" => Some("/home/me".into()),
            _ => None,
        });
        assert_eq!(dir, Some("/run/user/1000/makora".into()));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn socket_dir_falls_back_to_the_app_data_dir() {
        let dir = socket_dir_from(|key| (key == "HOME").then(|| "/home/me".into()));
        assert_eq!(dir, Some("/home/me/.local/share/dev.phucvinh57.makora/bridge".into()));
    }
}
