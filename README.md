# fern

A small 64-bit x86_64 kernel and POSIX-style shell, both written in Rust. It boots from a CD image under QEMU. The shell runs in ring 3 and can `ls`, `cd`, `mkdir`, `touch`, and edit files with `vim`. The filesystem is in memory.

## Build and run

Install the tools once:

```bash
sudo apt install build-essential xorriso qemu-system-x86 git
```

If those packages are missing and a Docker image named `fern-osdev` exists, `make` builds the ISO inside that image instead.

Rust nightly (with the `rust-src` component and the `x86_64-unknown-none` target) is selected by `rust-toolchain.toml`. Then:

```bash
make
qemu-system-x86_64 -cdrom target/fern.iso -m 256M
```

`make run` does both. At the `fern:/$ ` prompt, `help` lists the commands. Leave the guest with `poweroff`.

`make test` boots the ISO with no display and checks `ls`, `cd`, `mkdir`, `touch`, and `vim` over the serial port.
