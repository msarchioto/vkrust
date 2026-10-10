//! Limine boot protocol requests. The linker script keeps these in `.data`.

use core::cell::UnsafeCell;

const COMMON_MAGIC: [u64; 2] = [0xc7b1dd30df4c8b88, 0x0a82e883a194f07b];

#[repr(C)]
struct BaseRevision {
    magic: UnsafeCell<[u64; 3]>,
}

unsafe impl Sync for BaseRevision {}

impl BaseRevision {
    const fn new(revision: u64) -> Self {
        Self {
            magic: UnsafeCell::new([0xf9562b2d5c95a6c8, 0x6a7b384944536bdc, revision]),
        }
    }

    fn is_supported(&self) -> bool {
        unsafe { (*self.magic.get())[2] == 0 }
    }
}

#[repr(C)]
struct Request<T> {
    magic: [u64; 2],
    id: [u64; 2],
    revision: u64,
    response: UnsafeCell<*mut T>,
}

unsafe impl<T> Sync for Request<T> {}

impl<T> Request<T> {
    const fn new(id: [u64; 2]) -> Self {
        Self {
            magic: COMMON_MAGIC,
            id,
            revision: 0,
            response: UnsafeCell::new(core::ptr::null_mut()),
        }
    }

    fn response(&self) -> Option<&'static T> {
        let ptr = unsafe { self.response.get().read_volatile() };
        if ptr.is_null() {
            None
        } else {
            Some(unsafe { &*ptr })
        }
    }
}

#[repr(C)]
struct StackSizeRequest {
    magic: [u64; 2],
    id: [u64; 2],
    revision: u64,
    response: UnsafeCell<*mut u64>,
    size: u64,
}

unsafe impl Sync for StackSizeRequest {}

#[repr(C)]
struct PagingModeRequest {
    magic: [u64; 2],
    id: [u64; 2],
    revision: u64,
    response: UnsafeCell<*mut u64>,
    mode: u64,
    max_mode: u64,
    min_mode: u64,
}

unsafe impl Sync for PagingModeRequest {}

#[repr(C)]
struct RequestsStartMarker([u64; 4]);

#[repr(C)]
struct RequestsEndMarker([u64; 2]);

#[used]
#[unsafe(link_section = ".requests_start_marker")]
static _START_MARKER: RequestsStartMarker = RequestsStartMarker([
    0xf6b8f4b39de7d1ae,
    0xfab91a6940fcb9cf,
    0x785c6ed015d3e316,
    0x181e920a7852b9d9,
]);

#[used]
#[unsafe(link_section = ".requests_end_marker")]
static _END_MARKER: RequestsEndMarker =
    RequestsEndMarker([0xadc0e0531bb10d03, 0x9572709f31764c62]);

#[used]
#[unsafe(link_section = ".requests")]
// Limine 10.8.5 (the v10.x-binary tag this tree builds against) implements
// base revisions through 4. Revision 6 arrived in Limine 11.
static BASE_REVISION: BaseRevision = BaseRevision::new(4);

#[used]
#[unsafe(link_section = ".requests")]
static STACK_SIZE: StackSizeRequest = StackSizeRequest {
    magic: COMMON_MAGIC,
    id: [0x224ef0460a8e8926, 0xe1cb0fc25f46ea3d],
    revision: 0,
    response: UnsafeCell::new(core::ptr::null_mut()),
    size: 128 * 1024,
};

/// Ask for classic 4-level paging so `OffsetPageTable` can walk it.
#[used]
#[unsafe(link_section = ".requests")]
static PAGING_MODE: PagingModeRequest = PagingModeRequest {
    magic: COMMON_MAGIC,
    id: [0x95c1a0edab0944cb, 0xa4e5cb3842f7488a],
    revision: 0,
    response: UnsafeCell::new(core::ptr::null_mut()),
    mode: 0,
    max_mode: 0,
    min_mode: 0,
};

#[repr(C)]
pub struct HhdmResponse {
    pub revision: u64,
    pub offset: u64,
}

#[used]
#[unsafe(link_section = ".requests")]
static HHDM: Request<HhdmResponse> = Request::new([0x48dcf1cb8ad2b852, 0x63984e959a98244b]);

#[repr(C)]
pub struct MemmapEntry {
    pub base: u64,
    pub length: u64,
    pub entry_type: u64,
}

pub const MEMMAP_USABLE: u64 = 0;

#[repr(C)]
struct MemmapResponse {
    revision: u64,
    entry_count: u64,
    entries: *const *const MemmapEntry,
}

#[used]
#[unsafe(link_section = ".requests")]
static MEMMAP: Request<MemmapResponse> = Request::new([0x67cf3d9d378a806f, 0xe304acdfc50c3c62]);

#[repr(C)]
pub struct Framebuffer {
    pub address: *mut u8,
    pub width: u64,
    pub height: u64,
    pub pitch: u64,
    pub bpp: u16,
    pub memory_model: u8,
    pub red_mask_size: u8,
    pub red_mask_shift: u8,
    pub green_mask_size: u8,
    pub green_mask_shift: u8,
    pub blue_mask_size: u8,
    pub blue_mask_shift: u8,
    pub unused: [u8; 7],
    pub edid_size: u64,
    pub edid: *mut u8,
}

#[repr(C)]
struct FramebufferResponse {
    revision: u64,
    framebuffer_count: u64,
    framebuffers: *const *const Framebuffer,
}

#[used]
#[unsafe(link_section = ".requests")]
static FRAMEBUFFER: Request<FramebufferResponse> =
    Request::new([0x9d5827dcd881dd75, 0xa3148604f6fab11b]);

pub struct Boot {
    pub hhdm_offset: u64,
    pub framebuffer: Option<&'static Framebuffer>,
    pub memory_map: &'static [&'static MemmapEntry],
}

pub fn take() -> Boot {
    // Touch every request so the linker cannot drop it.
    let _ = &_START_MARKER as *const _;
    let _ = &_END_MARKER as *const _;
    let _ = &STACK_SIZE as *const _;
    let _ = &PAGING_MODE as *const _;

    if !BASE_REVISION.is_supported() {
        panic!("Limine base revision is not supported");
    }

    let hhdm = HHDM.response().expect("Limine did not provide HHDM");
    let mem = MEMMAP.response().expect("Limine did not provide a memory map");
    let memory_map = unsafe {
        core::slice::from_raw_parts(
            mem.entries as *const &'static MemmapEntry,
            mem.entry_count as usize,
        )
    };
    let framebuffer = FRAMEBUFFER.response().and_then(|resp| {
        if resp.framebuffer_count == 0 {
            None
        } else {
            Some(unsafe { &*resp.framebuffers.read() })
        }
    });

    Boot {
        hhdm_offset: hhdm.offset,
        framebuffer,
        memory_map,
    }
}
