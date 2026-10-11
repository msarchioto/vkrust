# Build the vkrust ISO, or boot it.
#
#   make            # userland, kernel, target/vkrust.iso
#   make run        # the same, then QEMU
#   make test       # headless serial check of the shell

ROOT := $(abspath .)
USER_ELF := $(ROOT)/user/target/x86_64-unknown-none/release/shell
KERNEL := $(ROOT)/kernel/target/x86_64-unknown-none/release/kernel
LOADER := $(ROOT)/loader/target/x86_64-unknown-uefi/release/loader.efi
ISO := $(ROOT)/target/vkrust.iso

.PHONY: all user kernel loader iso run test clean

all: iso

user:
	cd $(ROOT)/user && cargo build --release

kernel: user
	cd $(ROOT)/kernel && USER_ELF=$(USER_ELF) cargo build --release

loader:
	cd $(ROOT)/loader && cargo build --release

iso: kernel loader
	bash $(ROOT)/scripts/make-iso.sh

run: iso
	@fw=$$(bash $(ROOT)/scripts/qemu-fw.sh | tr '\n' ' '); \
	if command -v qemu-system-x86_64 >/dev/null 2>&1; then \
		qemu-system-x86_64 $$fw -cdrom $(ISO) -m 256M -boot d; \
	else \
		docker run --rm -it \
			-v "$(ROOT):/src" -w /src \
			-v /usr/share/ovmf:/usr/share/ovmf:ro \
			-v /usr/share/OVMF:/usr/share/OVMF:ro \
			-v /usr/share/qemu:/usr/share/qemu:ro \
			vkrust-osdev \
			qemu-system-x86_64 $$fw -cdrom target/vkrust.iso -m 256M -boot d; \
	fi

test: iso
	python3 $(ROOT)/scripts/qemu-test.py

clean:
	cd $(ROOT)/user && cargo clean
	cd $(ROOT)/kernel && cargo clean
	cd $(ROOT)/loader && cargo clean
	rm -rf $(ROOT)/target
