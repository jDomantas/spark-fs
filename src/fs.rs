use core::{u8, u32};
use io::{self, ReadWriteSeek, SeekFrom};
use path::{self, Path};

const SECTOR_SIZE: u32 = 4096;

const FD_MAGIC_NUMBER: u8 = 0xC4;
const MAX_SECTORS: u32 = 1048576;
const MAX_OPEN_FILES: u8 = 128;

#[repr(u8)]
#[derive(Debug, Copy, Clone)]
enum FileType {
    Folder,
    Exec,
    Default,
    Error,
}

#[repr(u8)]
#[derive(Debug, Copy, Clone)]
enum FileFlag {
    Special,
    Readonly,
    ReadWrite,
    Error,
}

#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum OpenMode {
    Read,
    ReadWrite,
    Write,
    Append,
    Error,
}

pub struct FileSystem<'a, T: 'a> {
    storage: &'a mut T,
    sectors: [bool; MAX_SECTORS as usize],
    capacity: u32,
    current_sector: u32,
    file_handles: [FileHandle; MAX_OPEN_FILES as usize],
    handle_usage: [bool; MAX_OPEN_FILES as usize],
}

#[derive(Copy, Clone)]
pub struct FileDescriptor {
    filetype: FileType,
    fileflag: FileFlag,
    end_position: u32,
    filename: Path,
    active_locks: u8,
    writelock: bool,
}

#[derive(Copy, Clone)]
pub struct FileHandle {
    current_position: u32,
    open_mode: OpenMode,
    fdesc: FileDescriptor,
    handle_no: u8,
}

fn transform_u32_to_array_of_u8(x: u32) -> [u8; 4] {
    let b1: u8 = ((x >> 24) & 0xff) as u8;
    let b2: u8 = ((x >> 16) & 0xff) as u8;
    let b3: u8 = ((x >> 8) & 0xff) as u8;
    let b4: u8 = (x & 0xff) as u8;
    return [b1, b2, b3, b4];
}

fn transform_array_of_u8_to_u32(x: [u8; 4]) -> u32 {
    let mut y: u32 = (x[0] as u32) << 24;
    y = y & ((x[1] as u32) << 16);
    y = y & ((x[2] as u32) << 8);
    y = y & (x[3] as u32);
    return y;
}

