# VKRUST - Vibe Kernel in RUST

Architecture of the kernel and its POSIX-style shell. Both are written in Rust. The kernel boots from a CD under QEMU. The shell is the only program, it runs in ring 3, and its files live in memory.

This folder explains how that machine is put together and why it is shaped this way. The pictures under [`images/`](images/) are PNG renders of the SVG sources beside them. The graphs are [Mermaid](https://mermaid.js.org/) diagrams; GitHub renders those in the Markdown.

## Picture of the whole system

![The machine, Limine, the kernel, the abi crate, and the shell](images/layers.png)

## Read this in order

| Document | What it explains |
| --- | --- |
| [Overview](overview.md) | Goals, crates, module graph, and the choices that fix the shape |
| [Boot](boot.md) | The ISO, Limine requests, and the path into `kmain` |
| [Memory](memory.md) | The virtual map, frames, and the two heaps |
| [CPU](cpu.md) | The GDT, rings, the IDT, and the interrupt stacks |
| [Syscalls](syscalls.md) | The `int 0x80` ABI and the user-pointer check |
| [Filesystem](vfs.md) | The in-memory tree, file descriptors, and `rename` |
| [Console](console.md) | The framebuffer, serial port, and the tty |
| [Userland](userland.md) | The shell ELF, the builtins, and vim |

Build and run instructions, and a [glossary](../README.md#glossary) of the terms in these pages, stay in the [project README](../README.md).

## A one-screen map of the source

| Path | Role |
| --- | --- |
| `abi/` | Numbers and `repr(C)` records shared by the kernel and the shell |
| `kernel/src/main.rs` | `kmain`, panic, QEMU exit port |
| `kernel/src/boot.rs` | Limine requests |
| `kernel/src/memory.rs` | Frame allocator, page tables, kernel heap |
| `kernel/src/gdt.rs` | GDT, TSS, ring-3 selectors |
| `kernel/src/interrupts.rs` | IDT, PIC, `int 0x80` entry |
| `kernel/src/syscall.rs` | Dispatch and user copies |
| `kernel/src/vfs.rs` | The filesystem |
| `kernel/src/process.rs` | ELF loader and `iretq` into ring 3 |
| `kernel/src/console.rs` | Framebuffer text and ANSI |
| `kernel/src/input.rs` | Keyboard, serial, tty line discipline |
| `kernel/src/serial.rs` | COM1 |
| `user/src/shell.rs` | Prompt, parser, builtins |
| `user/src/vim.rs` | Modal editor |
| `user/src/syscall.rs` | `int 0x80` wrappers |
| `kernel/link.ld`, `user/link.ld` | Link addresses |
