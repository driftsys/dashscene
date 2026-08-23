//! One memory mapping of a `.dsb` file — the native half of R5's "mmap +
//! section discipline" (story #595, epic #594).
//!
//! `docs/decisions/dsb-sectioned-container.md` specifies the loading model as
//! "one `mmap` of the whole file, once. The envelope is read through the
//! mapping (page 0 faults — it is the hottest data in the file)", and
//! [`crate::container`] was written for it: [`crate::container::Container::parse`]
//! takes a `&[u8]` and hands out borrowed slices into it, blobs align to a
//! 64-byte quantum, and large ones are page-aligned. So a mapping is a drop-in
//! — [`crate::map::MappedFile`] derefs to `&[u8]` and [`crate::open`] takes it unchanged.
//!
//! # What mapping does and does not buy, today
//!
//! It removes the read: the file's pages are not copied into a `Vec`, and a
//! page never touched is never faulted in.
//!
//! It did **not** by itself make cold start proportional to what is shown. At
//! story #595 the reader this hands bytes to resolved every asset entry through
//! [`crate::container::Container::blob_by_hash`], which hash-verifies the whole
//! payload, so the first open faulted every page holding one in anyway; the
//! criterion did not move. Story #597 is what made mapping pay: [`crate::open`]
//! resolves an entry to where its payload lies and reads none of them, and
//! [`crate::residency::BlobResidency::touch`] faults in the ones a frame actually
//! draws. Mapping is the half that makes the other half possible, and neither
//! half moves a number alone.
//!
//! # Native only
//!
//! wasm has no `mmap`, which is why [`crate::prefix`] exists: a browser host
//! fetches a prefix, reads the envelope from it, and fetches the rest by byte
//! range (`docs/decisions/container-parse-reads-a-prefix-through-a-host-reader.md`).
//! This module is compiled out there, and `dashbuf`'s `memmap2` dependency is
//! target-gated so a wasm build links no part of it.

use std::fs::File;
use std::io;
use std::ops::Deref;
use std::path::Path;

use memmap2::{Mmap, MmapOptions};

/// A `.dsb` file, mapped read-only into this process's address space.
///
/// Derefs to the file's bytes, so it goes straight into [`crate::open`] or
/// [`crate::container::Container::parse`]. It owns the mapping and unmaps on
/// drop; every slice either of those hands back borrows from it, so the
/// mapping must outlive the document read out of it — which the borrow checker
/// enforces, since those slices carry this value's lifetime.
///
/// `Send` and `Sync`, both inherited from [`Mmap`]. Story #596 put the region
/// behind a reference-counted handle that `dashpaint`'s image table holds
/// (`docs/decisions/assets-borrow-from-the-mapping.md` D4), so the property is
/// load-bearing rather than incidental —
/// [`crate::residency::BlobResidency`] is `Send + Sync` for the same reason, and a
/// loader thread is the next step rather than a different design.
#[derive(Debug)]
pub struct MappedFile {
    map: Mmap,
}