impl<'a, T: ReadWriteSeek + 'a> FileSystem<'a, T> {
    fn write_shorthead(&mut self, x: u32) -> io::Result<()> {
        let buf: [u8; 4] = transform_u32_to_array_of_u8(x);
        self.storage.write_all(&buf)
    }

    fn read_shorthead(&mut self) -> io::Result<u32> {
        let mut buf: [u8; 4] = [0; 4];
        self.storage.read_exact(&mut buf)?;
        Ok(transform_array_of_u8_to_u32(buf))
    }

    fn write_header(&mut self, mut hndl: FileHandle) -> io::Result<()> {
        let fd = hndl.fdesc;
        self.write_shorthead(u32::MAX)?;
        hndl.current_position += 4;
        self.storage.write_all(&[FD_MAGIC_NUMBER])?;
        self.storage.write_all(&[fd.filetype as u8])?;
        self.storage.write_all(&[fd.fileflag as u8])?;
        self.storage.write_all(&[0])?;
        self.storage
            .write_all(&(transform_u32_to_array_of_u8(fd.end_position)))?;
        if fd.writelock {
            self.storage.write_all(&[0])?;
        } else {
            self.storage.write_all(&[1])?;
        }
        let buf = fd.filename.raw_buf();
        let buflen = buf.len() as u32;
        self.storage.write_all(&buf)?;

        hndl.current_position += 9 + buflen;
        hndl.fdesc.end_position = hndl.current_position;
        Ok(())
    }

    pub fn new(&mut self, storage: &'a mut T, size: u32) -> io::Result<Self> {
        let fs = FileSystem {
            storage: storage,
            sectors: [false; MAX_SECTORS as usize],
            handle_usage: [false; MAX_OPEN_FILES as usize],
            current_sector: 0,
            capacity: size,
            file_handles: [FileHandle {
                current_position: 0,
                open_mode: OpenMode::Error,
                fdesc: FileDescriptor {
                    filetype: FileType::Error,
                    fileflag: FileFlag::Error,
                    end_position: 0,
                    filename: path::EMPTY,
                    active_locks: 0,
                    writelock: false,
                },
                handle_no: MAX_OPEN_FILES,
            }; MAX_OPEN_FILES as usize],
        };

        self.sectors[0] = true;
        self.handle_usage[0] = true;

        self.storage.seek(SeekFrom::Start(0))?;

        let hndl: FileHandle = FileHandle {
            current_position: 0,
            open_mode: OpenMode::Read,
            handle_no: 0,
            fdesc: FileDescriptor {
                filetype: FileType::Folder,
                fileflag: FileFlag::Special,
                end_position: 0,
                filename: Path::from_ascii_str(b"root").expect("failed to construct root path"),
                active_locks: 1,
                writelock: false,
            },
        };

        self.write_header(hndl)?;
        Ok(fs)
    }

    fn read_header(&mut self) -> io::Result<FileDescriptor> {
        self.read_shorthead()?;
        let mut magicno: [u8; 1] = [0];
        self.storage.read_exact(&mut magicno)?;
        if magicno[0] != FD_MAGIC_NUMBER {
            return Err(io::Error::new(io::ErrorKind::Other, "File header corrupt"));
        }

        let ftype: FileType;

        self.storage.read_exact(&mut magicno)?;
        if FileType::Folder as u8 == magicno[0] {
            ftype = FileType::Folder;
        } else if FileType::Exec as u8 == magicno[0] {
            ftype = FileType::Exec;
        } else if FileType::Default as u8 == magicno[0] {
            ftype = FileType::Default;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::FileNotFound,
                "File header corrupt",
            ));
        }

        let fflag: FileFlag;

        self.storage.read_exact(&mut magicno)?;
        if FileFlag::ReadWrite as u8 == magicno[0] {
            fflag = FileFlag::ReadWrite;
        } else if FileFlag::Readonly as u8 == magicno[0] {
            fflag = FileFlag::Readonly;
        } else if FileFlag::Special as u8 == magicno[0] {
            fflag = FileFlag::Special;
        } else {
            return Err(io::Error::new(io::ErrorKind::Other, "File header corrupt"));
        }

        self.storage.read(&mut magicno)?;

        let mut end_pos_buf: [u8; 4] = [0; 4];
        self.storage.read_exact(&mut end_pos_buf)?;

        let mut writelock: [u8; 1] = [0; 1];
        self.storage.read_exact(&mut writelock)?;

        let mut fname = [0; path::MAX_PATH_LENGTH];

        self.storage.read_exact(&mut fname)?;

        let wlock: bool = writelock[0] == 1;
        let end_pos: u32 = transform_array_of_u8_to_u32(end_pos_buf);
        let fdesc = FileDescriptor {
            filetype: ftype,
            fileflag: fflag,
            filename: Path::from_ascii_str(&fname).expect("failed to construct filename"),
            active_locks: (magicno[0]),
            writelock: wlock,
            end_position: end_pos,
        };
        return Ok(fdesc);
    }

    fn navigate(&mut self, _path: Path) -> io::Result<u32> {
        self.storage.seek(SeekFrom::Start(0))?;
        // TODO - implement navigation. Returns sector number, or MAX_SECTORS if not found.
        Ok(0)
    }

    pub fn open_file(&mut self, path: Path, mode: OpenMode) -> io::Result<FileHandle> {
        let sect = self.navigate(path)?;
        if sect == MAX_SECTORS {
            return Err(io::Error::new(
                io::ErrorKind::FileNotFound,
                "file not found",
            ));
        }
        self.storage
            .seek(SeekFrom::Start((sect * SECTOR_SIZE as u32) as u64))?;
        let mut descr = self.read_header().expect("failed to read header");
        match mode {
            OpenMode::Read => {
                if !(descr.writelock) {
                    descr.active_locks += 1;
                    Ok(self.file_handles[self.create_handle(descr, mode) as usize])
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::Other,
                        "this file is already open for writing",
                    ))
                }
            }

            OpenMode::ReadWrite => {
                if descr.writelock {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        "this file is already open for writing",
                    ));
                }
                if descr.active_locks > 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        "this file is already open for reading",
                    ));
                }
                descr.active_locks += 1;
                descr.writelock = true;
                Ok(self.file_handles[self.create_handle(descr, mode) as usize])
            }

            OpenMode::Write => {
                if descr.writelock {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        "this file is already open for writing",
                    ));
                }
                if descr.active_locks > 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        "this file is already open for reading",
                    ));
                }
                descr.writelock = true;
                Ok(self.file_handles[self.create_handle(descr, mode) as usize])
            }

            OpenMode::Append => {
                if descr.writelock {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        "this file is already open for writing",
                    ));
                }
                if descr.active_locks > 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        "this file is already open for reading",
                    ));
                }
                descr.writelock = true;
                return Ok(self.file_handles[self.create_handle(descr, mode) as usize]);
            }

            OpenMode::Error => {
                return Err(io::Error::new(io::ErrorKind::Other, "error open mode"));
            }
        }
    }

    fn create_handle(&mut self, fdesc: FileDescriptor, mode: OpenMode) -> u8 {
        let i: usize = 0;
        while i < MAX_OPEN_FILES as usize {
            if !(self.handle_usage[i]) {
                if mode == OpenMode::Append {
                    self.file_handles[i].current_position = fdesc.end_position;
                } else {
                    self.file_handles[i].current_position =
                        (i * SECTOR_SIZE as usize + 13 + path::MAX_PATH_LENGTH) as u32;
                }
                self.handle_usage[i] = true;
                self.file_handles[i].open_mode = mode;
                self.file_handles[i].fdesc = fdesc;
                return i as u8;
            }
        }
        return MAX_OPEN_FILES;
    }

    fn delete_handle(&mut self, hndl: FileHandle) {
        if self.handle_usage[hndl.handle_no as usize] {
            self.file_handles[hndl.handle_no as usize].fdesc.filetype = FileType::Error;
            self.file_handles[hndl.handle_no as usize].fdesc.fileflag = FileFlag::Error;
            self.file_handles[hndl.handle_no as usize].fdesc.filename =
                Path::from_ascii_str(b"").expect("failed to construct null path");
            self.handle_usage[hndl.handle_no as usize] = false;
        }
    }

    fn write_auto(&mut self, buf: &[u8], mut hndl: FileHandle) -> io::Result<()> {
        if buf.len() > (SECTOR_SIZE - (hndl.current_position % SECTOR_SIZE)) as usize {
            let curr_sec = hndl.current_position / SECTOR_SIZE;
            let buf1 = &buf[((SECTOR_SIZE - (hndl.current_position % SECTOR_SIZE)) as usize)..];
            self.storage.write_all(
                &(buf[..((SECTOR_SIZE - (hndl.current_position % SECTOR_SIZE)) as usize)]),
            )?;
            self.storage
                .seek(SeekFrom::Start((curr_sec * SECTOR_SIZE) as u64))?;
            hndl.current_position = curr_sec * SECTOR_SIZE;
            let new_sector = self.get_valid_sector().expect("couldn't get valid sector");
            self.write_shorthead(new_sector)?;
            hndl.current_position = new_sector * SECTOR_SIZE;
            self.storage
                .seek(SeekFrom::Start(hndl.current_position as u64))?;
            self.write_auto(&buf1, hndl)?;
        } else {
            self.storage.write_all(buf)?;
            hndl.current_position += buf.len() as u32;
        }
        Ok(())
    }

    fn get_valid_sector(&mut self) -> io::Result<u32> {
        let mut i: u32 = 0;
        while i < MAX_SECTORS && i < (self.capacity / SECTOR_SIZE) {
            if !(self.sectors[i as usize]) {
                self.sectors[i as usize] = true;
                return Ok(i as u32);
            }
            i += 1;
        }
        return Err(io::Error::new(
            io::ErrorKind::OutOfSpace,
            "File system ran out of space",
        ));
    }

    fn free_children(&mut self, sector: u32) -> io::Result<()> {
        self.storage
            .seek(SeekFrom::Start((SECTOR_SIZE * sector) as u64))?;
        let pointer = self.read_shorthead()?;
        if pointer != MAX_SECTORS {
            self.free_auto(pointer)?;
        }
        Ok(())
    }

    fn free_auto(&mut self, sector: u32) -> io::Result<()> {
        self.storage
            .seek(SeekFrom::Start((SECTOR_SIZE * sector) as u64))?;
        let pointer = self.read_shorthead()?;
        if pointer == MAX_SECTORS {
            self.sectors[sector as usize] = false;
        } else {
            self.sectors[sector as usize] = false;
            self.free_auto(pointer)?;
        }
        Ok(())
    }

    pub fn inner_mut(&mut self) -> &mut T {
        self.storage
    }
}
