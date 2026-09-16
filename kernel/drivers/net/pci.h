#pragma once
#include <stdint.h>

#define PCI_VENDOR_VIRTIO 0x1AF4
#define PCI_DEVICE_NET 0x1000 // virtio-net legacy

typedef struct
{
	uint16_t vendor;
	uint16_t device;
	uint8_t bus;
	uint8_t slot;
	uint8_t func;
	uint32_t bar[6]; // Base Address Registers
	uint8_t irq;
} pci_device_t;

void pci_init(void);
int pci_find(uint16_t vendor, uint16_t device, pci_device_t *out);
uint32_t pci_read(uint8_t bus, uint8_t slot, uint8_t func, uint8_t offset);
void pci_write(uint8_t bus, uint8_t slot, uint8_t func, uint8_t offset, uint32_t val);
void pci_enable_busmaster(pci_device_t *dev);
