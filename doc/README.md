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

Build and run instructions stay in the [project README](../README.md). Terms used in these pages are in the [glossary](#glossary) at the end of this page.

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

## Glossary

Terms used in these pages. The documents above are the long form.

| Term | Meaning |
| --- | --- |
| `abi`, ABI | The crate shared by the kernel and the shell. It holds the Linux x86_64 syscall numbers, open flags, errno values, and the `repr(C)` layouts of `Stat`, `DirEntry`, and `WinSize`. |
| `abi_x86_interrupt` | Nightly Rust calling convention for a handler the CPU enters with an interrupt frame already on the stack. Fault and IRQ handlers use it. |
| ACPI | Firmware tables. Limine's higher-half map covers them. The kernel leaves them unread. |
| `alloc` | Rust's heap crate. Both programs are `no_std` and use `alloc` for `Vec` and `String` on their own heaps. |
| ANSI | Escape sequences that start with `ESC` (`0x1b`). The console implements `ESC[2J` (clear), `ESC[H` and `ESC[row;colH` (cursor), and `ESC[K` (clear to end of line). Serial arrow keys arrive as `ESC [` sequences. |
| base revision | The Limine protocol generation the kernel requests. vkr-user asks for revision 4. After a successful boot the revision word is 0. A loader that ignores the request leaves it unchanged, and `kmain` panics. |
| baud, 8N1 | COM1 runs at 115200 baud, 8 data bits, no parity, one stop bit. |
| BIOS | Legacy PC firmware. It boots the CD through the El Torito image. |
| `.bss`, `.data`, `.rodata`, `.text` | ELF sections. `.text` is code, `.rodata` is read-only data, `.data` is writable data, `.bss` is zeros. Limine requests live in `.data` so the loader can fill them. The RSP0 and double-fault stacks are `#[link_section = ".bss"]`. An all-zero `static` would otherwise land in `.rodata`, and the CPU has to be able to push an interrupt frame onto those stacks. |
| builtin | A command implemented as a function inside the shell ELF. `ls`, `cd`, `mkdir`, `touch`, `cp`, `mv`, `rm`, `cat`, `echo`, `vim`, `poweroff`, and the rest of `help` are builtins. |
| bump allocator, `FrameAlloc`, frame | Physical memory is handed out 4 KiB at a time and never freed. Frame *n* is the n-th usable frame in the memory map. |
| canonical address | An x86_64 virtual address whose top bits repeat bit 47. The hole between the low half and the high half is non-canonical. A syscall range must end at or below `0x0000800000000000`. |
| canonical mode | The tty mode of the shell prompt. Input is echoed and returned as one line when Enter is pressed. Arrow keys are ignored. |
| cell | One character in the framebuffer grid. A cell stores a character. Backspace in canonical mode erases one by writing backspace, space, backspace. |
| `chdir` | Syscall 80. The path must name a directory. |
| `clobber_abi("sysv64")` | Asm annotation on the shell's `int 0x80`. It tells the compiler that caller-saved registers, including XMM, may be destroyed across the call. |
| code model | `code-model=kernel`. LLVM then assumes the kernel sits in the top 2 GiB and addresses its own data with ordinary displacements. |
| codegen unit, LTO | Release builds compile each crate as one LLVM unit and leave link-time optimization off, so a rebuild stays cheap. |
| `COM1`, serial, UART | The PC serial port at I/O `0x3F8`, driven by `kernel/src/serial.rs`. Boot logs and console output are written here, and received bytes feed the tty. IRQ4, vector 36. The line-status register is polled as well as the IRQ: data-ready on input, transmitter-empty on output. `make test` drives the shell through this port. |
| `copy_nonoverlapping` | The byte copy used for syscall buffers. SMAP is off, so this is an ordinary copy. `user_ok` is what keeps the range inside the shell. |
| `CR2` | The address that caused a page fault. The page-fault handler prints it. |
| `CR3` | The physical address of the top-level page table. The kernel adds the HHDM offset and walks the table from there. |
| `CR4` | The control register that holds SMAP and SMEP. `kmain` clears both before any user page exists. |
| C string, NUL | A path as the kernel reads it: bytes ending in a zero. The shell appends that zero. More than 256 bytes, counting the zero, returns `ENAMETOOLONG`. |
| cwd | The current directory: one node index, because there is one process. `getcwd` walks parents to the root and joins names. A bare `cd` selects `/home/user`. Unlinking the cwd moves it to the parent. |
| debug-exit, port `0xf4` | QEMU's `isa-debug-exit` device. The kernel writes `0x10` for success and `0x11` for failure, then halts. QEMU turns value `v` into process status `(v << 1) \| 1`, so success is 33 and failure is 35. |
| demand paging | Mapping a page only when a program touches it. vkr-user maps every shell page before `iretq`. A missing page is a fatal fault. |
| descriptor | One slot in the GDT (a segment or the TSS) or one slot in the IDT (a gate). |
| `DirEntry` | The `repr(C)` record `getdents` writes: `inode`, `kind`, `name_len`, and a 64-byte `name`. |
| double fault, `DF_STACK` | Exception vector 8, raised when delivering another exception itself faults. The handler runs on its own 64 KiB stack and exits with `0x11`. |
| DPL | Descriptor privilege level of an IDT gate. The `int 0x80` gate has DPL 3, so ring 3 may invoke it. Any other software interrupt from the shell is a general-protection fault. |
| dynamic linker | The loader that would apply relocations at run time. Both ELFs are linked at fixed addresses, so the link address is the run address. |
| El Torito | The BIOS CD boot format. The ISO carries a no-emulation boot image, `limine-bios-cd.bin`. |
| ELF | Executable and Linkable Format. The kernel is a higher-half ELF. The shell is a static ELF linked at `0x400000` and embedded in the kernel. The kernel never loads a second one. |
| EOI | End of interrupt. A keyboard or serial handler tells the PIC it is done so the same IRQ can fire again. |
| errno | A negative `rax` return. Names match Linux: `ENOENT` 2, `EIO` 5, `EBADF` 9, `EFAULT` 14, `EEXIST` 17, `ENOTDIR` 20, `EISDIR` 21, `EINVAL` 22, `ENOTTY` 25, `ENOSPC` 28 (the descriptor table is full), `ENAMETOOLONG` 36, `ENOSYS` 38, `ENOTEMPTY` 39. `abi::errno_name` prints the sentence. Any other code, including vim's 27 for a file over 256 KiB, prints as `Error`. |
| `exec` | The POSIX operation that replaces the running program with another ELF. The loader runs once. Commands are functions in the shell. |
| `exit` | Syscall 60. Status 0 writes `0x10` to the debug-exit port. Any other status writes `0x11`. A shell panic calls it with status 101. |
| file descriptor, fd | A small integer the shell passes to `read`, `write`, `close`, and `ioctl`. Slots 0, 1, and 2 are the tty for the whole boot. `close` on them succeeds and does nothing. `open` returns slots 3 through 31. |
| firmware | BIOS or UEFI. It loads Limine, and Limine loads the kernel. |
| `font8x8`, glyph | The 8×8 bitmap font, indexed by Unicode scalar. The least significant bit of a row is the leftmost pixel. Modes at least 900 pixels wide draw each glyph at twice the size. |
| `fork` | The POSIX operation that clones a process. vkr-user has one process, the shell. |
| framebuffer | The pixel buffer Limine reports: base, pitch (bytes per row), size, and red, green, and blue mask shifts. `memory_model` 1 means RGB, which is QEMU's linear framebuffer. With no framebuffer the console still runs on COM1 and reports 24 rows by 80 columns. |
| gate | An IDT entry: a handler address, a privilege (DPL), and sometimes a dedicated stack. |
| GDT | Global Descriptor Table. Loaded before the IDT, which records the current CS. Slots: null, kernel code `0x08`, kernel data `0x10`, user data `0x18`, user code `0x20`, then the TSS. User data comes before user code so the ring-3 selectors stay `0x1b` and `0x23`. |
| general-protection fault | Exception vector 13. A privilege or segment violation, including any software interrupt from the shell other than `int 0x80`. The handler prints the frame and exits with `0x11`. |
| `getcwd` | Syscall 79. Writes the cwd path and returns the length, excluding the trailing zero. |
| `getdents` | Syscall 217. Returns one directory child per call, as a `DirEntry`. A return of 0 means the cursor is past the last child. `.` and `..` are not stored, so they are not returned. The shell sorts. |
| HHDM | Higher-half direct map. Limine's offset where virtual address = physical address + offset. The kernel zeros new frames through this window and walks page tables through it. |
| higher half | The top of the canonical address space. The kernel is linked at `0xffffffff80000000`. Limine maps usable RAM, bootloader-reclaimable memory, the kernel, modules, the framebuffer, ACPI, and reserved firmware regions there. The shell is embedded, so no module is loaded. |
| `hlt` | Halt until the next interrupt. A tty `read` with nothing pending waits with `sti; hlt`. |
| identity map | A mapping where the virtual address equals the physical address. Limine dropped the low identity map at revision 1, so the shell's addresses stay empty until `process::enter`. |
| IDT | Interrupt Descriptor Table. Installed gates: breakpoint 3 (print and continue), invalid opcode 6, double fault 8, general protection 13, page fault 14, keyboard 33, serial 36, and `int 0x80`. |
| IF | Interrupt flag, bit 9 of `RFLAGS`. The shell is entered with IF set, so a key can arrive while it runs. |
| `include_bytes!`, `USER_ELF` | The shell ELF is compiled into the kernel. The default path is `user/target/x86_64-unknown-none/release/shell`. `USER_ELF` overrides it. `make` builds the shell first. |
| `inlateout` | The asm operand kind on `rax` in the shell wrapper: the syscall number goes out, and the result comes back in the same register. |
| inode | The identity field of a directory record. Every `DirEntry.inode` is 1. The stable identity is the VFS node index, and listings leave that index in the kernel. |
| `int 0x80` | The software interrupt that is every syscall. One IDT gate, DPL 3, running on the RSP0 stack. |
| interrupt frame | The five qwords the CPU pushes on an interrupt: SS, RSP, RFLAGS, CS, RIP. `iretq` pops them. `process::enter` builds this frame by hand. |
| invalid opcode | Exception vector 6. Print the frame and exit with `0x11`. |
| `ioctl` | Syscall 16. Two requests, both on fd 0 or 1: `TCSETS` and `TIOCGWINSZ`. Any other request is `ENOTTY`. |
| IOPL | I/O privilege level, in `RFLAGS`. The shell runs at IOPL 0, so `in` and `out` fault. The serial port is reached through `write`. |
| IRQ | A hardware interrupt line. IRQ1 is the keyboard (vector 33). IRQ4 is COM1 (vector 36). IRQ0, the timer, stays masked. |
| `iretq` | Return from interrupt. It pops an interrupt frame and can drop from ring 0 to ring 3. Entering the shell and returning from a syscall both use it. |
| ISO, hybrid image, `xorriso` | `target/vkrust.iso`, written by `xorriso`: an El Torito BIOS boot, a UEFI boot file, and a protective MBR. `limine bios-install` then plants the BIOS stages and marks partition 1 active. |
| ISR | Interrupt service routine. The keyboard ISR reads port `0x60`. The serial ISR drains COM1's data-ready bit. Each pushes a byte into a ring, sends EOI, and returns. Handlers only touch that ring. |
| IST | Interrupt stack table, inside the TSS. Slot 0 is the double-fault stack. The gate selects it with `set_stack_index`. Other interrupts from ring 3 use RSP0, which is a privilege stack, not an IST slot. |
| `KEEP`, `.requests`, `#[used]`, `boot()` | Limine requests are tagged structs in the `.requests` section, inside `.data`, between start and end markers. `KEEP` and `#[used]` stop the linker from dropping them, and `boot::take` names each one. `boot():/boot/kernel` means the kernel file on the device Limine itself was loaded from. |
| `kmain` | The kernel entry, `extern "C" -> !`. Limine jumps here and `kmain` does not return. Order: serial, clear SMAP and SMEP, take the Limine answers, GDT, input rings, IDT and PIC, `sti`, page tables, kernel heap, console, VFS, then `process::enter`. |
| Limine | The bootloader, pinned at tag `v10.8.5-binary`. The kernel speaks base revision 4 through requests in its own ELF. The CD config has timeout 0 and one entry. |
| line discipline | The tty policy that turns keyboard and serial bytes into the stream `read` returns. Canonical and raw are the two disciplines. |
| `linked_list_allocator`, `LockedHeap` | The heap implementation. The kernel heap is 4 MiB at `0x0000444444440000`, mapped supervisor-only. The shell heap is a separate `LockedHeap` of 2 MiB at `0x00800000`. Interrupt handlers only push a byte into a ring. Running out of heap aborts. `ENOSPC` means the descriptor table is full. |
| lost wakeup | The reason the tty waits with `sti` then `hlt`. An interrupt in that window still wakes the CPU. The tty lock is dropped before the wait. |
| `ltr` | Load task register. It points the CPU at the TSS selector in the GDT. |
| mark | How `make test` reads the serial log. Each wait searches only the bytes that arrived after the previous prompt, so an older prompt cannot satisfy the next command. |
| MBR | Master boot record. On this hybrid ISO it is a protective partition table. `limine bios-install` writes the BIOS stages into it. |
| memory map, PC hole | Limine's list of physical regions. The frame allocator keeps up to 64 usable regions (type 0), rounded in to whole frames, and skips everything below 1 MiB, where firmware tables and legacy devices sit. |
| Mermaid | The diagram language in the docs. GitHub renders the fenced blocks. The files in [`images/`](images/) are PNG renders of the SVGs next to them. |
| `mkdir` | Syscall 83. Creates one path component. The parent must already exist. `mkdir -p` is a loop in the shell. |
| monolithic kernel | Drivers, the filesystem, the tty, and syscall dispatch live in one address space and call each other as ordinary Rust functions. |
| naked function, `syscall_entry` | A function with no compiler prologue. The syscall stub is naked: the CPU has already pushed the interrupt frame, and the stub pushes `rax`, `rcx`, `rdx`, `rsi`, `rdi`, `r8`, `r9`, `r10`, `r11`, and `rbx` itself. |
| nightly | The Rust channel in `rust-toolchain.toml`, with `rust-src` and the `x86_64-unknown-none` target. `abi_x86_interrupt` and naked functions need it. |
| node | One file or directory in the VFS: an index into a `Vec`. It has a name of at most 64 bytes, a parent, a `live` flag, and either children or file bytes. Unlink clears `live` and leaves the slot in place, so an open descriptor still works. |
| `no_std` | The crate has no Rust standard library and no operating-system runtime. The kernel and the shell are both `no_std`. |
| NX | The page-table bit that forbids fetching instructions. Shell pages leave it clear, so they are writable and executable. |
| `OffsetPageTable` | The page-table walker from the `x86_64` crate. It follows a 4-level table through the HHDM offset. |
| open flags | Linux values from `abi`: `O_RDONLY` 0, `O_WRONLY` 1, `O_RDWR` 2, `O_CREAT` `0x40`, `O_TRUNC` `0x200`, `O_APPEND` `0x400`, `O_DIRECTORY` `0x10000`. A writable `O_TRUNC` clears a file. `O_APPEND` sets the offset to the current length at open. `O_DIRECTORY` on a file is `ENOTDIR`. Truncating a directory is `EISDIR`. |
| page | 4 KiB, the unit of mapping. The shell is linked with `-z max-page-size=0x1000` so each ELF segment is aligned to that unit. |
| page fault | Exception vector 14. The handler prints `CR2` and the error code and exits with `0x11`. |
| page flags, user bit, supervisor page | Shell pages are mapped `PRESENT`, `WRITABLE`, and `USER_ACCESSIBLE`. The kernel heap omits the user bit, so ring 3 cannot touch it. If a virtual page is already mapped, `map_page` keeps the frame and `update_flags` applies the new flags, because two ELF segments can share one page. |
| page table, paging mode, 4-level paging | The tree `CR3` points at. The Limine paging-mode request sets mode, max, and min to 0, which asks for 4-level paging, the shape `OffsetPageTable` walks. vkr-user keeps Limine's tables and adds the shell's mappings. |
| panic, unwinder | An unwinder walks frames after a panic. Here a panic aborts in place: the kernel prints to serial and exits QEMU with `0x11`, and the shell writes to fd 1 and exits with status 101. |
| `pc-keyboard`, PS/2, scancode | The keyboard controller, data port `0x60`, IRQ1. A scancode is the byte sent on press and on release. `pc-keyboard` decodes scancode set 1 with a UK 105-key layout, so Shift, Ctrl, and Alt turn off again. `£` and `¬` are kept as Latin-1 bytes. |
| pending queue | Bytes the tty has accepted and not yet returned from `read`. Canonical mode queues a finished line. Raw mode queues each key. |
| `PF_X`, `PF_W` | ELF flags for an executable or writable segment. The loader ignores them and uses one flag set for the whole shell image. |
| PIC | The 8259 interrupt controller, a master and a slave. IRQs are remapped to vectors 32 and 40 so they do not overlap CPU exceptions. The master mask `0xED` leaves the keyboard and COM1 enabled. The slave mask `0xFF` leaves the slave quiet. |
| POSIX | The Unix interface the syscall numbers and the shell commands follow. The implemented set is the one in `abi`. |
| `poweroff`, `reboot` | The way out of the guest. The shell's `poweroff` is syscall 169 (`reboot`). The argument is ignored. The kernel writes `0x10` to port `0xf4`. |
| `process::enter` | Maps the embedded shell, the user stack (`0x00600000 .. 0x00700000`, top exclusive), and the user heap, records those ranges, and executes `iretq` into ring 3. This is the last thing `kmain` does. |
| program header, `PT_LOAD` | An ELF segment the loader maps. File bytes are copied to the segment's virtual address. Bytes past the file size stay zero, which is how `.bss` arrives. The shell must fit in `0x400000 .. 0x600000`. |
| QEMU | The emulator, `qemu-system-x86_64`. `make run` boots the ISO with 256 MiB. `make test` boots it with no display, a serial pipe, and the debug-exit device. A triple fault with `-no-reboot` exits 0 and prints nothing. |
| raw mode | The tty mode vim uses. `set_raw(true)` is `ioctl(TCSETS)` with argument 1, and it drops any partial line. Each key is one byte, with no echo: Enter `\n`, Backspace `0x7f`, Escape `0x1b`, arrows and Home, End, Delete `0x80` through `0x86`. |
| recorded region, `user_ok` | Up to eight ranges saved at load: the user stack, the user heap, and each `PT_LOAD` rounded out to pages. A syscall pointer must lie entirely inside one range, must not wrap, and must end at or below the canonical ceiling. Copies are capped at 1 MiB. The current shell uses six ranges. |
| red zone | The 128 bytes below RSP that System V leaf functions may use. `x86_64-unknown-none` has no red zone, so an interrupt can push a frame at the current stack pointer. |
| redirection | Shell syntax. `>` or `>>` counts only when it is the second-to-last token. The file is opened for writing, created if needed, truncated or appended, and the command's output goes there. vim ignores redirection. |
| registers | On `int 0x80`, `rax` is the syscall number and the return value. Arguments are `rdi`, `rsi`, `rdx`, and `r10`. The fourth argument is `r10`, matching Linux, where the `syscall` instruction overwrites `rcx`. |
| relocation model | `relocation-model=static`. The ELF contains no dynamic relocations. |
| `rename` | Syscall 82, and what `mv` calls. The node moves to a new parent and a new name. Bytes are not copied. A directory moved into itself is `EINVAL`. Replacing a non-empty directory is `ENOTEMPTY`. A file and a directory cannot replace each other. |
| `repr(C)` | Rust layout that fixes field order to the C ABI. The shell and the kernel share `Stat`, `DirEntry`, and `WinSize` through it. |
| `RFLAGS` | The flags register. The shell's entry frame sets it to `0x202`: reserved bit 1, and IF set. IOPL is 0. |
| ring, RPL | Privilege level. Ring 0 is the kernel. Ring 3 is the shell. RPL is the low two bits of a segment selector, and the CPU checks it against the descriptor. `user_cs` and `user_ss` are `0x23` and `0x1b`, both RPL 3. The hardware rejects a ring-0 selector on an `iretq` into user code. |
| ring buffer | The two 512-byte queues filled by the keyboard and serial ISRs. A full ring drops the new byte. The tty drains them with interrupts disabled. |
| `RSP0`, boot stack | The kernel stack used for any interrupt taken in ring 3, including `int 0x80`. `RSP0_STACK` is 64 KiB in `.bss`. Limine's own 128 KiB stack is only the stack that enters `kmain`. |
| scheduler, tick | The shell runs until it makes a syscall or a device IRQ fires. The timer IRQ stays masked, so a tick does not preempt it. |
| segment, selector | A GDT entry chosen by CS or SS. In 64-bit mode the selector carries privilege. The selector is a 16-bit index plus an RPL. |
| signal | A Unix notice delivered to a process. A fault here prints the frame on serial and exits QEMU. |
| SMAP | Supervisor Mode Access Prevention, a `CR4` bit. When set, a kernel load from a user address faults. It is cleared so syscall copies can be ordinary loads. |
| SMEP | Supervisor Mode Execution Prevention, a `CR4` bit. When set, the kernel cannot execute a user page. It is cleared at boot next to SMAP. |
| SSE, XMM | The vector registers. The syscall stub does not save them. The kernel may use them while a syscall is in progress. The shell marks them clobbered. |
| `_start` | The shell's entry symbol. It initializes `LockedHeap` on the 2 MiB user heap and calls `shell::run`. |
| `stat`, `Stat` | Syscall 4. Writes a `repr(C)` record of `kind` and `size`. Kind is `S_FILE` (1) or `S_DIR` (2). |
| `sti` | Set the interrupt flag. `interrupts::init` executes it after the PIC is programmed. The tty's wait is `sti` followed by `hlt`. |
| syscall | A request from the shell, made with `int 0x80`. The number in `rax` is the Linux x86_64 value: `read` 0, `write` 1, `open` 2, `close` 3, `stat` 4, `ioctl` 16, `exit` 60, `getcwd` 79, `chdir` 80, `rename` 82, `mkdir` 83, `unlink` 87, `reboot` 169, `getdents` 217. Anything else returns `-ENOSYS`. |
| `syscall` instruction | The fast Linux entry. It needs the `STAR` and `LSTAR` MSRs and `EFER.SCE`, and it skips the IDT. vkr-user enters through `int 0x80`, which is one gate on the same stack path as a fault. |
| System V ABI, trampoline | The x86_64 C calling convention. After the naked stub saves registers, an `extern "C"` trampoline receives that block in `rdi`, calls `syscall::dispatch`, and writes the result over the saved `rax`. RSP at that `call` is 8 bytes off the ABI's 16-byte alignment. The dispatcher makes no aligned SSE store, so the call returns. |
| `TCSETS` | `ioctl` request `0x5402`. Argument 0 sets canonical mode. Argument 1 sets raw mode. |
| `TIOCGWINSZ`, `WinSize` | `ioctl` request `0x5413`. It writes a `repr(C)` record of `rows` and `cols` for the framebuffer grid, or 24 and 80 when there is no framebuffer. |
| triple fault | A fault while delivering the double fault. The CPU resets. Under QEMU `-no-reboot` the process exits 0, and the serial log never shows the original fault. |
| TSS | Task State Segment. It holds RSP0 and the interrupt stack table. In the GDT it is a 16-byte system descriptor, so it occupies two slots, and nothing is appended after it. Loaded with `ltr`. |
| tty | The terminal in `kernel/src/input.rs`. File descriptors 0, 1, and 2. Input is the PS/2 keyboard and COM1, decoded into one byte stream. Output is the framebuffer, when present, and always COM1. |
| UEFI | The other firmware path. The ISO carries `EFI/BOOT/BOOTX64.EFI` and `limine-uefi-cd.bin`. |
| `unlink` | Syscall 87. Clears `live` and removes the node from its parent. The `Vec` slot stays, so a descriptor opened earlier still reads and writes. A non-empty directory fails. |
| userland | The shell, the only ring-3 program. |
| vector | The IDT index of an exception or interrupt. CPU exceptions use 0 through 31. The PIC delivers the master IRQs from 32 and the slave IRQs from 40. |
| VFS | The filesystem: a `Vec` of nodes behind one mutex, stored in RAM. Children stay in insertion order, and `ls` sorts in the shell. Boot seeds `/`, `/README`, `/home`, `/home/user`, `/home/user/welcome.txt`, and `/tmp`. Rebooting discards the tree. |
| vim | The modal editor builtin. It sets the tty raw around its loop. Normal, insert, and command modes share one `read` loop. A file over 256 KiB is refused. A missing file starts empty and is created on write. |
| vkr-user | The name printed at boot and in the prompt (`vkr-user:/$ `). This repository is VKRUST. |
| `x86_64-unknown-none` | The freestanding Rust target both crates build for. No C library and no red zone. |
