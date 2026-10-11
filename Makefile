# Build the vkrust ISO, or boot it.
#
#   make            # userland, kernel, target/vkrust.iso
#   make run        # the same, then QEMU
#   make test       # headless serial check of the shell

ROOT := $(abspath .)
USER_ELF := $(ROOT)/user/target/x86_64-unknown-none/release/shell
KERNEL := $(ROOT)/kernel/target/x86_64-unknown-none/release/kernel
ISO := $(ROOT)/target/vkrust.iso

.PHONY: all user kernel iso run test clean

all: iso

user:
	cd $(ROOT)/user && cargo build --release

kernel: user
	cd $(ROOT)/kernel && USER_ELF=$(USER_ELF) cargo build --release

iso: kernel
	bash $(ROOT)/scripts/make-iso.sh

run: iso
	@if command -v qemu-system-x86_64 >/dev/null 2>&1; then \
		qemu-system-x86_64 -cdrom $(ISO) -m 256M; \
	else \
		docker run --rm -it -v "$(ROOT):/src" -w /src vkrust-osdev \
			qemu-system-x86_64 -cdrom target/vkrust.iso -m 256M; \
	fi

test: iso
	python3 $(ROOT)/scripts/qemu-test.py

clean:
	cd $(ROOT)/user && cargo clean
	cd $(ROOT)/kernel && cargo clean
	rm -rf $(ROOT)/target
