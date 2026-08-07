//! One memory mapping of a `.dsb` file — the native half of R5's "mmap +
//! section discipline" (story #595, epic #594).
//!
//! `docs/decisions/dsb-sectioned-container.md` specifies the loading model as
//! "one `mmap` of the whole file, once. The envelope is read through the
//! mapping (page 0 faults — it is the hottest data in the file)", and
//! [`crate::container`] was written for it: [`crate::container::Container::parse`]
//! takes a `&[u8]` and hands out borrowed slices into it, blobs align to a
//! 64-byte quantum, and large ones are page-aligned. So a mapping is a drop-in
//! — [`MappedFile`] derefs to `&[u8]` and [`crate::open`] takes it unchanged.
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
//! [`crate::residency::Residency::touch`] faults in the ones a frame actually
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

use memmap2::Mmap;

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
/// [`crate::residency::Residency`] is `Send + Sync` for the same reason, and a
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
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        if len == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{} is empty, and a .dsb begins with a {}-byte header",
                    path.display(),
                    crate::container::HEADER_SIZE
                ),
            ));
        }
        // SAFETY: see "Safety of the mapping" above. The mapping is read-only,
        // nothing in this process writes the file, and the hazard that remains
        // — another process changing the file underneath a live mapping — is
        // inherent to mapping and is not removable by any check here.
        let map = unsafe { Mmap::map(&file)? };
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
