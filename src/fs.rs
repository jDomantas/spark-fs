use core::u8;
use io::{self, ReadWriteSeek, SeekFrom};
use path::{self, Path};

//const MAX_FILES: usize = 16;
const SECTOR_SIZE: u64 = 1024 * 1024;
//const FS_SIZE: u64 = MAX_FILES as u64 * FILE_RAW_SIZE;
//const MAX_DESCRIPTORS: usize = 16;

const FD_MAGIC_NUMBER: u8 = 0xC4;


#[repr(u8)]
enum FileType {
	folder,
	exec,
	default,
	error,
}

#[repr(u8)]
enum FileFlag {
	special,
	readonly,
	readwrite,
	error,
}

pub struct FileSystem<'a, T: 'a> {
    storage: &'a mut T
}

pub struct FileDescriptor {
	filetype:  FileType,
	fileflag: FileFlag,
	filename: Path,
	activelocks: u8,
	
}


impl<'a, T: ReadWriteSeek + 'a> FileSystem<'a, T> {
    
	fn create_fd(&mut self, ftype: FileType, fflag: FileFlag, name: Path) {
		
		self.storage.writeall(&[FD_MAGIC_NUMBER])?;
		self.storage.writeall(&[ftype as u8])?;
		self.storage.writeall(&[fflag as u8])?;
		self.storage.writeall(&[0])?;
		self.storage.writeall(name.raw_buf())?;
		
	}
	
	pub fn new(&mut self, storage: &'a mut T) -> io::Result<Self> {
        let mut fs = FileSystem {
            storage
        };
		
		self.storage.seek(SeekFrom::Start(0))?;
		self.create_fd(FileType::folder, FileFlag::special, Path::from_ascii_str(b"root"));
        Ok(fs)
    }
	

    fn read_header(&mut self) -> FileDescriptor {
		let mut magicno: [u8;1];
		self.storage.read_exact(&magicno);
		if (magicno[0] != FD_MAGIC_NUMBER) {
			return Err(io::Error::new(io::ErrorKind::FileNotFound, "File header corrupt"));
		}
		
		
		let ftype: FileType;
		
		self.storage.read_exact(&magicno);
        if (FileType::folder as u8 == magicno[0]) {
			ftype = FileType::folder;
		}
		else if (FileType::exec as u8 == magicno[0]) {
			ftype = FileType::exec;
		}
		else if (FileType::default as u8 == magicno[0]) {
			ftype = FileType::default;
		}
		else {
			return Err(io::Error::new(io::ErrorKind::FileNotFound, "File header corrupt"));
		}
		
		let fflag: FileFlag;
		
		self.storage.read_exact(&magicno);
		if (FileFlag::readwrite as u8 == magicno[0]) {
			fflag = FileFlag::readwrite;
		}
		else if (FileFlag::readonly as u8 == magicno[0]) {
			fflag = FileFlag::readonly;
		}
		else if (FileFlag::special as u8 == magicno[0]) {
			fflag = FileFlag::special;
		}
		else {
			return Err(io::Error::new(io::ErrorKind::FileNotFound, "File header corrupt"));
		}
		
		self.storage.read(&magicno);
		
		let fname = [0;Path::MAX_PATH_LENGTH];
		
		self.storage.read_exact(&fname);
		
		FileDescriptor{
			filetype: ftype,
			fileflag: fflag,
			filename : fname,
			activelocks: (magicno[0])
		}
    }

    fn write_header(&mut self, index: u64, header: FileDescriptor) -> io::Result<()> {
        
        self.storage.writeall(&[FD_MAGIC_NUMBER])?;
		self.storage.writeall(&[header.filetype as u8])?;
		self.storage.writeall(&[header.fileflag as u8])?;
		self.storage.writeall(&[0])?;
		self.storage.writeall(header.filename.raw_buf())?;
        
    }

    /*pub fn flush_to_storage(&mut self) -> io::Result<()> {
        for i in 0..MAX_FILES {
            let header = self.headers[i];
            self.write_header(i as u64, header)?;
        }
        Ok(())
    }*/

	/*
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
	*/
	

    pub fn create(&mut self, path: Path) -> io::Result<Fd> {
        let pt: Path = path.as_slice();
        let mut i = 0;
        self.storage.seek(SeekFrom::Start(0));
        
        while (i < path.len()) {
            self.storage.seek(SeekFrom::Start(0));
            /////////////////////////////////////////////////////////////////////////////////////////
        }
    }

    pub fn open_read(&mut self, path: Path) -> io::Result<Fd> {
        let desc = match self.alloc_descriptor() {
            Some(index) => index,
            None => return Err(io::Error::new(io::ErrorKind::Other, "cannot open")),
        };
        if let Some((index, existing)) = self.find_file(path) {
            if existing.can_read() {
                existing.lock_read();
				
				/*
                self.descriptors[desc] = OpenFile {
                    used: true,
                    index,
                    pos: 0,
                    writing: false,
                };
				*/
				
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
        self.storage
            .seek(SeekFrom::Start(file_position(desc.index as u64) + desc.pos))?;
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
        self.storage
            .seek(SeekFrom::Start(file_position(desc.index as u64) + desc.pos))?;
        Ok(FsReader {
            pos: &mut desc.pos,
            len: self.headers[desc.index].len,
            reader: self.storage,
        })
    }

    pub fn list_files<'b>(&'b mut self) -> impl Iterator<Item = Path> + 'b {
        FileIterator {
            headers: &self.headers,
        }
    }

    pub fn inner_mut(&mut self) -> &mut T {
        self.storage
    }
}

