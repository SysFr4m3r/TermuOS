const POLY: u32 = 0xEDB8_8320;

fn crc32_update(mut crc: u32, data: &[u8]) -> u32 {
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ POLY;
            } else {
                crc >>= 1;
            }
        }
    }
    crc
}

/// CRC of `len` bytes at `data`. Null/empty -> 0
#[no_mangle]
pub unsafe extern "C" fn crc32(data: *const u8, len: usize) -> u32 {
    if data.is_null() || len == 0 {
        return 0;
    }
    let slice = core::slice::from_raw_parts(data, len);
    !crc32_update(0xFFFF_FFFF, slice)
}

extern "C" {
    fn kprintf(fmt: *const u8, ...);
}

/// Boot smoke test: CRC("123456789") must be 0xCBF43926
#[no_mangle]
pub extern "C" fn crc32_selftest() {
    let msg = b"123456789";
    let got = unsafe { crc32(msg.as_ptr(), msg.len()) };
    let ok = got == 0xCBF4_3926;
    unsafe {
        kprintf(
            b"crc32: selftest %s (got 0x%x)\n\0".as_ptr(),
            if ok {
                b"ok\0".as_ptr()
            } else {
                b"FAIL\0".as_ptr()
            },
            got,
        );
    }
}
