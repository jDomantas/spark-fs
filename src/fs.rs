use core::u8;
use io::{self, ReadWriteSeek, SeekFrom};
use path::{self, Path};

//const MAX_FILES: usize = 16;
const SECTOR_SIZE: u64 = 4096;
//const FS_SIZE: u64 = MAX_FILES as u64 * FILE_RAW_SIZE;
//const MAX_DESCRIPTORS: usize = 16;

const FD_MAGIC_NUMBER: u8 = 0xC4;
const MAX_SECTORS: usize = 1048576;
const U32_MAX: u32 = 4294967295;
const MAX_OPEN_FILES: u8 = 128;


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

#[repr(u8)]
enum OpenMode {
    read,
    readwrite,
    write,
    append,
    overwrite,
    error,
}

pub struct FileSystem<'a, T: 'a> {
    storage: &'a mut T,
    sectors: [bool;MAX_SECTORS],
    capacity: u64,
    current_sector: u32,
    current_position: usize,
    file_handles: [FileHandle;MAX_OPEN_FILES],
    handle_usage: [bool;MAX_OPEN_FILES],
}

pub struct FileDescriptor {
	filetype:  FileType,
	fileflag: FileFlag,
	filename: Path,
	active_locks: u8,
	writelock: bool,
}

#[derive(Copy, Clone)]
pub struct FileHandle {
    current_position: usize,
    open_mode: OpenMode,
    fdesc: FileDescriptor,
    handle_no: u8
}

impl Clone for FileHandle {
    fn clone(&self) -> FileHandle {
        FileHandle {
            current_position: self.current_position,
            open_mode: self.open_mode,
            fdesc: FileDescriptor {
                filetype: self.fdesc.filetype;
                fileflag: self.fdesc.fileflag;
                filename: self.fdesc.filename;
                active_locks: self.fdesc.active_locks;
                writelock: self.fdesc.writelock;
            }
        }
    }
}


