use std::env;
use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;

use tokio::net::UnixStream;


const RUNTIME_DIR_VAR: &str = "XDG_RUNTIME_DIR";
const WAYLAND_DISPLAY_VAR: &str = "WAYLAND_DISPLAY";
const DEFAULT_WAYLAND_DISPLAY: &str = "wayland-0";


#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    MissingEnvVar { name: String },
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e)
                => write!(f, "I/O error: {}", e),
            Self::MissingEnvVar { name }
                => write!(f, "missing required environment variable {:?}", name),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::MissingEnvVar { .. } => None,
        }
    }
}
impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self { Self::Io(value) }
}


pub async fn connect_to_server_env() -> Result<UnixStream, Error> {
    let runtime_dir = env::var_os(RUNTIME_DIR_VAR)
        .ok_or_else(|| Error::MissingEnvVar { name: RUNTIME_DIR_VAR.to_owned() })?;
    let wayland_display = env::var_os(WAYLAND_DISPLAY_VAR)
        .unwrap_or_else(|| OsString::from(DEFAULT_WAYLAND_DISPLAY));
    let mut wayland_display_path = PathBuf::from(&runtime_dir);
    wayland_display_path.push(&wayland_display);

    let sock = UnixStream::connect(&wayland_display_path).await?;
    Ok(sock)
}
