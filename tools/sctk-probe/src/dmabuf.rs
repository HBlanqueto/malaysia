use std::{
    fs::OpenOptions,
    io,
    mem::size_of,
    os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd},
    ptr,
};

const UDMABUF_CREATE: libc::c_ulong = (1 << 30)
    | ((size_of::<UdmabufCreate>() as libc::c_ulong) << 16)
    | ((b'u' as libc::c_ulong) << 8)
    | 0x42;

#[repr(C)]
struct UdmabufCreate {
    memfd: u32,
    flags: u32,
    offset: u64,
    size: u64,
}

pub struct DmaBuffer {
    fd: OwnedFd,
    map: *mut u8,
    len: usize,
    stride: usize,
}

// SAFETY: the mapping is owned by this buffer and only read through `&self`.
unsafe impl Send for DmaBuffer {}

impl DmaBuffer {
    pub fn new(_width: i32, height: i32, stride: usize) -> io::Result<Self> {
        let len = stride * height as usize;
        let page = 4096;
        let aligned = (len + page - 1) & !(page - 1);
        let name = b"malaysia-capture\0";
        // SAFETY: `name` is NUL-terminated and the flags are valid for memfd_create(2).
        let memfd = unsafe {
            libc::memfd_create(
                name.as_ptr().cast(),
                libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING,
            )
        };
        if memfd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: memfd_create returned a fresh fd that nothing else owns.
        let memfd = unsafe { OwnedFd::from_raw_fd(memfd) };
        // SAFETY: `memfd` is an open memfd.
        if unsafe { libc::ftruncate(memfd.as_raw_fd(), aligned as libc::off_t) } < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `memfd` was created with MFD_ALLOW_SEALING.
        if unsafe { libc::fcntl(memfd.as_raw_fd(), libc::F_ADD_SEALS, libc::F_SEAL_SHRINK) } < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `memfd` is `aligned` bytes long and a shared mapping of it is requested.
        let map = unsafe {
            libc::mmap(
                ptr::null_mut(),
                aligned,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                memfd.as_raw_fd(),
                0,
            )
        };
        if map == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        let udmabuf = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/udmabuf")?;
        let create = UdmabufCreate {
            memfd: memfd.as_raw_fd() as u32,
            flags: 0,
            offset: 0,
            size: aligned as u64,
        };
        // SAFETY: `create` is the UDMABUF_CREATE argument and outlives the call.
        let dmabuf = unsafe {
            libc::ioctl(
                udmabuf.as_raw_fd(),
                UDMABUF_CREATE,
                &create as *const UdmabufCreate,
            )
        };
        if dmabuf < 0 {
            let error = io::Error::last_os_error();
            // SAFETY: `map` is the `aligned`-byte mapping created above.
            unsafe { libc::munmap(map, aligned) };
            return Err(error);
        }
        Ok(Self {
            // SAFETY: UDMABUF_CREATE returned a fresh fd that nothing else owns.
            fd: unsafe { OwnedFd::from_raw_fd(dmabuf) },
            map: map.cast(),
            len: aligned,
            stride,
        })
    }

    pub fn fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }

    pub fn count_different(&self, background: [u8; 4]) -> usize {
        // SAFETY: `map` stays mapped for `len` bytes while `self` lives.
        let bytes = unsafe { std::slice::from_raw_parts(self.map, self.len) };
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| **pixel != background)
            .count()
    }

    pub fn pixel(&self, x: usize, y: usize) -> [u8; 4] {
        let offset = y * self.stride + x * 4;
        if offset + 4 > self.len {
            return [0; 4];
        }
        let mut pixel = [0; 4];
        // SAFETY: `offset + 4 <= len` was checked above.
        unsafe {
            ptr::copy_nonoverlapping(self.map.add(offset), pixel.as_mut_ptr(), 4);
        }
        pixel
    }
}

impl Drop for DmaBuffer {
    fn drop(&mut self) {
        // SAFETY: `map` is the `len`-byte mapping this buffer owns.
        unsafe { libc::munmap(self.map.cast(), self.len) };
    }
}
