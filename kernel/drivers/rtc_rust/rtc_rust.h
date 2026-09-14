#pragma once
#include <stdint.h>

void rtc_rust_init(void);
void rtc_rust_read(uint8_t *hour, uint8_t *min, uint8_t *sec);