impl MappedFile {
    /// Maps `path` read-only, whole.
    ///
    /// # Errors
    ///
    /// The file cannot be opened, is empty, or cannot be mapped. An empty file
    /// is refused here rather than passed to the operating system, which
    /// rejects a zero-length mapping with a bare "invalid argument" that names
    /// neither the length nor the path. It is not a `.dsb` either way — the
    /// header alone is [`crate::container::HEADER_SIZE`] bytes, and the
    /// envelope is that plus a 64-byte entry per section — but
    /// [`crate::container::Container::parse`] is the one that says so, and it
    /// needs bytes to say it about.
    ///
    /// # Safety of the mapping
    ///
    /// [`Mmap::map`] is `unsafe` for a reason this call cannot remove: the
    /// mapping aliases a file that another process may write or truncate while
    /// it is live. A concurrent write makes the bytes change under a reader
    /// that has already checked them, and a truncation turns an already-mapped
    /// page into `SIGBUS` on touch. Nothing in this process writes the file,
    /// and a `.dsb` is a content-addressed artifact that is written once and
    /// read afterwards, so the hazard is another process editing an asset the
    /// host is drawing — a case the format's own section hashes catch at open
    /// and cannot catch afterwards. It is the standing caveat of mapping a file
    /// at all; stating it is the honest position, and no `unsafe` block here
    /// makes it false.
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        // Every error out of this function names the file, including the ones
        // the operating system produces — see [`Self::open_range`], which keeps
        // the same rule so that a caller can pass either through unwrapped.
        let named =
            |error: io::Error| io::Error::new(error.kind(), format!("{}: {error}", path.display()));
        let file = File::open(path).map_err(named)?;
        let len = file.metadata().map_err(named)?.len();
        if len == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{}: the file is empty, and a .dsb begins with a {}-byte header",
                    path.display(),
                    crate::container::HEADER_SIZE
                ),
            ));
        }
        // SAFETY: see "Safety of the mapping" above. The mapping is read-only,
        // nothing in this process writes the file, and the hazard that remains
        // — another process changing the file underneath a live mapping — is
        // inherent to mapping and is not removable by any check here.
        let map = unsafe { Mmap::map(&file).map_err(named)? };
        Ok(Self { map })
    }

    /// Maps the `length` bytes at `offset` inside `path`, read-only.
    ///
    /// [`Self::open`] is this call over the whole file, and the two exist
    /// separately because a `.dsb` does not always begin a file. A container
    /// format that stores entries uncompressed — an Android APK is the case
    /// story #1124 met — holds the document at an offset inside a larger file,
    /// and reading it out to a path of its own is the copy mapping exists to
    /// avoid.
    ///
    /// `offset` needs no alignment. [`MmapOptions::offset`] maps from the page
    /// below it and returns a slice starting at the byte asked for, so the
    /// caller passes the offset a container reports rather than one it has
    /// rounded.
    ///
    /// **What a misaligned offset costs is real and is not correctness.**
    /// `docs/design/dsb-container-format.md` requires page alignment in exactly
    /// one place — the hot/cold boundary, so
    /// [`crate::container::Container::verify_hot`] faults no cold page — and
    /// leaves every other alignment to writer policy, which a reader does not
    /// enforce. A document at an offset shifts the required boundary against
    /// the process's pages.
    ///
    /// **On most hosts it is already shifted**, which is what makes this a cost
    /// of degree rather than of kind: [`crate::container::PAGE_ALIGN`] is 4096
    /// and is a property of the file, while this machine's pages are 16 KiB and
    /// Android 15 requires 16 KB page support on new devices. So the guarantee
    /// is approximate at offset 0 there too. `madvise` is not called and nothing
    /// reads alignment, so the cost is extra pages faulted, not a wrong answer.
    /// `docs/decisions/the-document-is-mapped-where-it-is-packed.md` D4 carries
    /// the full argument.
    ///
    /// # Errors
    ///
    /// `length` is 0, the range ends past the end of the file, `offset +
    /// length` overflows, `length` exceeds this target's address space, or the
    /// file cannot be opened or mapped. A range naming bytes the file does not
    /// have is refused here rather than mapped: `mmap` past the end of a file
    /// succeeds and answers `SIGBUS` on touch, which arrives with nothing
    /// naming the range that caused it.
    ///
    /// # Safety of the mapping
    ///
    /// The same standing caveat [`Self::open`] states, and one addition that
    /// matters for a container: the range is checked against the file's length
    /// as it is at this moment, and another process truncating the file
    /// afterwards turns an already-mapped page into `SIGBUS` on touch. No check
    /// here removes that.
    pub fn open_range(path: impl AsRef<Path>, offset: u64, length: u64) -> io::Result<Self> {
        let path = path.as_ref();
        // **Every error out of this function names the file**, including the
        // ones the operating system produces — `File::open` answers a bare "No
        // such file or directory" that names nothing. That uniformity is what
        // lets `dashscene-ffi`'s mapped loader pass these through unwrapped,
        // where it prefixes [`Self::open`]'s. Without it the range arm reported
        // a failure naming no container.
        let named =
            |error: io::Error| io::Error::new(error.kind(), format!("{}: {error}", path.display()));
        let file = File::open(path).map_err(named)?;
        let file_len = file.metadata().map_err(named)?.len();

        if length == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{} at offset {offset}: a range of 0 bytes names no document, and a .dsb \
                     begins with a {}-byte header",
                    path.display(),
                    crate::container::HEADER_SIZE
                ),
            ));
        }

        let end = offset.checked_add(length).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{}: offset {offset} plus length {length} overflows",
                    path.display()
                ),
            )
        })?;
        if end > file_len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{}: bytes {offset}..{end} were asked for and the file is {file_len} bytes",
                    path.display()
                ),
            ));
        }

        let len = usize::try_from(length).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{}: a length of {length} bytes does not fit this target's address space",
                    path.display()
                ),
            )
        })?;

        // SAFETY: see "Safety of the mapping" above and on `open`. The mapping
        // is read-only, the range has been checked against the file's length,
        // and the hazard that remains — another process changing the file
        // underneath a live mapping — is inherent to mapping and is not
        // removable by any check here.
        let map = unsafe {
            MmapOptions::new()
                .offset(offset)
                .len(len)
                .map(&file)
                .map_err(named)?
        };
        Ok(Self { map })
    }

    /// The mapped bytes.
    ///
    /// The same slice [`Deref`] yields; named so that a caller reading a
    /// pointer or a length does not have to spell out a deref to get one.
    pub fn bytes(&self) -> &[u8] {
        &self.map
    }
}

impl Deref for MappedFile {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.map
    }
}

impl AsRef<[u8]> for MappedFile {
    fn as_ref(&self) -> &[u8] {
        &self.map
    }
}
