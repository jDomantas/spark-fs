use core::u8;
use io::{self, ReadWriteSeek, SeekFrom};
use path::{self, Path};

//const MAX_FILES: usize = 16;
const SECTOR_SIZE: u64 = 4096;
//const FS_SIZE: u64 = MAX_FILES as u64 * FILE_RAW_SIZE;
//const MAX_DESCRIPTORS: usize = 16;

const FD_MAGIC_NUMBER: u8 = 0xC4;
const MAX_SECTORS: u32 = 1048576;
const U32_MAX: u32 = 4294967295;
const MAX_OPEN_FILES: u8 = 128;


#[repr(u8)]
#[derive(Debug, Copy, Clone)]
enum FileType {
	folder,
	exec,
	default,
	error,
}

#[repr(u8)]
#[derive(Debug, Copy, Clone)]
enum FileFlag {
	special,
	readonly,
	readwrite,
	error,
}

#[repr(u8)]
#[derive(Debug, Copy, Clone)]
enum OpenMode {
    read,
    readwrite,
    write,
    append,
    error,
}

pub struct FileSystem<'a, T: 'a> {
    storage: &'a mut T,
    sectors: [bool;MAX_SECTORS as usize],
    capacity: u32,
    current_sector: u32,
    //current_position: u32,
    file_handles: [FileHandle;MAX_OPEN_FILES as usize],
    handle_usage: [bool;MAX_OPEN_FILES as usize],
}

#[derive(Copy)]
pub struct FileDescriptor {
	filetype:  FileType,
	fileflag: FileFlag,
	end_position: u32,
	filename: Path,
	active_locks: u8,
	writelock: bool,
}

#[derive(Copy)]
pub struct FileHandle {
    current_position: usize,
    open_mode: OpenMode,
    fdesc: FileDescriptor,
    handle_no: u8
}

impl Clone for FileDescriptor {
    fn clone(&self) -> FileDescriptor {
        FileDescriptor {
            filetype: self.filetype,
            fileflag: self.fileflag,
            end_position: self.end_position,
            filename: self.filename,
            active_locks: self.active_locks,
            writelock: self.writelock,
        }
    }
}

impl Clone for FileHandle {
    fn clone(&self) -> FileHandle {
        FileHandle {
            current_position: self.current_position,
            open_mode: self.open_mode,
            fdesc: FileDescriptor {
                filetype: self.fdesc.filetype,
                fileflag: self.fdesc.fileflag,
                filename: self.fdesc.filename,
                end_position: self.fdesc.end_position,
                active_locks: self.fdesc.active_locks,
                writelock: self.fdesc.writelock,
            }
        }
    }
}


