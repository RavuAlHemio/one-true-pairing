use std::io;
use std::fmt;


#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    MissingEnvVar { name: String },
    PacketTooLong { actual: usize, maximum: usize },
    PacketTooShort { actual: usize, minimum: usize },
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e)
                => write!(f, "I/O error: {}", e),
            Self::MissingEnvVar { name }
                => write!(f, "missing environment variable {:?}", name),
            Self::PacketTooLong { actual, maximum }
                => write!(f, "packet ({} bytes) too long (maximum {} bytes)", actual, maximum),
            Self::PacketTooShort { actual, minimum }
                => write!(f, "packet ({} bytes) too short (minimum {} bytes)", actual, minimum),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::MissingEnvVar { .. } => None,
            Self::PacketTooLong { .. } => None,
            Self::PacketTooShort { .. } => None,
        }
    }
}
impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self { Self::Io(value) }
}
