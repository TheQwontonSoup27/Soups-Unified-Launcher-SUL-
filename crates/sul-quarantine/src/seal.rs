//! The sealed format of a quarantined copy: XChaCha20-Poly1305 in the STREAM construction (D-155).
//!
//! This module turns bytes into bytes and nothing else. It never opens, creates or deletes a file, so the places where SUL
//! touches the disk stay the ones `crates/sul-core/tests/write_sites.rs` lists.
//!
//! The cipher is the vault's (Ch. 2 §2.3: XChaCha20-Poly1305, with a STREAM final-chunk flag against truncation). The format
//! is this crate's own: it is the `aead::stream` STREAM-BE32 construction, which counts the chunks in the nonce, and it is
//! **not** the vault's file layout (Ch. 2 §2.3 derives nonces by XORing a base nonce with the chunk index, and names no chunk
//! size). Nothing depends on the two matching. What lets the vault take custody of the keys is the key-source seam
//! ([`crate::keys::KeySource`]), which hands back 32 bytes whatever sits behind it; the sealed copy does not change.
//!
//! # Format, version 1
//!
//! ```text
//! offset  size  field
//!      0     4  magic: the bytes "SULQ"
//!      4     1  format version: 1
//!      5     1  cipher: 1 = XChaCha20-Poly1305, STREAM-BE32
//!      6     1  chunk size as a power of two: every chunk but the last holds 1 << this many plaintext bytes (16 = 64 KiB)
//!      7     1  reserved, zero
//!      8    19  nonce prefix, random and new for every sealed copy
//!     27     5  reserved, zero
//!     32     …  the chunks, each its ciphertext followed by a 16-byte Poly1305 tag
//! ```
//!
//! Chunk `n` (counting from zero) is sealed under the 24-byte nonce `prefix || n as u32, big-endian || last`, where `last` is
//! 1 for the final chunk and 0 for every other. The 32 header bytes are the associated data of every chunk. The final chunk
//! is the one after which the stream ends; it may be empty (an empty file is one empty chunk, a tag and nothing else).
//!
//! What that gives: a changed byte anywhere, a changed header, a reordered, repeated, removed or appended chunk, and a
//! stream cut short at a chunk boundary all fail to open. What it does not give: it does not hide the size of the file (the
//! length of the sealed copy shows it to within 16 bytes per chunk), and it does not keep a secret from anyone who can read
//! the key, which for the default key source is the folder it sits in (see [`crate::keys`]).

use chacha20poly1305::XChaCha20Poly1305;
use chacha20poly1305::aead::generic_array::GenericArray;
use chacha20poly1305::aead::rand_core::RngCore;
use chacha20poly1305::aead::stream::{DecryptorBE32, EncryptorBE32};
use chacha20poly1305::aead::{KeyInit, OsRng};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::Path;
use sul_core::Error;
use zeroize::Zeroizing;

/// Bytes in a key.
pub const KEY_BYTES: usize = 32;
const MAGIC: [u8; 4] = *b"SULQ";
const FORMAT_VERSION: u8 = 1;
const CIPHER_ID: u8 = 1;
/// STREAM-BE32 over a 24-byte XChaCha nonce leaves 24 - 5 = 19 bytes for the prefix (4 for the counter, 1 for the last flag).
const PREFIX_BYTES: usize = 19;
pub(crate) const HEADER_BYTES: usize = 32;
const TAG_BYTES: usize = 16;
/// What this version writes: 64 KiB chunks. Chosen here for throughput (one buffer and one tag per 64 KiB instead of per
/// 4 KiB); the spec gives no chunk size for the vault, so this is not copied from it.
pub(crate) const CHUNK_LOG2: u8 = 16;
/// What this version agrees to read: 4 KiB to 1 MiB, so a damaged header cannot ask for an enormous buffer.
const MIN_CHUNK_LOG2: u8 = 12;
const MAX_CHUNK_LOG2: u8 = 20;