impl<'a, T: ReadWriteSeek + 'a> FileSystem<'a, T> {

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

    fn write_shorthead(&mut self, x: u32) {
        let buf: [u8;4] = transform_u32_to_array_of_u8(x);
        self.current_position += 4;
        self.storage.writeall(&buf);
    }
    
    fn read_shorthead(&mut self) -> u32 {
        let buf: [u8;4] = [0;4];
        self.storage.read_exact(&buf);
        self.current_position += 4;
        return transform_array_of_u8_to_u32(buf);
    }
    
	fn write_header(&mut self, fd: FileDescriptor) {
		
		write_shorthead(U32_MAX);
		self.storage.writeall(&[FD_MAGIC_NUMBER])?;
		self.storage.writeall(&[fd.filetype as u8])?;
		self.storage.writeall(&[fd.fileflag as u8])?;
		self.storage.writeall(&[0])?;
		self.storage.writeall(&(transform_u32_to_array_of_u8(fd.end_position)))?;
		if fd.writelock {
            self.storage.writeall(&[0])?;
		}
		else {
            self.storage.writeall(&[1])?;
		}
		self.storage.writeall(name.raw_buf())?;
		
		self.current_position += 9 + Path::MAX_PATH_LENGTH;
	}
	
	pub fn new(&mut self, storage: &'a mut T, size: u64) -> io::Result<Self> {
        let mut fs = FileSystem {
            storage: storage,
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
		if magicno[0] != FD_MAGIC_NUMBER {
			return Err(io::Error::new(io::ErrorKind::FileNotFound, "File header corrupt"));
		}
		
		
		let ftype: FileType;
		
		self.storage.read_exact(&magicno);
        if FileType::folder as u8 == magicno[0] {
			ftype = FileType::folder;
		}
		else if FileType::exec as u8 == magicno[0] {
			ftype = FileType::exec;
		}
		else if FileType::default as u8 == magicno[0] {
			ftype = FileType::default;
		}
		else {
			return Err(io::Error::new(io::ErrorKind::FileNotFound, "File header corrupt"));
		}
		
		let fflag: FileFlag;
		
		self.storage.read_exact(&magicno);
		if FileFlag::readwrite as u8 == magicno[0] {
			fflag = FileFlag::readwrite;
		}
		else if FileFlag::readonly as u8 == magicno[0] {
			fflag = FileFlag::readonly;
		}
		else if FileFlag::special as u8 == magicno[0] {
			fflag = FileFlag::special;
		}
		else {
			return Err(io::Error::new(io::ErrorKind::Other, "File header corrupt"));
		}
		
		self.storage.read(&magicno);
		
		
        let end_pos_buf: [u8;4] = [0;4];
        self.storage.read_exact(&end_pos_buf);
        
        let writelock: [u8;1] = [0;1];
        self.storage.read_exact(&writelock);
		
		let fname = [0;Path::MAX_PATH_LENGTH];
		
		self.storage.read_exact(&fname);
		
		let wlock: bool = false;
		if writelock == 1 {
            wlock = true;
		}
		let end_pos: u32 = transform_array_of_u8_to_u32(end_pos_buf);
		FileDescriptor {
			filetype: ftype,
			fileflag: fflag,
			filename : fname,
			active_locks: (magicno[0]),
			writelock: wlock,
			end_position: end_pos,
		};
		
		self.current_position += 9 + Path::MAX_PATH_LENGTH;
    }
    
    pub fn open_file(&mut self, path: Path, mode: OpenMode) -> io::Result<FileHandle> {
        let sect = navigate(path);
        if sect == MAX_SECTORS {
            Err(io::Error::new(io::ErrorKind::FileNotFound, "file not found"));
        }
        self.storage.seek(SeekFrom::Start(SECTOR_SIZE * sect + 4));
        match mode {
            OpenMode::Read => {
                if !(descr.writelock) {
                    descr.active_locks += 1;
                    Ok(create_handle(descr, mode));
                }
                else {
                    Err(io::Error::new(io::ErrorKind::Other, "this file is already open for writing"));
                }
            }
            
            OpenMode::ReadWrite => {
                if descr.writelock {
                    Err(io::Error::new(io::ErrorKind::Other, "this file is already open for writing"));
                }
                if descr.active_locks > 0 {
                    Err(io::Error::new(io::ErrorKind::Other, "this file is already open for reading"));
                }
                descr.active_locks += 1;
                descr.writelock = true;
                Ok(create_handle(descr, mode));
            }
            
            OpenMode::Write => {
                if descr.writelock {
                    Err(io::Error::new(io::ErrorKind::Other, "this file is already open for writing"));
                }
                if descr.active_locks > 0 {
                    Err(io::Error::new(io::ErrorKind::Other, "this file is already open for reading"));
                }
                descr.writelock = true;
                Ok(create_handle(descr, mode));
            }
            
            OpenMode::append => {
                if descr.writelock {
                    Err(io::Error::new(io::ErrorKind::Other, "this file is already open for writing"));
                }
                if descr.active_locks > 0 {
                    Err(io::Error::new(io::ErrorKind::Other, "this file is already open for reading"));
                }
                descr.writelock = true;
                Ok(create_handle(descr, mode));
            }
            
            OpenMode::error => {
                Err(io::Error::new(io::ErrorKind::Other, "error open mode"));
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
        while i < MAX_OPEN_FILES {
            if !(self.handle_usage[i]) {
                if mode == OpenMode::append {
                    self.file_handles[i].current_position = fdesc.end_position;
                }
                else {
                    self.file_handles[i].current_position = i * SECTOR_SIZE;
                }
                self.handle_usage[i] = true;
                self.file_handles[i].open_mode = mode;
                self.file_handles[i].fdesc = fdesc;
                return i;
            }
            return MAX_OPEN_FILES;
        }
    }
    
    fn delete_handle(&mut self, hndl: FileHandle) {
        if self.handle_usage[i] {
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
        if len(buf) > SECTOR_SIZE - (self.current_position % SECTOR_SIZE) {
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
        while i < MAX_SECTORS && i < (self.capacity / SECTOR_SIZE) {
            if !(sectors[i]) {
                sectors[i] = true;
                Ok(i as u32);
            }
            i += 1;
        }
        Err(io::Error::new(io::ErrorKind::OutOfSpace, "File system ran out of space"));
    }
    
    
    
    fn free_children(&mut self, sector: u32) {
        self.storage.seek(SeekFrom::Start(SECTOR_SIZE * sector));
        let pointer = read_shorthead();
        if pointer == MAX_SECTORS {
            return;
        }
        free_auto(pointer);
        return;
    }
    
    fn free_auto(&mut self, sector: u32) {
        self.storage.seek(SeekFrom::Start(SECTOR_SIZE * sector));
        let pointer = read_shorthead();
        if pointer == MAX_SECTORS {
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
	

    
    fn navigate(&mut self, path: Path) -> u32 {
        self.storage.seek(SeekFrom::Start(0));
        let fd: FileDescriptor = read_header();
        // TODO - implement navigation. Returns sector number, or MAX_SECTORS if not found.
    }

    pub fn inner_mut(&mut self) -> &mut T {
        self.storage
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
