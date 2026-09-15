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

extern "C" {
    fn pci_find(vendor: u16, device: u16, out: *mut PciDevice) -> i32;
    fn pci_read(bus: u8, slot: u8, func: u8, offset: u8) -> u32;
    fn pci_write(bus: u8, slot: u8, func: u8, offset: u8, val: u32);
    fn kprintf(fmt: *const u8, ...);
    fn termuos_hhdm_base() -> u64;
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

struct CapLoc {
    bar: u8,
    offset: u32,
    length: u32,
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

    let found = unsafe { pci_find(VIRTIO_VENDOR, VIRTIO_GPU_DEVICE, &mut dev) };
    if found != 0 {
        unsafe {
            kprintf(b"virtio-gpu: not found\n\0".as_ptr());
        }
        return;
    }

    unsafe {
        kprintf(b"virtio-gpu: pci found\n\0".as_ptr());
        pci_enable_mem_bm(dev.bus, dev.slot, dev.func);
    }

    let mut common: Option<CapLoc> = None;
    let mut cap = unsafe { pci_read8(dev.bus, dev.slot, dev.func, 0x34) };

    while cap != 0 && cap != 0xff {
        let id = unsafe { pci_read8(dev.bus, dev.slot, dev.func, cap) };
        if id == PCI_CAP_ID_VNDR {
            let (typ, loc) = unsafe { read_vndr_cap(dev.bus, dev.slot, dev.func, cap) };
            unsafe {
                match typ {
                    VIRTIO_PCI_CAP_COMMON_CFG => kprintf(b"virtio-gpu: cap common\n\0".as_ptr()),
                    VIRTIO_PCI_CAP_NOTIFY_CFG => kprintf(b"virtio-gpu: cap notify\n\0".as_ptr()),
                    VIRTIO_PCI_CAP_ISR_CFG => kprintf(b"virtio-gpu: cap isr\n\0".as_ptr()),
                    VIRTIO_PCI_CAP_DEVICE_CFG => kprintf(b"virtio-gpu: cap device\n\0".as_ptr()),
                    _ => kprintf(b"virtio-gpu: cap other\n\0".as_ptr()),
                }
            }
            if typ == VIRTIO_PCI_CAP_COMMON_CFG {
                common = Some(loc);
            }
        }
        cap = unsafe { pci_read8(dev.bus, dev.slot, dev.func, cap.wrapping_add(1)) };
    }

    let Some(common) = common else {
        unsafe {
            kprintf(b"virtio-gpu: no common cfg\n\0".as_ptr());
        }
        return;
    };

    if common.bar as usize >= 6 {
        unsafe {
            kprintf(b"virtio-gpu: bad common bar idx\n\0".as_ptr());
        }
        return;
    }

    let bar_raw = dev.bar[common.bar as usize];
    let base = unsafe { map_bar(bar_raw) };
    if base.is_null() {
        unsafe {
            kprintf(b"virtio-gpu: bar map failed (bar phys 0?)\n\0".as_ptr());
        }
        return;
    }

    let status = unsafe { base.add(common.offset as usize + 0x14) };

    unsafe {
        core::ptr::write_volatile(status, 0u8);
        core::ptr::write_volatile(status, VIRTIO_STATUS_ACKOWNLEDGE);
        core::ptr::write_volatile(status, VIRTIO_STATUS_ACKOWNLEDGE | VIRTIO_STATUS_DRIVER);
        let s = core::ptr::read_volatile(status);
        if s == (VIRTIO_STATUS_ACKOWNLEDGE | VIRTIO_STATUS_DRIVER) {
            kprintf(b"virtio-gpu: phase2 ok (status=ACK|DRIVER)\n\0".as_ptr());
        } else {
            kprintf(b"virtio-gpu: phase2 status unexpected\n\0".as_ptr());
        }
    }
}