/// The key to one sealed copy. Each quarantined file has its own, made fresh from the operating system's random source.
///
/// It is wiped from memory when dropped, cannot be cloned, and never prints itself. It is not locked in memory, and the
/// cipher keeps its own copy for as long as one operation runs (the cipher wipes that copy too, when it is dropped).
pub struct ItemKey(Zeroizing<[u8; KEY_BYTES]>);

impl ItemKey {
    /// A new random key, or an error if the operating system's random source does not answer.
    pub fn generate() -> Result<Self, SealError> {
        let mut bytes = Zeroizing::new([0u8; KEY_BYTES]);
        OsRng
            .try_fill_bytes(&mut bytes[..])
            .map_err(|_| SealError::NoRandom)?;
        Ok(Self(bytes))
    }

    /// A key from exactly [`KEY_BYTES`] bytes (what a key source read back), or `None` if the length is wrong.
    pub fn from_slice(bytes: &[u8]) -> Option<Self> {
        let array: [u8; KEY_BYTES] = bytes.try_into().ok()?;
        Some(Self(Zeroizing::new(array)))
    }

    /// The key bytes, for a key source that has to store them. Handle with the care the name asks for.
    pub fn as_bytes(&self) -> &[u8; KEY_BYTES] {
        &self.0
    }
}

impl std::fmt::Debug for ItemKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ItemKey(..)")
    }
}

/// Why sealing or opening failed.
#[derive(Debug)]
pub enum SealError {
    /// Reading the bytes being sealed or opened, or writing the sealed bytes, failed.
    Io(std::io::Error),
    /// The `sink` that was handed opened plaintext failed (for example the disk it was writing to is full).
    Sink(std::io::Error),
    /// The operating system's random source did not answer, so no key or nonce could be made.
    NoRandom,
    /// The bytes are not a sealed copy at all (no header, or the wrong magic).
    NotSealed,
    /// A sealed copy, but of a version, cipher, chunk size or layout this version does not read.
    Unsupported(&'static str),
    /// A sealed copy that does not open: changed, cut short, extended, or sealed under another key.
    Unauthentic,
}

impl std::fmt::Display for SealError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) | Self::Sink(e) => write!(f, "{e}"),
            Self::NoRandom => f.write_str("the system's random source did not answer"),
            Self::NotSealed => f.write_str("this is not a sealed quarantine copy"),
            Self::Unsupported(what) => {
                write!(f, "a sealed copy with a {what} this version does not read")
            }
            Self::Unauthentic => f.write_str(
                "the sealed copy was changed, cut short or extended, or its key is not the one it was sealed with",
            ),
        }
    }
}

impl std::error::Error for SealError {}

impl From<std::io::Error> for SealError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl SealError {
    /// The error to report for a failure while working with `path`.
    pub(crate) fn at(self, path: &Path) -> Error {
        match self {
            Self::Io(source) | Self::Sink(source) => Error::Io {
                path: path.to_path_buf(),
                source,
            },
            other => Error::Invalid(other.to_string()),
        }
    }
}

fn header(prefix: &[u8; PREFIX_BYTES], chunk_log2: u8) -> [u8; HEADER_BYTES] {
    let mut h = [0u8; HEADER_BYTES];
    h[..4].copy_from_slice(&MAGIC);
    h[4] = FORMAT_VERSION;
    h[5] = CIPHER_ID;
    h[6] = chunk_log2;
    h[8..8 + PREFIX_BYTES].copy_from_slice(prefix);
    h
}

fn parse_header(h: &[u8; HEADER_BYTES]) -> Result<(u8, [u8; PREFIX_BYTES]), SealError> {
    if h[..4] != MAGIC {
        return Err(SealError::NotSealed);
    }
    if h[4] != FORMAT_VERSION {
        return Err(SealError::Unsupported("format version"));
    }
    if h[5] != CIPHER_ID {
        return Err(SealError::Unsupported("cipher"));
    }
    let chunk_log2 = h[6];
    if !(MIN_CHUNK_LOG2..=MAX_CHUNK_LOG2).contains(&chunk_log2) {
        return Err(SealError::Unsupported("chunk size"));
    }
    if h[7] != 0 || h[8 + PREFIX_BYTES..].iter().any(|b| *b != 0) {
        return Err(SealError::Unsupported("layout"));
    }
    let mut prefix = [0u8; PREFIX_BYTES];
    prefix.copy_from_slice(&h[8..8 + PREFIX_BYTES]);
    Ok((chunk_log2, prefix))
}

