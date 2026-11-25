#!/usr/bin/env python3
"""Independent reference for the sealed quarantine format of `sul-quarantine` (decision D-155).

Nothing here uses the Rust code or RustCrypto. The format is written out from the description in
`crates/sul-quarantine/src/seal.rs` and the XChaCha20 construction in draft-irtf-cfrg-xchacha:

* HChaCha20 is written below from the draft (and checked against the draft's own test vector before anything else runs);
* the AEAD is the `cryptography` package's ChaCha20-Poly1305 (OpenSSL), used with the HChaCha20 subkey and a 12-byte nonce of
  four zero bytes followed by the last eight bytes of the 24-byte nonce;
* the chunking is STREAM-BE32: chunk n is sealed under nonce `prefix(19) || n as u32 big-endian || last (1 byte)`, the 32
  header bytes are the associated data of every chunk, and the final chunk (possibly empty) carries last = 1.

Run: python3 -I seal_reference.py        (needs `pip install cryptography`)
It prints the SHA-256 of the sealed output for each pinned case; the Rust test `the_format_is_pinned_...` holds the same values.
"""

import hashlib
import struct

from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305

M = 0xFFFFFFFF


def rotl(x, n):
    return ((x << n) & M) | (x >> (32 - n))


def quarter(s, a, b, c, d):
    s[a] = (s[a] + s[b]) & M
    s[d] = rotl(s[d] ^ s[a], 16)
    s[c] = (s[c] + s[d]) & M
    s[b] = rotl(s[b] ^ s[c], 12)
    s[a] = (s[a] + s[b]) & M
    s[d] = rotl(s[d] ^ s[a], 8)
    s[c] = (s[c] + s[d]) & M
    s[b] = rotl(s[b] ^ s[c], 7)


def hchacha20(key, nonce16):
    assert len(key) == 32 and len(nonce16) == 16
    s = (
        list(struct.unpack("<4I", b"expand 32-byte k"))
        + list(struct.unpack("<8I", key))
        + list(struct.unpack("<4I", nonce16))
    )
    for _ in range(10):
        quarter(s, 0, 4, 8, 12)
        quarter(s, 1, 5, 9, 13)
        quarter(s, 2, 6, 10, 14)
        quarter(s, 3, 7, 11, 15)
        quarter(s, 0, 5, 10, 15)
        quarter(s, 1, 6, 11, 12)
        quarter(s, 2, 7, 8, 13)
        quarter(s, 3, 4, 9, 14)
    return struct.pack("<8I", *(s[0:4] + s[12:16]))


def check_hchacha20_against_the_draft():
    # draft-irtf-cfrg-xchacha, section 2.2.1.
    key = bytes(range(32))
    nonce = bytes.fromhex("000000090000004a0000000031415927")
    want = bytes.fromhex(
        "82413b4227b27bfed30e42508a877d73a0f9e4d58a74a853c12ec41326d3ecdc"
    )
    assert hchacha20(key, nonce) == want, "HChaCha20 does not match the draft's test vector"


def seal(key, plaintext, prefix, chunk_log2):
    assert len(key) == 32 and len(prefix) == 19
    chunk = 1 << chunk_log2
    header = b"SULQ" + bytes([1, 1, chunk_log2, 0]) + prefix + bytes(5)
    assert len(header) == 32
    out = bytearray(header)
    count = max(1, -(-len(plaintext) // chunk))
    for i in range(count):
        piece = plaintext[i * chunk : (i + 1) * chunk]
        last = 1 if i == count - 1 else 0
        nonce24 = prefix + struct.pack(">I", i) + bytes([last])
        subkey = hchacha20(key, nonce24[:16])
        out += ChaCha20Poly1305(subkey).encrypt(b"\0\0\0\0" + nonce24[16:], piece, header)
    return bytes(out)


def plain(n):
    return bytes(i % 251 for i in range(n))


if __name__ == "__main__":
    check_hchacha20_against_the_draft()
    print("HChaCha20 matches the draft's test vector")
    key = bytes([0x42]) * 32
    prefix = bytes(range(0xA0, 0xA0 + 19))
    for n in (0, 4096, 4097, 10000):
        sealed = seal(key, plain(n), prefix, 12)
        print(f"{n:>6} bytes -> {len(sealed):>6} sealed, sha256 {hashlib.sha256(sealed).hexdigest()}")