struct FsWriter<'a, T: 'a> {
    pos: &'a mut u64,
    len: &'a mut u64,
    max_len: u64,
    writer: &'a mut T,
}

impl<'a, T: ReadWriteSeek + 'a> io::Write for FsWriter<'a, T> {
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

struct FsReader<'a, T: 'a> {
    pos: &'a mut u64,
    len: u64,
    reader: &'a mut T,
}

impl<'a, T: ReadWriteSeek + 'a> io::Read for FsReader<'a, T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let remaining_data = self.len - *self.pos;
        let max_read = ::core::cmp::min(buf.len(), remaining_data as usize);
        let read = self.reader.read(&mut buf[..max_read])?;
        *self.pos += read as u64;
        Ok(read)
    }
}

struct FileIterator<'a> {
    headers: &'a [FileHeader],
}

impl<'a> Iterator for FileIterator<'a> {
    type Item = Path;

    fn next(&mut self) -> Option<Path> {
        while let Some(header) = self.headers.get(0) {
            self.headers = &self.headers[1..];
            if header.exists {
                return Some(header.name);
            }
        }
        None
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
    
	/*
	if len < FS_SIZE {
        panic!(
            "backing storage too small: is {}, should be at least {}",
            len, FS_SIZE,
        );
    }
	*/
	
	/*
    for file in 0..MAX_FILES {
        storage.seek(SeekFrom::Start(file_position(file as u64)))?;
        // just clear `exists` flag, leave everything else as-is
        storage.write_all(&[0])?;
    }
	*/
	
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use io::*;
    use std::prelude::v1::*;

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

    #[test]
    fn list_files() {
        let mut storage = empty_backing_storage();
        let mut fs = FileSystem::new(&mut storage).expect("failed to create fs");
        let path1 = Path::from_ascii_str(b"foo.txt").unwrap();
        fs.create(path1).expect("failed to create file");
        let path2 = Path::from_ascii_str(b"bar.txt").unwrap();
        fs.create(path2).expect("failed to create file");
        let files = fs.list_files().collect::<Vec<_>>();
        assert_eq!(files.len(), 2, "should be 2 files");
        assert!(files.iter().any(|p| *p == path1));
        assert!(files.iter().any(|p| *p == path2));
    }

    fn empty_backing_storage() -> impl ReadWriteSeek {
        let mut data = Vec::with_capacity(FS_SIZE as usize);
        for _ in 0..FS_SIZE {
            data.push(0);
        }
        io::Cursor::new(data)
    }
}
