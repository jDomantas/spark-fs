use core::u8;
use io::{self, ReadWriteSeek, SeekFrom};
use path::{self, Path};

const MAX_FILES: usize = 16;
const MAX_FILE_SIZE: u64 = 1024 * 1024;
const FILE_RAW_SIZE: u64 = 10 + ::path::MAX_PATH_LENGTH as u64 + MAX_FILE_SIZE;
const FS_SIZE: u64 = MAX_FILES as u64 * FILE_RAW_SIZE;
const MAX_DESCRIPTORS: usize = 16;

pub struct Fd {
    index: usize,
}

#[derive(Debug, Copy, Clone)]
struct OpenFile {
    used: bool,
    index: usize,
    pos: u64,
    writing: bool,
}

const UNUSED_FD: OpenFile = OpenFile {
    used: false,
    index: 0,
    pos: 0,
    writing: false,
};

#[derive(Debug, Copy, Clone)]
struct FileHeader {
    exists: bool,
    locks: u8,
    len: u64,
    name: Path,
    data: u64,
}

impl FileHeader {
    fn can_write(&self) -> bool {
        self.locks == 0
    }

    fn can_read(&self) -> bool {
        self.locks < u8::MAX
    }

    fn lock_write(&mut self) {
        debug_assert!(self.can_write(), "cannot lock for write");
        self.locks = u8::MAX;
    }

    fn lock_read(&mut self) {
        debug_assert!(self.can_read(), "cannot lock for read");
        self.locks += 1;
    }

    fn unlock_write(&mut self) {
        debug_assert!(self.locks == u8::MAX, "cannot unlock write");
        self.locks = 0;
    }

    fn unlock_read(&mut self) {
        debug_assert!(self.locks > 0, "cannot unlock read");
        self.locks -= 1;
    }
}

const NON_EXISTING_FILE: FileHeader = FileHeader {
    exists: false,
    locks: 0,
    len: 0,
    name: path::EMPTY,
    data: 0,
};

pub struct FileSystem<'a> {
    storage: &'a mut ReadWriteSeek,
    headers: [FileHeader; MAX_FILES as usize],
    descriptors: [OpenFile; MAX_DESCRIPTORS],
}

impl<'a> FileSystem<'a> {
    pub fn new(storage: &'a mut ReadWriteSeek) -> io::Result<Self> {
        let mut fs = FileSystem {
            storage,
            headers: [NON_EXISTING_FILE; MAX_FILES],
            descriptors: [UNUSED_FD; MAX_DESCRIPTORS],
        };
        for i in 0..MAX_FILES {
            let header = fs.read_header(i as u64)?;
            fs.headers[i] = header;
        }
        Ok(fs)
    }

    fn read_header(&mut self, index: u64) -> io::Result<FileHeader> {
        let mut buf = [0; 10 + path::MAX_PATH_LENGTH];
        self.storage.read_exact(&mut buf)?;
        // let name = Path::from_ascii_str(&buf[10..]);
        // if name.is_none() {
        //     panic!("bad path: {:?}", &buf[10..]);
        // }
        Ok(FileHeader {
            exists: buf[0] != 0,
            locks: buf[1],
            len: to_u64(&buf[2..10]),
            name: Path::from_ascii_zero_padded(&buf[10..]).expect("stored bad path"),
            data: file_position(index) + 10 + path::MAX_PATH_LENGTH as u64,
        })
    }

    fn write_header(&mut self, index: u64, header: FileHeader) -> io::Result<()> {
        let mut buf = [0; 11 + path::MAX_PATH_LENGTH];
        buf[0] = header.exists as u8;
        buf[1] = header.locks;
        let mut d = 1;
        for i in 0..8 {
            buf[2 + i] = ((header.len / d) & 0xFF) as u8;
            d = d.wrapping_mul(256);
        }
        let path = header.name.as_slice();
        buf[10..(10 + path.len())].copy_from_slice(path);
        self.storage.seek(SeekFrom::Start(file_position(index)))?;
        self.storage.write_all(&buf)
    }

