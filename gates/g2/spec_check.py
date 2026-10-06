#!/usr/bin/env python3
"""G2 spec check: compare the contractspecv0 custom section of two or more Wasm files.

Usage: spec_check.py BASELINE.wasm CANDIDATE.wasm [MORE.wasm ...]
Exits 0 only if every file's contractspecv0 section is byte-identical to the first.
"""
import hashlib
import sys


def leb128(b, i):
    result = shift = 0
    while True:
        byte = b[i]
        i += 1
        result |= (byte & 0x7F) << shift
        shift += 7
        if byte < 0x80:
            return result, i


def custom_sections(path):
    b = open(path, "rb").read()
    assert b[:4] == b"\0asm", f"{path}: not a Wasm module"
    i, out = 8, {}
    while i < len(b):
        section_id = b[i]
        size, i = leb128(b, i + 1)
        end = i + size
        if section_id == 0:
            name_len, j = leb128(b, i)
            out[b[j : j + name_len].decode()] = b[j + name_len : end]
        i = end
    return b, out


def main(paths):
    rows = []
    for p in paths:
        wasm, sections = custom_sections(p)
        spec = sections.get("contractspecv0", b"")
        rows.append((p, hashlib.sha256(wasm).hexdigest(), hashlib.sha256(spec).hexdigest(), len(spec)))
    base_spec = rows[0][2]
    identical = True
    for p, wasm_hash, spec_hash, spec_len in rows:
        same = spec_hash == base_spec
        identical &= same
        print(f"{'same' if same else 'DIFF'}  spec_sha256={spec_hash}  spec_bytes={spec_len}  wasm_sha256={wasm_hash}  {p}")
    print("RESULT:", "spec byte-identical" if identical else "spec differs")
    return 0 if identical else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
