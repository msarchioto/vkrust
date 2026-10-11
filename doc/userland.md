# Userland

The userland is one static ELF named `shell`. It is linked at `0x400000`, embedded in the kernel, and entered at ring 3. Commands are functions in that ELF.

## The program

`user/link.ld` sets the entry to `_start` and the base to `0x400000`. The cargo config selects `x86_64-unknown-none` and static relocation. The build script passes `-z max-page-size=0x1000` so load segments are aligned to a page, which is the unit `process::enter` maps.

`_start` initializes a `LockedHeap` on the 2 MiB mapping at `0x00800000` and calls `shell::run`. A panic formats a line, writes it to fd 1, and exits with status 101. Status 101 is non-zero, so the kernel writes `0x11` to the QEMU port and the test harness treats it as a failure.

The kernel build script expects the release ELF at `user/target/x86_64-unknown-none/release/shell`, or at `USER_ELF` if that variable is set. `include_bytes!` pulls those bytes into the kernel image. Rebuilding the shell and not the kernel would boot the previous shell. `make` builds the user crate first, then the kernel, so the image matches.

## Why builtins

A POSIX system would `exec` an ELF for `/bin/ls`. That needs a program loader that can be called more than once, a way to pass arguments, a way to wait, and somewhere to store the ELF. vkr-user already has a loader, and it runs once.

Keeping `ls` inside the shell means:

- the ISO stays one kernel file,
- argument parsing and redirection live in one place,
- a command can call `mkdir` in a loop without a new address space,
- vim can be a module that flips the tty to raw mode and flips it back.

The cost is that you cannot replace `ls` without rebuilding the shell, and you cannot run a binary you just wrote with `vim`. The filesystem holds text, not executables.

## The prompt

The prompt is `vkr-user:` plus the cwd plus `$ `. On the root that is `vkr-user:/$ `, because `getcwd` returns `/`. After `cd /tmp` it is `vkr-user:/tmp$ `.

The line editor is the kernel's canonical tty. The shell reads fd 0 until the buffer contains a newline. Quotes group one argument. A redirection is recognized only when `>` or `>>` is the second-to-last token. The target is opened `O_WRONLY | O_CREAT`, with `O_TRUNC` or `O_APPEND`, and the command's writes go to that descriptor instead of fd 1. vim ignores redirection and always talks to the console. Its screen is not a stream of output you want in a file.

An unknown command prints `command not found` and returns to the prompt. A missing operand prints `missing operand`. Syscall errors print the sentence from `errno_name`.

## Commands

| Command | Behavior |
| --- | --- |
| `help` | Prints the command list and the vim keys |
| `ls [path]` | Names in the directory, directories with a trailing `/`, sorted. A file argument prints the name. |
| `cd [path]` | Changes directory. No argument uses `/home/user`. |
| `pwd` | Prints the cwd |
| `mkdir [-p] path` | Creates a directory. `-p` creates parents and accepts an existing directory. |
| `touch path` | Creates an empty file, or opens and closes an existing one |
| `cp [-r] src... dest` | Copies a file, or a tree with `-r` |
| `mv src... dest` | Renames, or moves into a directory |
| `cat path...` | Writes each file to the output fd |
| `rm path` | Unlinks a file or an empty directory |
| `echo text` | Writes the arguments and a newline |
| `clear` | Sends the ANSI clear sequence |
| `uname` | Prints `vkr-user 0.1.0 x86_64` |
| `poweroff` | `reboot` syscall. QEMU exits. |
| `vim path` | Opens the modal editor |

`help` is the list the running image will accept. This table matches that list at the time it was written.

## vim

vim is a mode loop over the same `read` syscall, with the tty in raw mode.

| Mode | What a key does |
| --- | --- |
| Normal | `hjkl` or the arrow bytes move. `0` and `$` jump to the ends of the line, as do Home and End. `i`, `a`, and `A` enter insert. `o` opens a line below and enters insert. `x` and Delete remove the character under the cursor. `d` deletes the line. `w` writes the file. `:` enters command mode. |
| Insert | Characters are inserted. Enter splits the line. Backspace joins with the previous line at column 0. Escape returns to normal. |
| Command | `w` writes, `q` quits when the buffer is clean, `q!` quits anyway, `wq` and `x` write and quit. Escape cancels the command. |

The status line shows the path, `[Modified]` when the buffer is dirty, and `-- INSERT --` or `-- NORMAL --`. The screen is redrawn with `ESC[2J` and a cursor-position sequence on every key. The cursor column is clamped to the line, and the viewport scrolls so the cursor row stays on screen. The last row is the status line.

Files load through `open` and `read` into a vector of lines. A missing file starts as one empty line, which is how `vim /tmp/note.txt` creates a file on `:w`. A file larger than 256 KiB is refused. Save writes each line and a newline between lines, using `O_WRONLY | O_CREAT | O_TRUNC`. When `edit` returns `Ok`, vim sets the tty back to canonical and clears the screen, so the prompt is not painted over the last view of the buffer. A failed `save` returns the errno immediately, and the tty stays raw.

Arrow keys work here because the kernel already translated them. The shell prompt does not see those bytes. That split is intentional: the prompt is a line reader, and vim is the editor.

## What `make test` exercises

`scripts/qemu-test.py` boots the ISO with no display, a serial pipe, and the debug-exit device. It waits for `vkr-user:/$ ` and then drives the canonical tty with line-oriented writes.

The script checks `help`, `ls` of `/`, `mkdir`, `touch`, `echo` with truncation, `cat`, `cd`, `pwd`, a vim session (`i`, text, Escape, `:wq`), `mkdir -p`, `cp`, `cp` of a directory, `cp -r` into itself, `cp -r` of a tree, `mv` of a file, `mv` of a directory, `mv` onto an existing file, and `poweroff`.

Each wait searches only the bytes received after the previous mark. The serial log already contains older prompts, and a search of the whole buffer would succeed before the command under test had run.

Process status 0 or 33 is accepted after `poweroff`. 33 is `0x10` shifted by the debug-exit device. Any other status fails the run.