    pub fn flush_to_storage(&mut self) -> io::Result<()> {
        for i in 0..MAX_FILES {
            let header = self.headers[i];
            self.write_header(i as u64, header)?;
        }
        Ok(())
    }

    fn find_file(&mut self, name: Path) -> Option<(usize, &mut FileHeader)> {
        for (index, file) in self.headers.iter_mut().enumerate() {
            if file.name == name {
                return Some((index, file));
            }
        }
        None
    }

    fn find_empty_slot(&mut self) -> Option<(usize, &mut FileHeader)> {
        for (index, file) in self.headers.iter_mut().enumerate() {
            if !file.exists {
                *file = NON_EXISTING_FILE;
                return Some((index, file));
            }
        }
        None
    }

    fn alloc_descriptor(&mut self) -> Option<usize> {
        for (index, desc) in self.descriptors.iter().enumerate() {
            if !desc.used {
                return Some(index);
            }
        }
        None
    }

    pub fn create(&mut self, path: Path) -> io::Result<Fd> {
        let desc = match self.alloc_descriptor() {
            Some(index) => index,
            None => return Err(io::Error::new(io::ErrorKind::Other, "cannot create")),
        };
        if let Some((index, existing)) = self.find_file(path) {
            if existing.can_write() {
                existing.lock_write();
                existing.len = 0;
                self.descriptors[desc] = OpenFile {
                    used: true,
                    index,
                    pos: 0,
                    writing: true,
                };
                return Ok(Fd { index: desc });
            } else {
                return Err(io::Error::new(io::ErrorKind::Other, "cannot create"));
            }
        }
        if let Some((index, existing)) = self.find_empty_slot() {
            existing.lock_write();
            existing.name = path;
            self.descriptors[desc] = OpenFile {
                used: true,
                index,
                pos: 0,
                writing: true,
            };
            return Ok(Fd { index: desc });
        }
        Err(io::Error::new(io::ErrorKind::Other, "cannot create"))
    }

    pub fn open_read(&mut self, path: Path) -> io::Result<Fd> {
        let desc = match self.alloc_descriptor() {
            Some(index) => index,
            None => return Err(io::Error::new(io::ErrorKind::Other, "cannot open")),
        };
        if let Some((index, existing)) = self.find_file(path) {
            if existing.can_read() {
                existing.lock_read();
                self.descriptors[desc] = OpenFile {
                    used: true,
                    index,
                    pos: 0,
                    writing: false,
                };
                return Ok(Fd { index: desc });
            } else {
                return Err(io::Error::new(io::ErrorKind::Other, "cannot open"));
            }
        }
        Err(io::Error::new(io::ErrorKind::Other, "cannot open"))
    }

    pub fn close(&mut self, fd: Fd) -> io::Result<()> {
        debug_assert!(self.descriptors[fd.index].used, "cannot close unused fd");
        let index = self.descriptors[fd.index].index;
        if self.descriptors[fd.index].writing {
            self.headers[index].unlock_write();
        } else {
            self.headers[index].unlock_read();
        }
        self.descriptors[fd.index].used = false;
        Ok(())
    }

    pub fn get_writer<'b>(&'b mut self, fd: &Fd) -> io::Result<impl io::Write + 'b> {
        let desc = &mut self.descriptors[fd.index];
        debug_assert!(desc.writing && desc.used, "invalid descriptor");
        self.storage.seek(SeekFrom::Start(file_position(desc.index as u64) + desc.pos))?;
        Ok(FsWriter {
            pos: &mut desc.pos,
            len: &mut self.headers[desc.index].len,
            max_len: MAX_FILE_SIZE,
            writer: self.storage,
        })
    }

    pub fn get_reader<'b>(&'b mut self, fd: &Fd) -> io::Result<impl io::Read + 'b> {
        let desc = &mut self.descriptors[fd.index];
        debug_assert!(!desc.writing && desc.used, "invalid descriptor");
        self.storage.seek(SeekFrom::Start(file_position(desc.index as u64) + desc.pos))?;
        Ok(FsReader {
            pos: &mut desc.pos,
            len: self.headers[desc.index].len,
            reader: self.storage,
        })
    }
}