fn cipher(key: &ItemKey) -> XChaCha20Poly1305 {
    XChaCha20Poly1305::new(GenericArray::from_slice(key.as_bytes()))
}

/// Read up to `n` bytes into `buf` (replacing what it held), stopping early only at the end of the stream.
fn fill<R: Read>(src: &mut R, buf: &mut Vec<u8>, n: usize) -> std::io::Result<()> {
    buf.clear();
    src.by_ref().take(n as u64).read_to_end(buf)?;
    Ok(())
}

/// Seal everything `src` yields into `dst`, under a fresh random nonce prefix, in 64 KiB chunks.
pub(crate) fn seal<R: Read, W: Write>(key: &ItemKey, src: R, dst: W) -> Result<(), SealError> {
    let mut prefix = [0u8; PREFIX_BYTES];
    OsRng
        .try_fill_bytes(&mut prefix)
        .map_err(|_| SealError::NoRandom)?;
    seal_with(key, &prefix, CHUNK_LOG2, src, dst)
}

/// [`seal`] with the nonce prefix and chunk size chosen by the caller. Production code goes through [`seal`]; this exists so
/// a test can make the output repeatable and use small chunks to reach every edge of the chunking.
pub(crate) fn seal_with<R: Read, W: Write>(
    key: &ItemKey,
    prefix: &[u8; PREFIX_BYTES],
    chunk_log2: u8,
    mut src: R,
    mut dst: W,
) -> Result<(), SealError> {
    let head = header(prefix, chunk_log2);
    dst.write_all(&head)?;
    let chunk = 1usize << chunk_log2;
    let mut enc = EncryptorBE32::from_aead(cipher(key), GenericArray::from_slice(prefix));
    // One chunk is held back, so the final one is known to be final before it is sealed. Both buffers are reused, and the
    // tag is added in place, so no chunk costs an allocation.
    let mut cur: Vec<u8> = Vec::with_capacity(chunk + TAG_BYTES);
    let mut next: Vec<u8> = Vec::with_capacity(chunk + TAG_BYTES);
    fill(&mut src, &mut cur, chunk)?;
    loop {
        fill(&mut src, &mut next, chunk)?;
        if next.is_empty() {
            enc.encrypt_last_in_place(&head, &mut cur)
                .map_err(|_| SealError::Unauthentic)?;
            dst.write_all(&cur)?;
            return Ok(dst.flush()?);
        }
        enc.encrypt_next_in_place(&head, &mut cur)
            .map_err(|_| SealError::Unauthentic)?;
        dst.write_all(&cur)?;
        std::mem::swap(&mut cur, &mut next);
    }
}

/// Open a sealed copy, handing each chunk of plaintext to `sink` only after that chunk has authenticated. Nothing reaches
/// `sink` from a chunk that fails, but earlier chunks have already been handed over by then, so a caller that writes them
/// somewhere must be ready to take them back.
pub(crate) fn open<R: Read>(
    key: &ItemKey,
    mut src: R,
    mut sink: impl FnMut(&[u8]) -> std::io::Result<()>,
) -> Result<(), SealError> {
    let mut head = [0u8; HEADER_BYTES];
    if let Err(e) = src.read_exact(&mut head) {
        return Err(if e.kind() == std::io::ErrorKind::UnexpectedEof {
            SealError::NotSealed
        } else {
            e.into()
        });
    }
    let (chunk_log2, prefix) = parse_header(&head)?;
    let sealed_chunk = (1usize << chunk_log2) + TAG_BYTES;
    let mut dec = DecryptorBE32::from_aead(cipher(key), GenericArray::from_slice(&prefix));
    let mut cur: Vec<u8> = Vec::with_capacity(sealed_chunk);
    let mut next: Vec<u8> = Vec::with_capacity(sealed_chunk);
    fill(&mut src, &mut cur, sealed_chunk)?;
    loop {
        fill(&mut src, &mut next, sealed_chunk)?;
        if next.is_empty() {
            dec.decrypt_last_in_place(&head, &mut cur)
                .map_err(|_| SealError::Unauthentic)?;
            return sink(&cur).map_err(SealError::Sink);
        }
        dec.decrypt_next_in_place(&head, &mut cur)
            .map_err(|_| SealError::Unauthentic)?;
        sink(&cur).map_err(SealError::Sink)?;
        std::mem::swap(&mut cur, &mut next);
    }
}

