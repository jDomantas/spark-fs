pub const MAX_PATH_LENGTH: usize = 20;

#[derive(PartialEq, Eq, Debug, Copy, Clone)]
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

    pub fn as_slice(&self) -> &[u8] {
        for i in 0..MAX_PATH_LENGTH {
            if self.buf[i] == 0 {
                return &self.buf[..i];
            }
        }
        &self.buf
    }
}

pub const EMPTY: Path = Path {
    buf: [0; MAX_PATH_LENGTH],
};
