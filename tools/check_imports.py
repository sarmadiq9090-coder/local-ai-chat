#!/usr/bin/env python3
"""Fails if local-ai-chat.exe imports a DLL that is not part of Windows.

    python3 tools/check_imports.py target/release/local-ai-chat.exe

Reads the PE import and delay-load import tables with the standard library
only (no dumpbin needed) and prints every DLL. Each must be on ALLOWED (or
match an ALLOWED_PREFIXES entry). The Visual C++ runtime (VCRUNTIME*, MSVCP*,
the UCRT api-ms-win-crt-* forwarders, ucrtbase) must never appear: it is not
installed on every PC, so the exe would not start there. .cargo/config.toml
links the C runtime statically (+crt-static) to keep it out. GitHub's Windows
runners have the runtime installed, so only this check catches a regression;
CI (ci.yml, job "build") runs it on the release build.
"""
import struct
import sys

# DLLs that ship with every supported Windows version (10/11). Lower case.
ALLOWED = {
    "advapi32.dll", "bcrypt.dll", "bcryptprimitives.dll", "combase.dll",
    "comctl32.dll", "comdlg32.dll", "crypt32.dll", "d2d1.dll", "d3d11.dll",
    "dbghelp.dll", "dwmapi.dll", "dwrite.dll", "dxgi.dll", "gdi32.dll",
    "imm32.dll", "iphlpapi.dll", "kernel32.dll", "kernelbase.dll",
    "msimg32.dll", "mswsock.dll", "ncrypt.dll", "ntdll.dll", "ole32.dll",
    "oleaut32.dll", "powrprof.dll", "propsys.dll", "rpcrt4.dll", "secur32.dll",
    "setupapi.dll", "shcore.dll", "shell32.dll", "shlwapi.dll",
    "uiautomationcore.dll", "urlmon.dll", "user32.dll", "userenv.dll",
    "uxtheme.dll", "version.dll", "winhttp.dll", "wininet.dll", "winmm.dll",
    "ws2_32.dll",
}
# Windows API sets (api-ms-win-core-*, ...). api-ms-win-crt-* is the UCRT and
# is rejected below before this list is consulted.
ALLOWED_PREFIXES = ("api-ms-win-core-", "api-ms-win-security-", "api-ms-win-shcore-")
FORBIDDEN_PREFIXES = ("vcruntime", "msvcp", "msvcr", "ucrtbase", "api-ms-win-crt-", "concrt", "vccorlib")


def rva_to_offset(sections, rva):
    for vaddr, vsize, raw_ptr, raw_size in sections:
        if vaddr <= rva < vaddr + max(vsize, raw_size):
            return raw_ptr + (rva - vaddr)
    raise ValueError(f"RVA {rva:#x} is outside every section")


def cstr(data, off):
    end = data.index(b"\0", off)
    return data[off:end].decode("ascii", "replace")


def imported_dlls(path):
    with open(path, "rb") as f:
        data = f.read()
    if data[:2] != b"MZ":
        raise ValueError("not a PE file (no MZ header)")
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe:pe + 4] != b"PE\0\0":
        raise ValueError("not a PE file (no PE signature)")
    n_sections, = struct.unpack_from("<H", data, pe + 6)
    opt_size, = struct.unpack_from("<H", data, pe + 20)
    opt = pe + 24
    magic, = struct.unpack_from("<H", data, opt)
    dirs = opt + (112 if magic == 0x20B else 96)  # PE32+ / PE32
    sections = []
    sec = opt + opt_size
    for i in range(n_sections):
        vsize, vaddr, raw_size, raw_ptr = struct.unpack_from("<IIII", data, sec + i * 40 + 8)
        sections.append((vaddr, vsize, raw_ptr, raw_size))

    found = []
    # Data directory 1 = imports (20-byte descriptors, name RVA at +12),
    # 13 = delay-load imports (32-byte descriptors, name RVA at +4).
    for index, size, name_at, kind in ((1, 20, 12, "import"), (13, 32, 4, "delay-load")):
        rva, _ = struct.unpack_from("<II", data, dirs + index * 8)
        if rva == 0:
            continue
        off = rva_to_offset(sections, rva)
        while True:
            desc = data[off:off + size]
            if len(desc) < size or desc == b"\0" * size:
                break
            name_rva, = struct.unpack_from("<I", desc, name_at)
            found.append((cstr(data, rva_to_offset(sections, name_rva)), kind))
            off += size
    return found


def main():
    if len(sys.argv) != 2:
        print(__doc__.strip().splitlines()[2].strip())
        return 2
    bad = []
    for name, kind in imported_dlls(sys.argv[1]):
        low = name.lower()
        if low.startswith(FORBIDDEN_PREFIXES):
            verdict = "FORBIDDEN (Visual C++ runtime)"
        elif low in ALLOWED or low.startswith(ALLOWED_PREFIXES):
            verdict = "ok"
        else:
            verdict = "NOT ALLOWED (not a Windows system DLL)"
        print(f"  {name:<40} {kind:<10} {verdict}")
        if verdict != "ok":
            bad.append(name)
    if bad:
        print(f"::error::{sys.argv[1]} imports DLLs that are not part of Windows: "
              + ", ".join(bad)
              + ". The exe would fail to start on PCs without them. See tools/check_imports.py.")
        return 1
    print("All imported DLLs are Windows system DLLs.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