/// The SHA-256 (as lowercase hex, the form `sul_trust::manifest::hash_file` gives) and size of what a sealed copy holds,
/// found by opening it and hashing the plaintext as it comes out. Nothing is written anywhere.
pub(crate) fn open_digest<R: Read>(key: &ItemKey, src: R) -> Result<(String, u64), SealError> {
    let mut hasher = Sha256::new();
    let mut total = 0u64;
    open(key, src, |plain| {
        hasher.update(plain);
        total += plain.len() as u64;
        Ok(())
    })?;
    Ok((hex::encode(hasher.finalize()), total))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(byte: u8) -> ItemKey {
        ItemKey::from_slice(&[byte; KEY_BYTES]).unwrap()
    }

    const PREFIX: [u8; PREFIX_BYTES] = [
        0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xAB, 0xAC, 0xAD, 0xAE,
        0xAF, 0xB0, 0xB1, 0xB2,
    ];

    /// Small chunks (4 KiB) so every edge of the chunking is reached with small inputs.
    const SMALL: u8 = 12;

    fn plain(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    fn sealed(k: &ItemKey, data: &[u8], log2: u8) -> Vec<u8> {
        let mut out = Vec::new();
        seal_with(k, &PREFIX, log2, data, &mut out).unwrap();
        out
    }

    fn opened(k: &ItemKey, bytes: &[u8]) -> Result<Vec<u8>, SealError> {
        let mut out = Vec::new();
        open(k, bytes, |p| {
            out.extend_from_slice(p);
            Ok(())
        })?;
        Ok(out)
    }

    #[test]
    fn every_edge_of_the_chunking_round_trips() {
        let k = key(7);
        let c = 1usize << SMALL;
        for len in [
            0,
            1,
            c - 1,
            c,
            c + 1,
            2 * c - 1,
            2 * c,
            2 * c + 1,
            10 * c,
            10_000,
        ] {
            let data = plain(len);
            let s = sealed(&k, &data, SMALL);
            let chunks = len.div_ceil(c).max(1);
            assert_eq!(
                s.len(),
                HEADER_BYTES + len + chunks * TAG_BYTES,
                "length for {len}"
            );
            assert_eq!(opened(&k, &s).unwrap(), data, "round trip for {len}");
        }
    }

    #[test]
    fn the_production_path_uses_64_kib_chunks_and_a_fresh_prefix_each_time() {
        let k = key(1);
        let data = plain(200_000);
        let (mut a, mut b) = (Vec::new(), Vec::new());
        seal(&k, &data[..], &mut a).unwrap();
        seal(&k, &data[..], &mut b).unwrap();
        assert_eq!(a[6], 16, "64 KiB chunks");
        assert_ne!(a[8..27], b[8..27], "a new nonce prefix for every copy");
        assert_ne!(
            a[32..],
            b[32..],
            "so the same file never seals the same way"
        );
        assert_eq!(a.len(), 32 + 200_000 + 4 * 16, "four chunks, four tags");
        assert_eq!(opened(&k, &a).unwrap(), data);
        assert_eq!(opened(&k, &b).unwrap(), data);
    }

    #[test]
    fn the_plaintext_is_not_in_the_sealed_bytes() {
        let k = key(2);
        let data = b"MZ this is the secret-free but recognisable program text".repeat(50);
        let s = sealed(&k, &data, SMALL);
        assert!(
            !s.windows(16).any(|w| w == &data[..16]),
            "a run of the plaintext survived"
        );
    }

    #[test]
    fn the_wrong_key_does_not_open_it() {
        let s = sealed(&key(1), &plain(5000), SMALL);
        assert!(matches!(opened(&key(2), &s), Err(SealError::Unauthentic)));
    }

    #[test]
    fn a_changed_byte_anywhere_does_not_open() {
        let k = key(3);
        let s = sealed(&k, &plain(9000), SMALL);
        assert!(opened(&k, &s).is_ok());
        for at in 0..s.len() {
            let mut bad = s.clone();
            bad[at] ^= 0x01;
            assert!(
                opened(&k, &bad).is_err(),
                "a change at byte {at} was not noticed"
            );
        }
    }

    #[test]
    fn a_copy_cut_short_or_extended_does_not_open() {
        let k = key(4);
        let c = 1usize << SMALL;
        for len in [0, 1, c, 2 * c, 2 * c + 5] {
            let s = sealed(&k, &plain(len), SMALL);
            // Cut at every length, including exact chunk boundaries (the case a naive format would accept).
            for cut in 0..s.len() {
                assert!(
                    opened(&k, &s[..cut]).is_err(),
                    "{len}-byte file cut to {cut} bytes opened"
                );
            }
            let mut longer = s.clone();
            longer.push(0);
            assert!(opened(&k, &longer).is_err(), "an extra byte was accepted");
            let mut doubled = s.clone();
            doubled.extend_from_slice(&s[HEADER_BYTES..]);
            assert!(
                opened(&k, &doubled).is_err(),
                "a repeated chunk was accepted"
            );
        }
    }

    #[test]
    fn swapped_chunks_do_not_open() {
        let k = key(5);
        let c = 1usize << SMALL;
        let s = sealed(&k, &plain(3 * c), SMALL);
        let size = c + TAG_BYTES;
        let mut swapped = s.clone();
        swapped[HEADER_BYTES..HEADER_BYTES + size]
            .copy_from_slice(&s[HEADER_BYTES + size..HEADER_BYTES + 2 * size]);
        swapped[HEADER_BYTES + size..HEADER_BYTES + 2 * size]
            .copy_from_slice(&s[HEADER_BYTES..HEADER_BYTES + size]);
        assert!(opened(&k, &swapped).is_err());
    }

    #[test]
    fn a_header_that_is_not_ours_is_told_apart_from_a_damaged_copy() {
        let k = key(6);
        assert!(matches!(opened(&k, b""), Err(SealError::NotSealed)));
        assert!(matches!(opened(&k, b"SULQ"), Err(SealError::NotSealed)));
        assert!(matches!(
            opened(&k, &[0x41; 100]),
            Err(SealError::NotSealed)
        ));
        let s = sealed(&k, &plain(10), SMALL);
        for (at, value, what) in [
            (4usize, 2u8, "format version"),
            (5, 2, "cipher"),
            (6, 11, "chunk size"),
            (6, 21, "chunk size"),
            (7, 1, "layout"),
            (31, 1, "layout"),
        ] {
            let mut bad = s.clone();
            bad[at] = value;
            assert!(
                matches!(opened(&k, &bad), Err(SealError::Unsupported(w)) if w == what),
                "byte {at} = {value}"
            );
        }
    }

    #[test]
    fn nothing_reaches_the_sink_from_a_chunk_that_fails() {
        let k = key(8);
        let c = 1usize << SMALL;
        let mut s = sealed(&k, &plain(3 * c), SMALL);
        let last_chunk_start = HEADER_BYTES + 2 * (c + TAG_BYTES);
        s[last_chunk_start + 3] ^= 0xFF;
        let mut got = 0usize;
        let r = open(&k, &s[..], |p| {
            got += p.len();
            Ok(())
        });
        assert!(r.is_err());
        assert_eq!(
            got,
            2 * c,
            "the two good chunks were delivered, the bad one was not"
        );
    }

    #[test]
    fn a_sink_that_fails_stops_the_opening_with_its_error() {
        let k = key(9);
        let s = sealed(&k, &plain(100), SMALL);
        let r = open(&k, &s[..], |_| Err(std::io::Error::other("disk full")));
        assert!(matches!(r, Err(SealError::Sink(e)) if e.to_string() == "disk full"));
    }

    #[test]
    fn the_digest_is_the_sha256_of_the_plaintext_and_its_size() {
        let k = key(10);
        let data = plain(10_000);
        let s = sealed(&k, &data, SMALL);
        let (hash, size) = open_digest(&k, &s[..]).unwrap();
        assert_eq!(size, 10_000);
        assert_eq!(hash, hex::encode(Sha256::digest(&data)));
        // The same hex form `sul_trust::manifest::hash_file` gives, so the two can be compared.
        let (empty_hash, empty_size) = open_digest(&k, &sealed(&k, b"", SMALL)[..]).unwrap();
        assert_eq!(empty_size, 0);
        assert_eq!(
            empty_hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn a_key_wipes_itself_and_never_prints() {
        let k = key(0x5A);
        assert_eq!(format!("{k:?}"), "ItemKey(..)");
        assert!(ItemKey::from_slice(&[0u8; 31]).is_none());
        assert!(ItemKey::from_slice(&[0u8; 33]).is_none());
        let (a, b) = (ItemKey::generate().unwrap(), ItemKey::generate().unwrap());
        assert_ne!(a.as_bytes(), b.as_bytes(), "keys are random");
        assert_ne!(a.as_bytes(), &[0u8; KEY_BYTES]);
    }

    /// Pins the format. The header is asserted byte for byte and each whole sealed output by its SHA-256. The same bytes come
    /// from an independent implementation, `tests/golden/seal_reference.py`: HChaCha20 written from the XChaCha20 draft (and
    /// checked against the draft's own test vector), the `cryptography` package's ChaCha20-Poly1305 for the AEAD, and the
    /// STREAM-BE32 chunking written from the format description above. So the pinned values are the documented format, not
    /// just whatever this code did. The four sizes are the empty file, a file of exactly one chunk, one byte over a chunk, and
    /// three chunks with a short last one: the places where the "last chunk" flag is easiest to get wrong.
    #[test]
    fn the_format_is_pinned_and_matches_an_independent_implementation() {
        let k = key(0x42);
        for (len, sealed_len, golden) in GOLDEN {
            let s = sealed(&k, &plain(len), SMALL);
            assert_eq!(
                hex::encode(&s[..HEADER_BYTES]),
                "53554c51 0101 0c00 a0a1a2a3a4a5a6a7a8a9aaabacadaeafb0b1b2 0000000000"
                    .replace(' ', ""),
                "header for {len}"
            );
            assert_eq!(s.len(), sealed_len, "sealed length for {len}");
            assert_eq!(
                hex::encode(Sha256::digest(&s)),
                golden,
                "sealed bytes for {len}"
            );
        }
    }

    /// `(plaintext bytes, sealed bytes, SHA-256 of the sealed bytes)`, key `[0x42; 32]`, nonce prefix `a0..b2`, 4 KiB chunks,
    /// plaintext byte `i` = `i % 251`. Produced by `tests/golden/seal_reference.py`, not by this crate.
    const GOLDEN: [(usize, usize, &str); 4] = [
        (
            0,
            48,
            "0fa3931eb2187edbf71114824feea0146430017734b0206faa013bad29b88139",
        ),
        (
            4096,
            4144,
            "c009ed6b63f9cdc8f14155a283f6b1e2e4911711c478be70009701fcf58959fe",
        ),
        (
            4097,
            4161,
            "2837943dca8f751e30af7aac86413d712d59bd915024358c6490175fde41c8d3",
        ),
        (
            10_000,
            10_080,
            "bda27c7961b82ac5230d2514e3530e5cafd46617c9e97ccb68897b5e68dd2509",
        ),
    ];
}