impl<'a, T: ReadWriteSeek + 'a> FileSystem<'a, T> {
    
	fn create_fd(&mut self, ftype: FileType, fflag: FileFlag, name: Path) {
		
		self.storage.writeall(&[FD_MAGIC_NUMBER])?;
		self.storage.writeall(&[ftype as u8])?;
		self.storage.writeall(&[fflag as u8])?;
		self.storage.writeall(&[0])?;
		self.storage.writeall(name.raw_buf())?;
		
	}
	
	pub fn new(&mut self, storage: &'a mut T, size: u64) -> io::Result<Self> {
        let mut fs = FileSystem {
            self.storage: storage,
            sectors: [false;MAX_SECTORS],
            handle_usage: [false;MAX_OPEN_FILES],
            current_sector: 0,
            current_position: 0,
            capacity: size,
            file_handles: [
                FileHandle {
                    current_position: 0,
                    open_mode: OpenMode::error,
                    fdesc: FileDescriptor{
                        filetype:  FileType::error,
                        fileflag: FileFlag::error,
                        filename: Path::from_ascii_str(b""),
                        active_locks: 0,
                        writelock: false,
                    },
                    handle_no: u8
                };
                MAX_OPEN_FILES
            ]
        };
        
        self.sectors[0] = true;
        
		
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
			return Err(io::Error::new(io::ErrorKind::Other, "File header corrupt"));
		}
		
		self.storage.read(&magicno);
		
		let fname = [0;Path::MAX_PATH_LENGTH];
		
		self.storage.read_exact(&fname);
		
		FileDescriptor {
			filetype: ftype,
			fileflag: fflag,
			filename : fname,
			active_locks: (magicno[0])
		}
    }
    
    pub fn open_file(&mut self, path: Path, mode: OpenMode) -> io::Result<FileHandle> {
        let sect = navigate(path);
        if (sect == MAX_SECTORS) {
            Err(io::Error::new(io::ErrorKind::FileNotFound, "file not found"));
        }
        self.storage.seek(SeekFrom::Start(SECTOR_SIZE * sect + 4));
        let descr = read_header();
        match (mode) {
            OpenMode::Read => {
                if (!(descr.writelock)) {
                    descr.active_locks++;
                    Ok(create_handle(descr, mode));
                }
                else {
                    Err(io::Error::new(io::ErrorKind::Other, "this file is already open for writing"));
                }
            }
            
            OpenMode::ReadWrite => {
                if (descr.writelock) {
                    Err(io::Error::new(io::ErrorKind::Other, "this file is already open for writing"));
                }
                if (descr.active_locks > 0) {
                    Err(io::Error::new(io::ErrorKind::Other, "this file is already open for reading"));
                }
                descr.active_locks++;
                descr.writelock = true;
                Ok(create_handle(descr, mode));
            }
            
            OpenMode::Write => {
                if (descr.writelock) {
                    Err(io::Error::new(io::ErrorKind::Other, "this file is already open for writing"));
                }
                if (descr.active_locks > 0) {
                    Err(io::Error::new(io::ErrorKind::Other, "this file is already open for reading"));
                }
                descr.writelock = true;
                Ok(create_handle(descr, mode));
            }
            
            
        }
    }
    
    /*
    enum OpenMode {
        read,
        readwrite,
        write,
        append,
        overwrite,
    }
    */
    
    fn create_handle(&mut self, fdesc: FileDescriptor, mode: OpenMode) -> u8 {
        let i:u8 = 0;
        while (i < MAX_OPEN_FILES) {
            if (!(self.handle_usage[i])) {
                self.handle_usage[i] = true;
                self.file_handles[i].current_position = 0;
                self.file_handles[i].open_mode = mode;
                self.file_handles[i].fdesc = fdesc;
                return i;
            }
            return MAX_OPEN_FILES;
        }
    }
    
    fn delete_handle(&mut self, hndl: FileHandle) {
        if (self.handle_usage[i]) {
            self.file_handles[i].filetype = FileType::error;
            self.file_handles[i].fileflag = FileFlag::error;
            self.file_handles[i].filename = Path::from_ascii_str(b"");
            self.handle_usage[i] = false;
            return;
        }
    }
    
    /*
    FileHandle {
            current_position: self.current_position,
            fdesc: FileDescriptor {
                filetype: self.fdesc.filetype;
                fileflag: self.fdesc.fileflag;
                filename: self.fdesc.filename;
                active_locks: self.fdesc.active_locks;
            }
        }
    */
    
    fn write_auto(&mut self, buf: &[u8]) {
        if (len(buf) > SECTOR_SIZE - (self.current_position % SECTOR_SIZE)) {
            let buf1 = buf[(SECTOR_SIZE - (self.current_position % SECTOR_SIZE))..];
            self.storage.writeall(&(buf[..(SECTOR_SIZE - (self.current_position % SECTOR_SIZE))]));
            self.storage.seek(SeekFrom::Start(self.current_sector * SECTOR_SIZE));
            self.current_position = self.current_sector * SECTOR_SIZE;
            let new_sector = get_valid_sector();
            write_shorthead(new_sector);
            self.current_sector = new_sector;
            self.current_position = new_sector * SECTOR_SIZE;
            self.storage.seek(SeekFrom::Start(self.current_position));
            write_auto(&buf1);
            return;
        }
        self.storage.writeall(buf);
        self.current_position += len(buf);
        return;
    }
    
    fn get_valid_sector(&mut self) -> io::Result<u32> {
        let i: u32 = 0;
        while(i < MAX_SECTORS && i < (self.capacity / SECTOR_SIZE)) {
            if (!(sectors[i])) {
                sectors[i] = true;
                Ok(i as u32);
            }
            i++;
        }
        Err(io::Error::new(io::ErrorKind::OutOfSpace, "File system ran out of space"));
    }

    fn write_header(&mut self, index: u64, header: FileDescriptor)  {
        write_shorthead(U32_MAX);
        
        self.storage.writeall(&[FD_MAGIC_NUMBER])?;
		self.storage.writeall(&[header.filetype as u8])?;
		self.storage.writeall(&[header.fileflag as u8])?;
		self.storage.writeall(&[0])?;
		self.storage.writeall(header.filename.raw_buf())?;
    }
    
    fn write_shorthead(&mut self, x: u32) {
        let buf: [u8;4] = transform_u32_to_array_of_u8(x);
        self.current_position += 4;
        self.storage.writeall(&buf);
    }
    
    fn read_shorthead(&mut self) -> u32 {
        let buf: [u8;4] = [0;4];
        self.storage.read_exact(&buf);
        self.current_position += 4;
        return (transform_array_of_u8_to_u32(buf));
    }
    
    fn transform_u32_to_array_of_u8(x:u32) -> [u8;4] {
        let b1 : u8 = ((x >> 24) & 0xff) as u8;
        let b2 : u8 = ((x >> 16) & 0xff) as u8;
        let b3 : u8 = ((x >> 8) & 0xff) as u8;
        let b4 : u8 = (x & 0xff) as u8;
        return [b1, b2, b3, b4]
    }
    
    fn transform_array_of_u8_to_u32(x:[u8;4]) -> u32 {
        let y: u32 = (x[0] as u32) << 24;
        y = y & ((x[1] as u32) << 16);
        y = y & ((x[2] as u32) << 8);
        y = y & (x[3] as u32);
        return y;
    }
    
    fn free_children(&mut self, sector: u32) {
        self.storage.seek(SeekFrom::Start(SECTOR_SIZE * sector));
        let pointer = read_shorthead();
        if (pointer == MAX_SECTORS) {
            return;
        }
        free_auto(pointer);
        return;
    }
    
    fn free_auto(&mut self, sector: u32) {
        self.storage.seek(SeekFrom::Start(SECTOR_SIZE * sector));
        let pointer = read_shorthead();
        if (pointer == MAX_SECTORS) {
            self.sectors[sector] = false;
            return;
        }
        self.sectors[sector] = false;
        free_auto(pointer);
        return;
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
        
        if (pt[0] != b'/') {
            return Err(io::Error::new(io::ErrorKind::Other, "cannot create - bad path"))
        }
        while (i < path.len()) {
            self.storage.seek(SeekFrom::Start(0));
            if (pt[0] != b'/')
        }
    }
    
    fn navigate(&mut self, path: Path) -> u32 {
        self.storage.seek(SeekFrom::Start(0));
        let fd: FileDescriptor = read_header();
        // TODO - implement navigation. Returns sector number, or MAX_SECTORS if not found.
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
