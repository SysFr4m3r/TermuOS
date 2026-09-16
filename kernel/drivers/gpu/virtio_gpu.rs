#![allow(dead_code)]

const VIRTIO_VENDOR: u16 = 0x1AF4;
const VIRTIO_GPU_DEVICE: u16 = 0x1050;
const PCI_CAP_ID_VNDR: u8 = 0x09;

const VIRTIO_PCI_CAP_COMMON_CFG: u8 = 1;
const VIRTIO_PCI_CAP_NOTIFY_CFG: u8 = 2;
const VIRTIO_PCI_CAP_ISR_CFG: u8 = 3;
const VIRTIO_PCI_CAP_DEVICE_CFG: u8 = 4;

const VIRTIO_STATUS_ACKOWNLEDGE: u8 = 1;
const VIRTIO_STATUS_DRIVER: u8 = 2;
const VIRTIO_STATUS_FEATURES_OK: u8 = 8;
const VIRTIO_STATUS_DRIVER_OK: u8 = 4;

#[repr(C)]
struct PciDevice {
    vendor: u16,
    device: u16,
    bus: u8,
    slot: u8,
    func: u8,
    bar: [u32; 6],
    irq: u8,
}

struct CapLoc {
    bar: u8,
    offset: u32,
    length: u32,
}

extern "C" {
    fn pci_find(vendor: u16, device: u16, out: *mut PciDevice) -> i32;
    fn pci_read(bus: u8, slot: u8, func: u8, offset: u8) -> u32;
    fn pci_write(bus: u8, slot: u8, func: u8, offset: u8, val: u32);
    fn kprintf(fmt: *const u8, ...);
    fn termuos_hhdm_base() -> u64;
    fn pmm_alloc_pages(n: usize) -> *mut u8;
    fn kvirt_to_phys(v: *mut u8) -> u64;
}

unsafe fn pci_read8(bus: u8, slot: u8, func: u8, off: u8) -> u8 {
    let v = pci_read(bus, slot, func, off & !3);
    ((v >> ((off & 3) * 8)) & 0xff) as u8
}

unsafe fn pci_enable_mem_bm(bus: u8, slot: u8, func: u8) {
    let mut cmd = pci_read(bus, slot, func, 0x04);
    cmd |= (1 << 1) | (1 << 2);
    pci_write(bus, slot, func, 0x04, cmd);
}

unsafe fn read_vndr_cap(bus: u8, slot: u8, func: u8, cap_off: u8) -> (u8, CapLoc) {
    let cfg_type = pci_read8(bus, slot, func, cap_off.wrapping_add(3));
    let bar = pci_read8(bus, slot, func, cap_off.wrapping_add(4));
    let offset = pci_read(bus, slot, func, cap_off.wrapping_add(8));
    let length = pci_read(bus, slot, func, cap_off.wrapping_add(12));
    (
        cfg_type,
        CapLoc {
            bar,
            offset,
            length,
        },
    )
}

fn bar_phys(bar: u32) -> u64 {
    (bar & !0xF) as u64
}

unsafe fn map_bar(bar_raw: u32) -> *mut u8 {
    let phys = bar_phys(bar_raw);
    if phys == 0 {
        return core::ptr::null_mut();
    }
    (termuos_hhdm_base() + phys) as *mut u8
}

unsafe fn mmio_w8(p: *mut u8, off: usize, v: u8) {
    core::ptr::write_volatile(p.add(off), v);
}
unsafe fn mmio_r8(p: *mut u8, off: usize) -> u8 {
    core::ptr::read_volatile(p.add(off))
}
unsafe fn mmio_w16(p: *mut u8, off: usize, v: u16) {
    core::ptr::write_volatile(p.add(off) as *mut u16, v);
}
unsafe fn mmio_r16(p: *mut u8, off: usize) -> u16 {
    core::ptr::read_volatile(p.add(off) as *mut u16)
}
unsafe fn mmio_w64(p: *mut u8, off: usize, v: u64) {
    core::ptr::write_volatile(p.add(off) as *mut u64, v);
}

