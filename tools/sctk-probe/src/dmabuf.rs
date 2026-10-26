//! A linear DMA-BUF allocated with `/dev/udmabuf`, for the capture probes.
//!
//! The capture probe needs a DMA-BUF it can inspect from the CPU, without
//! pulling in GBM or EGL. `udmabuf` turns a memfd into a linear DMA-BUF, which
//! is enough for `ext-image-copy-capture-v1`'s constraints when the compositor
//! offers a linear modifier.

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

/// A mapped, linear DMA-BUF.
pub struct DmaBuffer {
    fd: OwnedFd,
    map: *mut u8,
    len: usize,
    stride: usize,
}

unsafe impl Send for DmaBuffer {}

impl DmaBuffer {
    /// Creates a DMA-BUF of `stride * height` bytes, rounded to a page.
    pub fn new(_width: i32, height: i32, stride: usize) -> io::Result<Self> {
        let len = stride * height as usize;
        let page = 4096;
        let aligned = (len + page - 1) & !(page - 1);
        let name = b"malaysia-capture\0";
        let memfd = unsafe {
            libc::memfd_create(
                name.as_ptr().cast(),
                libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING,
            )
        };
        if memfd < 0 {
            return Err(io::Error::last_os_error());
        }
        let memfd = unsafe { OwnedFd::from_raw_fd(memfd) };
        if unsafe { libc::ftruncate(memfd.as_raw_fd(), aligned as libc::off_t) } < 0 {
            return Err(io::Error::last_os_error());
        }
        // udmabuf only accepts a memfd whose size cannot change underneath it.
        if unsafe { libc::fcntl(memfd.as_raw_fd(), libc::F_ADD_SEALS, libc::F_SEAL_SHRINK) } < 0 {
            return Err(io::Error::last_os_error());
        }
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
        let dmabuf = unsafe {
            libc::ioctl(
                udmabuf.as_raw_fd(),
                UDMABUF_CREATE,
                &create as *const UdmabufCreate,
            )
        };
        if dmabuf < 0 {
            let error = io::Error::last_os_error();
            unsafe { libc::munmap(map, aligned) };
            return Err(error);
        }
        Ok(Self {
            fd: unsafe { OwnedFd::from_raw_fd(dmabuf) },
            map: map.cast(),
            len: aligned,
            stride,
        })
    }

    /// The DMA-BUF file descriptor.
    pub fn fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }

    /// Counts bytes that differ from `background`.
    pub fn count_different(&self, background: [u8; 4]) -> usize {
        let bytes = unsafe { std::slice::from_raw_parts(self.map, self.len) };
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| **pixel != background)
            .count()
    }

    /// Reads a pixel as B, G, R, X from the CPU mapping.
    pub fn pixel(&self, x: usize, y: usize) -> [u8; 4] {
        let offset = y * self.stride + x * 4;
        if offset + 4 > self.len {
            return [0; 4];
        }
        let mut pixel = [0; 4];
        unsafe {
            ptr::copy_nonoverlapping(self.map.add(offset), pixel.as_mut_ptr(), 4);
        }
        pixel
    }
}

impl Drop for DmaBuffer {
    fn drop(&mut self) {
        unsafe { libc::munmap(self.map.cast(), self.len) };
    }
}
