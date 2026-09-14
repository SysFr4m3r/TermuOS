#![no_std]
#![no_main]

extern "C" {
    fn kprintf(fmt: *const u8, ...);
}

#[no_mangle]
pub extern "C" fn example_driver_init() {
    unsafe {
        kprintf(b"example: rust driver ready\n\0".as_ptr());
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