unsafe fn setup_controlq(common: *mut u8) -> i32 {
    let st = mmio_r8(common, 0x14);
    mmio_w8(common, 0x14, st | VIRTIO_STATUS_FEATURES_OK);
    if mmio_r8(common, 0x14) & VIRTIO_STATUS_FEATURES_OK == 0 {
        kprintf(b"virtio-gpu: FEATURES_OK rejected\n\0".as_ptr());
        return -1;
    }

    mmio_w16(common, 0x16, 0);
    let mut qsz = mmio_r16(common, 0x18) as usize;
    if qsz == 0 {
        kprintf(b"virtio-gpu: queue0 size 0\n\0".as_ptr());
        return -1;
    }
    if qsz > 64 {
        qsz = 64;
        mmio_w16(common, 0x18, qsz as u16);
    }

    let desc_bytes = 16 * qsz;
    let avail_bytes = 4 + 2 * qsz;
    let used_bytes = 4 + 8 * qsz;
    let used_off = (desc_bytes + avail_bytes + 0xfff) & !0xfff;
    let total = used_off + used_bytes;
    let pages = (total + 0xfff) / 0x1000;

    let mem = pmm_alloc_pages(pages);
    if mem.is_null() {
        kprintf(b"virtio-gpu: oom queue\n\0".as_ptr());
        return -1;
    }
    core::ptr::write_bytes(mem, 0, pages * 0x1000);

    let desc_p = kvirt_to_phys(mem);
    let avail_p = kvirt_to_phys(mem.add(desc_bytes));
    let used_p = kvirt_to_phys(mem.add(used_off));

    mmio_w64(common, 0x20, desc_p);
    mmio_w64(common, 0x28, avail_p);
    mmio_w64(common, 0x30, used_p);
    mmio_w16(common, 0x1c, 1);

    let st = mmio_r8(common, 0x14);
    mmio_w8(common, 0x14, st | VIRTIO_STATUS_DRIVER_OK);

    kprintf(b"virtio-gpu: phase3 ok (controlq enabled)\n\0".as_ptr());
    0
}

#[no_mangle]
pub extern "C" fn virtio_gpu_rust_probe() {
    let mut dev = PciDevice {
        vendor: 0,
        device: 0,
        bus: 0,
        slot: 0,
        func: 0,
        bar: [0; 6],
        irq: 0,
    };

    if unsafe { pci_find(VIRTIO_VENDOR, VIRTIO_GPU_DEVICE, &mut dev) } != 0 {
        unsafe { kprintf(b"virtio-gpu: not found\n\0".as_ptr()) };
        return;
    }
    unsafe {
        kprintf(b"virtio-gpu: pci found\n\0".as_ptr());
        pci_enable_mem_bm(dev.bus, dev.slot, dev.func);
    }

    let mut common_loc: Option<CapLoc> = None;
    let mut cap = unsafe { pci_read8(dev.bus, dev.slot, dev.func, 0x34) };

    while cap != 0 && cap != 0xff {
        let id = unsafe { pci_read8(dev.bus, dev.slot, dev.func, cap) };
        if id == PCI_CAP_ID_VNDR {
            let (typ, loc) = unsafe { read_vndr_cap(dev.bus, dev.slot, dev.func, cap) };
            unsafe {
                match typ {
                    VIRTIO_PCI_CAP_COMMON_CFG => {
                        kprintf(b"virtio-gpu: cap common\n\0".as_ptr());
                        common_loc = Some(loc);
                    }
                    VIRTIO_PCI_CAP_NOTIFY_CFG => kprintf(b"virtio-gpu: cap notify\n\0".as_ptr()),
                    VIRTIO_PCI_CAP_ISR_CFG => kprintf(b"virtio-gpu: cap isr\n\0".as_ptr()),
                    VIRTIO_PCI_CAP_DEVICE_CFG => kprintf(b"virtio-gpu: cap device\n\0".as_ptr()),
                    _ => kprintf(b"virtio-gpu: cap other\n\0".as_ptr()),
                }
            }
        }
        cap = unsafe { pci_read8(dev.bus, dev.slot, dev.func, cap.wrapping_add(1)) };
    }

    let Some(common_loc) = common_loc else {
        unsafe { kprintf(b"virtio-gpu: no common cfg\n\0".as_ptr()) };
        return;
    };

    if (common_loc.bar as usize) >= 6 {
        unsafe { kprintf(b"virtio-gpu: bad bar idx\n\0".as_ptr()) };
        return;
    }

    let bar_raw = dev.bar[common_loc.bar as usize];
    let base = unsafe { map_bar(bar_raw) };
    if base.is_null() {
        unsafe { kprintf(b"virtio-gpu: bar map failed\n\0".as_ptr()) };
        return;
    }

    let common = unsafe { base.add(common_loc.offset as usize) };

    unsafe {
        mmio_w8(common, 0x14, 0);
        mmio_w8(common, 0x14, VIRTIO_STATUS_ACKOWNLEDGE);
        mmio_w8(
            common,
            0x14,
            VIRTIO_STATUS_ACKOWNLEDGE | VIRTIO_STATUS_DRIVER,
        );
        kprintf(b"virtio-gpu: phase2 ok (ACK|DRIVER)\n\0".as_ptr());
    }

    unsafe {
        let _ = setup_controlq(common);
    }
}

