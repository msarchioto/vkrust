#!/usr/bin/env python3
"""Boot target/fern.iso and exercise the shell over the serial port."""

import os
import shutil
import subprocess
import sys
import time

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
ISO = os.path.join(ROOT, "target", "fern.iso")
TIMEOUT = 180


def qemu_cmd():
    args = [
        "-cdrom", ISO,
        "-m", "256M",
        "-display", "none",
        "-monitor", "none",
        "-serial", "stdio",
        "-no-reboot",
        "-boot", "d",
        "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04",
    ]
    if shutil.which("qemu-system-x86_64"):
        return ["qemu-system-x86_64", *args]
    if shutil.which("docker"):
        return [
            "docker", "run", "--rm", "-i",
            "-v", f"{ROOT}:/src", "-w", "/src", "fern-osdev",
            "qemu-system-x86_64",
            "-cdrom", "target/fern.iso",
            *args[2:],
        ]
    sys.exit("qemu-system-x86_64 is not installed and docker is unavailable")


class Guest:
    def __init__(self, proc):
        self.proc = proc
        self.buf = ""
        self.mark_at = 0
        self.deadline = time.monotonic() + TIMEOUT

    def mark(self):
        """Ignore output that arrived before the next command."""
        self.mark_at = len(self.buf)

    def since(self):
        return self.buf[self.mark_at:]

    def wait_for(self, needle, timeout=None):
        end = time.monotonic() + (timeout or 60)
        end = min(end, self.deadline)
        while needle not in self.since():
            if time.monotonic() > end:
                tail = self.since()[-1500:]
                raise TimeoutError(f"timed out waiting for {needle!r}\n---\n{tail}")
            if self.proc.poll() is not None and not self._read_some(0.1):
                tail = self.since()[-1500:]
                raise RuntimeError(
                    f"qemu exited {self.proc.returncode} before {needle!r}\n---\n{tail}"
                )
            self._read_some(0.2)
        return self.since()

    def _read_some(self, wait):
        import select
        ready, _, _ = select.select([self.proc.stdout], [], [], wait)
        if not ready:
            return False
        chunk = os.read(self.proc.stdout.fileno(), 65536)
        if not chunk:
            return False
        self.buf += chunk.decode("utf-8", "replace")
        return True

    def send(self, data):
        if isinstance(data, str):
            data = data.encode()
        self.proc.stdin.write(data)
        self.proc.stdin.flush()

    def close(self):
        try:
            self.proc.stdin.close()
        except Exception:
            pass
        try:
            self.proc.wait(timeout=20)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.proc.wait(timeout=5)


def expect(text, needle):
    if needle not in text:
        raise AssertionError(f"missing {needle!r} in output")


def main():
    if not os.path.isfile(ISO):
        sys.exit(f"missing {ISO}; run make first")
    proc = subprocess.Popen(
        qemu_cmd(),
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        cwd=ROOT,
    )
    guest = Guest(proc)
    try:
        guest.wait_for("fern:/$ ", timeout=120)
        guest.mark()
        guest.send("help\n")
        guest.wait_for("poweroff")
        guest.wait_for("fern:/$ ")

        guest.mark()
        guest.send("ls\n")
        out = guest.wait_for("fern:/$ ")
        expect(out, "README")
        expect(out, "home/")
        expect(out, "tmp/")

        guest.mark()
        guest.send("mkdir /tmp/docs\n")
        guest.wait_for("fern:/$ ")
        guest.mark()
        guest.send("touch /tmp/docs/a.txt\n")
        guest.wait_for("fern:/$ ")
        guest.mark()
        guest.send("echo hello fern > /tmp/docs/a.txt\n")
        guest.wait_for("fern:/$ ")
        guest.mark()
        guest.send("cat /tmp/docs/a.txt\n")
        out = guest.wait_for("fern:/$ ")
        expect(out, "hello fern")

        guest.mark()
        guest.send("cd /tmp/docs\n")
        guest.wait_for("fern:/tmp/docs$ ")
        guest.mark()
        guest.send("pwd\n")
        out = guest.wait_for("fern:/tmp/docs$ ")
        expect(out, "/tmp/docs")
        guest.mark()
        guest.send("ls\n")
        out = guest.wait_for("fern:/tmp/docs$ ")
        expect(out, "a.txt")

        guest.mark()
        guest.send("vim /tmp/note.txt\n")
        guest.wait_for("note.txt", timeout=30)
        guest.mark()
        guest.send(b"i")
        guest.wait_for("-- INSERT --", timeout=20)
        guest.mark()
        guest.send(b"hello fern")
        guest.wait_for("hello fern", timeout=20)
        guest.mark()
        guest.send(b"\x1b")
        guest.wait_for("-- NORMAL --", timeout=20)
        guest.mark()
        guest.send(b":wq\n")
        guest.wait_for("fern:/tmp/docs$ ", timeout=30)
        guest.mark()
        guest.send("cat /tmp/note.txt\n")
        out = guest.wait_for("fern:/tmp/docs$ ")
        expect(out, "hello fern")

        guest.mark()
        guest.send("poweroff\n")
        guest.wait_for("poweroff", timeout=20)
        # Drain until QEMU exits. 0x10 on port 0xf4 becomes status 33.
        deadline = time.monotonic() + 30
        while proc.poll() is None and time.monotonic() < deadline:
            guest._read_some(0.2)
        code = proc.wait(timeout=10)
        if code not in (0, 33):
            tail = guest.buf[-1500:]
            raise SystemExit(f"qemu exit {code}, wanted 33\n---\n{tail}")
        print("ok")
    except Exception:
        tail = guest.since()[-2000:]
        print("--- guest output ---", file=sys.stderr)
        print(tail, file=sys.stderr)
        raise
    finally:
        guest.close()


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        sys.exit(1)
