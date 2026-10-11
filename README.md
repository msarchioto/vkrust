# VKRUST - Vibe Kernel in RUST

A small 64-bit x86_64 kernel and POSIX-style shell, both written in Rust. It boots from a CD image under QEMU. The shell runs in ring 3 and can `ls`, `cd`, `mkdir`, `touch`, `cp`, `mv`, and edit files with `vim`. The filesystem is in memory. The architecture, with diagrams, is documented in [`doc/`](doc/README.md).

## Build and run

Install the tools once:

```bash
sudo apt install build-essential xorriso qemu-system-x86 git
```

If those packages are missing and a Docker image named `vkrust-osdev` exists, `make` builds the ISO inside that image instead.

Rust nightly (with the `rust-src` component and the `x86_64-unknown-none` target) is selected by `rust-toolchain.toml`. Then:

```bash
make
qemu-system-x86_64 -cdrom target/vkrust.iso -m 256M
```

`make run` does both. At the `vkr-user:/$ ` prompt, `help` lists the commands. Leave the guest with `poweroff`.

`make test` boots the ISO with no display and checks `ls`, `cd`, `mkdir`, `touch`, `cp`, `mv`, and `vim` over the serial port.

## Showcase

These frames are from `make run`. The prompt is `vkr-user:/$ `.

### Help

```text
help
```

![help lists the shell commands and the vim keys](doc/images/showcase/01-help.png)

### Files present at boot

```text
ls
cat README
```

![ls of the root and cat of README](doc/images/showcase/02-files.png)

Shots still open, in the same guest: `cd`, `pwd`, and `cat welcome.txt`; `mkdir -p`, `echo`, and `cat`; `cp`, `cp -r`, `mv`, and `rm`; `vim`; Backspace and `uname`; `poweroff`. Name them `03-cd.png` through `08-poweroff.png` in `doc/images/showcase/`.
