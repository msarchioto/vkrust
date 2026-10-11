# VKRUST - Vibe Kernel in RUST

A small 64-bit x86_64 kernel and POSIX-style shell, both written in Rust. It boots from a CD image under QEMU. The shell runs in ring 3 and can `ls`, `cd`, `mkdir`, `touch`, `cp`, `mv`, and edit files with `vim`. The filesystem is in memory. The architecture, with diagrams, is documented in [`doc/`](doc/README.md), including a [glossary](doc/README.md#glossary) of the terms those pages use.

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

These frames are the guest framebuffer from `target/vkrust.iso`. Each one starts from `clear`, and the prompt is `vkr-user:/$ `.

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

### Home directory

```text
cd
pwd
ls
cat welcome.txt
```

![cd to home, pwd, ls, and cat welcome.txt](doc/images/showcase/03-cd.png)

### Create a file

```text
cd /
mkdir -p /tmp/tree/nested
echo hello vkrust > /tmp/tree/nested/leaf.txt
cat /tmp/tree/nested/leaf.txt
ls /tmp/tree/nested
```

![mkdir -p, echo into a file, cat, and ls](doc/images/showcase/04-files-write.png)

### Copy, move, and remove

```text
cp /tmp/tree /tmp/nope
cp -r /tmp/tree /tmp/tree2
mv /tmp/tree2/nested/leaf.txt /tmp/tree2/moved.txt
ls /tmp/tree2
cat /tmp/tree2/moved.txt
rm /tmp/tree2/moved.txt
```

![cp refuses a directory, then cp -r, mv, ls, cat, and rm](doc/images/showcase/05-copy-move.png)

### Vim

```text
vim /tmp/note.txt
```

Then `i` and `hello vkrust`. The picture keeps the buffer line and the `-- INSERT --` status line from the bottom of the screen.

![vim insert mode with hello vkrust](doc/images/showcase/06-vim.png)

### Backspace and uname

`echo abX`, then Backspace, then Enter. The `X` is gone before the line runs.

```text
uname
```

![backspace edits the line to ab, then uname](doc/images/showcase/07-backspace-uname.png)

### Poweroff

`poweroff` is typed at the prompt. Enter leaves QEMU.

![poweroff typed at the prompt](doc/images/showcase/08-poweroff.png)