struct FsWriter<'a> {
    pos: &'a mut u64,
    len: &'a mut u64,
    max_len: u64,
    writer: &'a mut ReadWriteSeek,
}

impl<'a> io::Write for FsWriter<'a> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let remaining_space = self.max_len - *self.len;
        let max_write = ::core::cmp::min(buf.len(), remaining_space as usize);
        let written = self.writer.write(&buf[..max_write])?;
        *self.pos += written as u64;
        *self.len += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

struct FsReader<'a> {
    pos: &'a mut u64,
    len: u64,
    reader: &'a mut ReadWriteSeek,
}

impl<'a> io::Read for FsReader<'a> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let remaining_data = self.len - *self.pos;
        let max_read = ::core::cmp::min(buf.len(), remaining_data as usize);
        let read = self.reader.read(&mut buf[..max_read])?;
        *self.pos += read as u64;
        Ok(read)
    }
}

fn to_u64(buf: &[u8]) -> u64 {
    assert_eq!(buf.len(), 8);
    let mut result = 0;
    let mut mul = 1;
    for &byte in buf {
        result += mul * u64::from(byte);
        mul = mul.wrapping_mul(256);
    }
    result
}

fn file_position(index: u64) -> u64 {
    index * FILE_RAW_SIZE
}

pub fn format_storage<T: ReadWriteSeek>(storage: &mut T, len: u64) -> io::Result<()> {
    if len < FS_SIZE {
        panic!(
            "backing storage too small: is {}, should be at least {}",
            len, FS_SIZE,
        );
    }
    for file in 0..MAX_FILES {
        storage.seek(SeekFrom::Start(file_position(file as u64)))?;
        // just clear `exists` flag, leave everything else as-is
        storage.write_all(&[0])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::prelude::v1::*;
    use io::*;
    use super::*;

    #[test]
    fn smoke() {
        let mut storage = empty_backing_storage();
        let _fs = FileSystem::new(&mut storage).expect("failed to create fs");
    }

    #[test]
    fn create() {
        let mut storage = empty_backing_storage();
        let mut fs = FileSystem::new(&mut storage).expect("failed to create fs");
        let path = Path::from_ascii_str(b"foo.txt").unwrap();
        fs.create(path).expect("failed to create");
    }

    #[test]
    fn write_and_read() {
        let mut storage = empty_backing_storage();
        let mut fs = FileSystem::new(&mut storage).expect("failed to create fs");
        let path = Path::from_ascii_str(b"foo.txt").unwrap();
        {
            let fd = {
                let fd = fs.create(path).expect("failed to create");
                let mut writer = fs.get_writer(&fd).expect("failed to get writer");
                writer.write_all(&[1, 2, 3, 4]).expect("failed to write");
                fd
            };
            fs.close(fd).expect("failed to close");
        }
        let fd = fs.open_read(path).expect("failed to open");
        let mut reader = fs.get_reader(&fd).expect("failed to get reader");
        let mut buf = [0; 5];
        let bytes = reader.read(&mut buf).expect("failed to read");
        assert_eq!(bytes, 4, "should have read 4 bytes");
        assert_eq!(buf, [1, 2, 3, 4, 0], "should have read written bytes");
    }

    fn empty_backing_storage() -> impl ReadWriteSeek {
        let mut data = Vec::with_capacity(FS_SIZE as usize);
        for _ in 0..FS_SIZE {
            data.push(0);
        }
        io::Cursor::new(data)
    }
}
