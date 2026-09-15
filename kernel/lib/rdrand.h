#pragma once
#include <stdint.h>
#include <stddef.h>

int rdrand_fill(void *buf, size_t len);
void rdrand_selftest(void);
