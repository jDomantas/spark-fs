use core::fmt;

pub const MAX_PATH_LENGTH: usize = 255;

#[derive(Copy, Clone)]
pub struct Path {
    buf: [u8; MAX_PATH_LENGTH],
}

impl Path {
    pub fn from_ascii_str(path: &[u8]) -> Option<Self> {
        if path.len() <= MAX_PATH_LENGTH && path.iter().all(|c| *c != 0) {
            let mut p = Path {
                buf: [0; MAX_PATH_LENGTH],
            };
            p.buf[..(path.len())].copy_from_slice(path);
            Some(p)
        } else {
            None
        }
    }

    pub(crate) fn from_ascii_zero_padded(path: &[u8]) -> Option<Self> {
        if let Some(first_zero) = path.iter().position(|c| *c == 0) {
            Self::from_ascii_str(&path[..first_zero])
        } else {
            Self::from_ascii_str(path)
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        for i in 0..MAX_PATH_LENGTH {
            if self.buf[i] == 0 {
                return &self.buf[..i];
            }
        }
        &self.buf
    }

    pub fn raw_buf(&self) -> &[u8] {
        &self.buf
    }
}

impl PartialEq<Path> for Path {
    fn eq(&self, other: &Path) -> bool {
        for i in 0..MAX_PATH_LENGTH {
            if self.buf[i] != other.buf[i] {
                return false;
            }
        }
        true
    }
}

impl Eq for Path {}

pub const EMPTY: Path = Path {
    buf: [0; MAX_PATH_LENGTH],
};

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_tuple("Path")
            .field(&FmtPathData { path: &self.buf })
            .finish()
    }
}

struct FmtPathData<'a> {
    path: &'a [u8],
}

impl<'a> fmt::Debug for FmtPathData<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for &ch in self.path {
            if ch == 0 {
                break;
            }
            if ch < 0x20 || ch >= 127 || ch == b'\\' {
                write!(f, "\\x{:>02x}", ch)?;
            } else {
                write!(f, "{}", ch as char)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_empty() {
        let path = Path::from_ascii_str(b"").unwrap();
        assert_eq!(format!("{}", path), "Path()");
    }

    #[test]
    fn format_simple() {
        let path = Path::from_ascii_str(b"abcdef").unwrap();
        assert_eq!(format!("{}", path), "Path(abcdef)");
    }

    #[test]
    fn format_escapes() {
        let path = Path::from_ascii_str(b"\x0f\\ foo").unwrap();
        assert_eq!(format!("{}", path), "Path(\\x0f\\x5c foo)");
    }

    #[test]
    fn construct_long() {
        let data = b"123456789012345678901234567890";
        assert!(data.len() > MAX_PATH_LENGTH, "should test with long path");
        assert!(Path::from_ascii_str(data).is_none());
    }

    #[test]
    fn construct_inner_zeros() {
        let data = b"123\0123";
        assert!(Path::from_ascii_str(data).is_none());
    }
}
