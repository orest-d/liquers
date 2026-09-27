//! 64-byte-aligned, `Arc`-shared byte and typed buffers.
//!
//! These are the storage `Column` and `Bitmap` are built from (see
//! `specs/design/record-streams/phase2-architecture.md`, §"Columns, not rows", §"Bitmap" and
//! §"64-byte alignment"). [COLUMNAR] *Buffer Alignment and Padding* requires 8-byte buffer
//! alignment and recommends 64, so a consumer can use aligned SIMD loads without a special case at
//! the tail, and so an export through the Arrow C Data Interface hands over a pointer rather than a
//! converted copy. Buffers are `Arc`-shared and immutable once built, so cloning one is cheap.

use std::marker::PhantomData;
use std::sync::Arc;

use liquers_core::error::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Bytes per aligned chunk, and the alignment [`AlignedBuffer`] guarantees.
const ALIGNMENT: usize = 64;

/// One 64-byte, 64-byte-aligned block.
///
/// `size_of::<AlignedChunk>() == align_of::<AlignedChunk>() == 64`, so a run of `AlignedChunk`s is
/// laid out contiguously with no padding between elements — a slice of them is byte-for-byte the
/// same as a flat byte buffer, just with a stronger alignment guarantee on its start. `Pod` and
/// `Zeroable` are derived (the `derive` feature of `bytemuck`) rather than hand-implemented, so
/// `liquers-records` never has an `unsafe` block of its own: the derive macro performs the
/// padding/layout checks that would otherwise have to be asserted by hand.
#[repr(C, align(64))]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
struct AlignedChunk([u8; ALIGNMENT]);

/// A byte buffer whose start is 64-byte aligned, as Arrow recommends.
///
/// Backed by [`AlignedChunk`] blocks; `bytemuck::cast_slice` reads them as plain bytes. The
/// logical length is tracked separately from the storage length, which is padded to a whole
/// number of 64-byte chunks — the padding, when there is any, is zeroed and excluded from
/// [`AlignedBuffer::as_bytes`].
///
/// `Arc`-shared and immutable once built through its public API: cloning an `AlignedBuffer` clones
/// a reference, not the bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct AlignedBuffer {
    /// `Arc<Vec<_>>` rather than `Arc<[_]>` (Step 2.2's original shape) so
    /// [`AlignedBuffer::into_mut`] can reach for `Arc::try_unwrap`, which needs a `Sized` payload
    /// and refuses an unsized `Arc<[T]>` — see `specs/design/record-streams/phase2-architecture.md`
    /// §"Writing a view" (`RecordBatch::into_mut`). Purely an internal storage choice: every public
    /// method's behavior is unchanged.
    chunks: Arc<Vec<AlignedChunk>>,
    len: usize,
}

impl AlignedBuffer {
    /// Copies `bytes` into a freshly allocated, 64-byte-aligned buffer.
    pub fn from_slice(bytes: &[u8]) -> Self {
        let chunk_count = bytes.len().div_ceil(ALIGNMENT);
        let mut chunks = vec![AlignedChunk([0u8; ALIGNMENT]); chunk_count];
        let flat: &mut [u8] = bytemuck::cast_slice_mut(&mut chunks);
        flat[..bytes.len()].copy_from_slice(bytes);
        AlignedBuffer {
            chunks: Arc::new(chunks),
            len: bytes.len(),
        }
    }

    /// The buffer's logical bytes: 64-byte aligned, and exactly as long as what `from_slice` was
    /// given — any chunk padding is excluded.
    pub fn as_bytes(&self) -> &[u8] {
        let flat: &[u8] = bytemuck::cast_slice(self.chunks.as_slice());
        &flat[..self.len]
    }

    /// The alignment `AlignedBuffer` guarantees: 64 bytes, as Arrow recommends. Asserted in a
    /// test.
    pub fn alignment() -> usize {
        ALIGNMENT
    }

    /// Mutable access to the buffer's logical bytes, copy-on-write when the backing storage is
    /// shared with another `AlignedBuffer`. Private: the public API is immutable, and this exists
    /// only so [`Bitmap::set`] can flip a single bit in place while it is still being built,
    /// without corrupting a clone that shares the same storage.
    fn make_bytes_mut(&mut self) -> &mut [u8] {
        if Arc::get_mut(&mut self.chunks).is_none() {
            self.chunks = Arc::new((*self.chunks).clone());
        }
        let len = self.len;
        match Arc::get_mut(&mut self.chunks) {
            Some(chunks) => {
                let flat: &mut [u8] = bytemuck::cast_slice_mut(chunks.as_mut_slice());
                let end = len.min(flat.len());
                &mut flat[..end]
            }
            None => &mut [],
        }
    }

    /// Takes over this buffer's storage as a growable [`AlignedBytesMut`] — a move, no byte copy,
    /// when this `AlignedBuffer` is the storage's only handle (`Arc::try_unwrap` succeeds); a
    /// single copy otherwise. `pub(crate)`: `mutable.rs`'s `RecordBatch::into_mut` is the only
    /// caller, and its `ColumnMut` is the public mutable surface.
    pub(crate) fn into_mut(self) -> AlignedBytesMut {
        let AlignedBuffer { chunks, len } = self;
        match Arc::try_unwrap(chunks) {
            Ok(chunks) => AlignedBytesMut { chunks, len },
            Err(shared) => {
                let flat: &[u8] = bytemuck::cast_slice(shared.as_slice());
                let mut mutable = AlignedBytesMut::with_capacity(len);
                mutable.extend_from_slice(&flat[..len]);
                mutable
            }
        }
    }
}

/// The growable counterpart of [`AlignedBuffer`]: append-only and chunk-backed from the start, so
/// [`AlignedBytesMut::freeze`] hands its storage to a new `AlignedBuffer` without copying — the
/// `BytesMut` → `Bytes` idiom phase2-architecture.md's §"Writing a view" describes for `ColumnMut`.
/// `pub(crate)`: the public mutable surface is `ColumnMut` (`mutable.rs`), which this backs.
#[derive(Debug, Clone)]
pub(crate) struct AlignedBytesMut {
    chunks: Vec<AlignedChunk>,
    len: usize,
}

impl AlignedBytesMut {
    pub(crate) fn new() -> Self {
        AlignedBytesMut { chunks: Vec::new(), len: 0 }
    }

    pub(crate) fn with_capacity(bytes: usize) -> Self {
        AlignedBytesMut {
            chunks: Vec::with_capacity(bytes.div_ceil(ALIGNMENT)),
            len: 0,
        }
    }

    /// Logical bytes written so far — not the padded chunk capacity.
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    pub(crate) fn reserve(&mut self, additional_bytes: usize) {
        self.chunks.reserve(additional_bytes.div_ceil(ALIGNMENT));
    }

    /// The bytes written so far, 64-byte aligned, padding excluded — mirrors
    /// [`AlignedBuffer::as_bytes`].
    pub(crate) fn as_bytes(&self) -> &[u8] {
        let flat: &[u8] = bytemuck::cast_slice(self.chunks.as_slice());
        &flat[..self.len]
    }

    /// Appends `bytes`, growing the chunk storage as needed. A newly added tail chunk is
    /// zero-initialized before writing, exactly as [`AlignedBuffer::from_slice`] does.
    pub(crate) fn extend_from_slice(&mut self, bytes: &[u8]) {
        let start = self.len;
        let needed_chunks = (start + bytes.len()).div_ceil(ALIGNMENT);
        if needed_chunks > self.chunks.len() {
            self.chunks.resize(needed_chunks, AlignedChunk([0u8; ALIGNMENT]));
        }
        let flat: &mut [u8] = bytemuck::cast_slice_mut(&mut self.chunks);
        flat[start..start + bytes.len()].copy_from_slice(bytes);
        self.len += bytes.len();
    }

    /// Overwrites `bytes.len()` already-written bytes starting at `offset` — how `ColumnMut::set`
    /// replaces one fixed-width cell in place.
    pub(crate) fn set(&mut self, offset: usize, bytes: &[u8]) {
        let flat: &mut [u8] = bytemuck::cast_slice_mut(&mut self.chunks);
        flat[offset..offset + bytes.len()].copy_from_slice(bytes);
    }

    /// Shortens the buffer to `len` bytes (no-op when it is not longer), zeroing the dropped
    /// bytes of the last kept chunk so the padding stays zero as `extend_from_slice` leaves it.
    /// How `ColumnMut` undoes a partially appended row.
    pub(crate) fn truncate(&mut self, len: usize) {
        if len >= self.len {
            return;
        }
        self.chunks.truncate(len.div_ceil(ALIGNMENT));
        let flat: &mut [u8] = bytemuck::cast_slice_mut(&mut self.chunks);
        flat[len..].fill(0);
        self.len = len;
    }

    /// Consumes this buffer into an immutable [`AlignedBuffer`] — the chunk storage becomes the
    /// new buffer's `Arc` directly, so this is a move, not a copy.
    pub(crate) fn freeze(self) -> AlignedBuffer {
        AlignedBuffer {
            chunks: Arc::new(self.chunks),
            len: self.len,
        }
    }
}

/// Serializes as its logical bytes, so it round-trips through any serde format — `from_slice`
/// rebuilds the 64-byte alignment on the way back in, rather than trying to preserve it in the
/// serialized form.
impl Serialize for AlignedBuffer {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(self.as_bytes())
    }
}

impl<'de> Deserialize<'de> for AlignedBuffer {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes = Vec::<u8>::deserialize(deserializer)?;
        Ok(AlignedBuffer::from_slice(&bytes))
    }
}

/// A typed, `Arc`-shared, 64-byte-aligned slice. A `bytemuck::Pod` view over an [`AlignedBuffer`].
///
/// `Debug`, `Clone` and `PartialEq` are implemented by hand rather than derived, so `Buffer<T>`
/// keeps exactly the bound Phase 2 declares (`T: bytemuck::Pod`) instead of the extra
/// `T: Debug + Clone + PartialEq` a derive would add for a `PhantomData<T>` field that carries no
/// actual `T` value.
pub struct Buffer<T: bytemuck::Pod> {
    data: AlignedBuffer,
    _marker: PhantomData<T>,
}

impl<T: bytemuck::Pod> Buffer<T> {
    /// Copies `values` into a freshly allocated, 64-byte-aligned buffer.
    pub fn from_slice(values: &[T]) -> Self {
        Buffer {
            data: AlignedBuffer::from_slice(bytemuck::cast_slice(values)),
            _marker: PhantomData,
        }
    }

    /// The buffer's values, as a typed slice.
    pub fn as_slice(&self) -> &[T] {
        bytemuck::cast_slice(self.data.as_bytes())
    }

    /// The buffer's raw bytes — 64-byte aligned, little-endian on every target this project
    /// builds for.
    pub fn as_bytes(&self) -> &[u8] {
        self.data.as_bytes()
    }

    /// Unwraps the typed view, handing back the untyped storage — how `mutable.rs` reaches
    /// [`AlignedBuffer::into_mut`] without `Buffer<T>` exposing its private field. `pub(crate)`:
    /// `ColumnMut`/`RecordBatch::into_mut` are the only callers.
    pub(crate) fn into_aligned(self) -> AlignedBuffer {
        self.data
    }

    /// The inverse of [`Buffer::into_aligned`] — wraps an already-built `AlignedBuffer` back into
    /// a typed `Buffer<T>`, e.g. the frozen result of [`AlignedBytesMut::freeze`].
    pub(crate) fn from_aligned(data: AlignedBuffer) -> Self {
        Buffer { data, _marker: PhantomData }
    }
}

impl<T: bytemuck::Pod> std::fmt::Debug for Buffer<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Buffer")
            .field("bytes", &self.data.as_bytes().len())
            .finish()
    }
}

impl<T: bytemuck::Pod> Clone for Buffer<T> {
    fn clone(&self) -> Self {
        Buffer {
            data: self.data.clone(),
            _marker: PhantomData,
        }
    }
}

impl<T: bytemuck::Pod> PartialEq for Buffer<T> {
    fn eq(&self, other: &Self) -> bool {
        self.data == other.data
    }
}

/// Serializes as a plain sequence of `T` values — `[1, 2, 3]` in JSON — rather than as raw
/// little-endian bytes. `Column`'s buffers hold typed numeric and offset data, and a reader of a
/// stored `RecordBatch` (a person debugging a manifest, an `NDJSON` export) benefits far more from
/// readable numbers than from a base64 blob whose endianness is a silent assumption. Lossless
/// either way — `from_slice` on the way back in rebuilds the 64-byte alignment, exactly as
/// [`AlignedBuffer`]'s own `Serialize` does — so this is a readability choice, not a correctness
/// one. Contrast [`AlignedBuffer`], which holds untyped bytes (`Column::Text`/`Binary`'s `data`)
/// and so has no typed values to serialize as.
impl<T: bytemuck::Pod + Serialize> Serialize for Buffer<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.as_slice().serialize(serializer)
    }
}

impl<'de, T: bytemuck::Pod + Deserialize<'de>> Deserialize<'de> for Buffer<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let values = Vec::<T>::deserialize(deserializer)?;
        Ok(Buffer::from_slice(&values))
    }
}

/// Bit-packed booleans, LSB-first within each byte, as Arrow specifies. Used for validity
/// (nulls), filter masks and `Column::Bool` storage — see phase2-architecture.md §"Bitmap".
///
/// Only the first `len` bits are part of the bitmap. Bits beyond `len` — the padding of the last
/// byte, or whole bytes a deserialized bitmap happens to carry — are ignored by every operation,
/// including equality (implemented by hand for that reason) and [`Bitmap::count_ones`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bitmap {
    bits: AlignedBuffer,
    len: usize,
}

impl PartialEq for Bitmap {
    fn eq(&self, other: &Self) -> bool {
        self.len == other.len
            && (0..self.len.div_ceil(8)).all(|index| self.masked_byte(index) == other.masked_byte(index))
    }
}

impl Bitmap {
    /// `len` bits, all clear.
    pub fn new(len: usize) -> Self {
        let byte_len = len.div_ceil(8);
        Bitmap {
            bits: AlignedBuffer::from_slice(&vec![0u8; byte_len]),
            len,
        }
    }

    /// The way a caller builds a mask without knowing the LSB-first packing.
    pub fn from_bools(values: &[bool]) -> Self {
        let mut bitmap = Bitmap::new(values.len());
        for (i, &value) in values.iter().enumerate() {
            if value {
                bitmap.set(i, true);
            }
        }
        bitmap
    }

    /// Panics on an out-of-range `i`, as slice indexing does. The only deliberate panic in this
    /// crate.
    pub fn set(&mut self, i: usize, value: bool) {
        assert!(
            i < self.len,
            "index out of bounds: the len is {} but the index is {}",
            self.len,
            i
        );
        let byte_idx = i / 8;
        let bit_idx = i % 8;
        let bytes = self.bits.make_bytes_mut();
        if value {
            bytes[byte_idx] |= 1 << bit_idx;
        } else {
            bytes[byte_idx] &= !(1 << bit_idx);
        }
    }

    /// Bits, not bytes.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the bitmap has no bits at all — distinct from every bit being clear.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// `false` for a bit the storage does not hold (a deserialized bitmap whose bytes are shorter
    /// than its `len`) rather than a panic; bounds against `len` are the caller's to check.
    pub fn get(&self, i: usize) -> bool {
        let byte_idx = i / 8;
        let bit_idx = i % 8;
        self.bits
            .as_bytes()
            .get(byte_idx)
            .is_some_and(|byte| (byte >> bit_idx) & 1 == 1)
    }

    /// Byte `index` of the bitmap with every bit at or beyond `len` cleared, and `0` for a byte
    /// the storage does not hold — what equality and `count_ones` compare and count.
    fn masked_byte(&self, index: usize) -> u8 {
        let byte = self.bits.as_bytes().get(index).copied().unwrap_or(0);
        let first_bit = index * 8;
        if first_bit >= self.len {
            0
        } else if self.len - first_bit >= 8 {
            byte
        } else {
            byte & ((1u8 << (self.len - first_bit)) - 1)
        }
    }

    /// Whether the storage holds at least one byte per eight bits of `len` — always true for a
    /// bitmap built through this API; checked by [`crate::column::Column::validate`] for one
    /// that arrived through deserialization.
    pub(crate) fn has_storage_for_len(&self) -> bool {
        self.bits.as_bytes().len() >= self.len.div_ceil(8)
    }

    /// The bitmap's raw bytes, LSB-first within each byte — exactly Arrow's validity/`Bool`
    /// buffer layout ([COLUMNAR] *Validity bitmaps*). Mirrors [`AlignedBuffer::as_bytes`]; used by
    /// the Arrow IPC writer (`formats/ipc.rs`) to hand off a validity or `Bool` values buffer
    /// without reconstructing it bit by bit.
    pub fn as_bytes(&self) -> &[u8] {
        self.bits.as_bytes()
    }

    /// The inverse of [`Bitmap::as_bytes`]: `len` bits packed LSB-first into `bytes`, as Arrow's
    /// IPC reader (`formats/ipc.rs`) decodes a validity or `Bool` values buffer from a file. Only
    /// the `len.div_ceil(8)` bytes the bitmap needs are kept, and the bits of the last one beyond
    /// `len` (the padding out to a whole byte) are cleared, matching [`Bitmap::not`]'s treatment
    /// of trailing padding. A `bytes` shorter than that reads as clear bits past its end — this
    /// constructor does not fail; a caller reading untrusted bytes that must refuse a short
    /// buffer checks `bytes.len() >= len.div_ceil(8)` itself.
    pub fn from_bytes(bytes: &[u8], len: usize) -> Bitmap {
        let byte_len = len.div_ceil(8);
        let mut kept = vec![0u8; byte_len];
        let available = bytes.len().min(byte_len);
        kept[..available].copy_from_slice(&bytes[..available]);
        let remainder = len % 8;
        if remainder != 0 {
            if let Some(last) = kept.last_mut() {
                *last &= (1u8 << remainder) - 1;
            }
        }
        Bitmap {
            bits: AlignedBuffer::from_slice(&kept),
            len,
        }
    }

    pub fn and(&self, other: &Bitmap) -> Result<Bitmap, Error> {
        if self.len != other.len {
            return Err(Error::general_error(format!(
                "Bitmap::and: length mismatch ({} vs {})",
                self.len, other.len
            )));
        }
        let bytes: Vec<u8> = (0..self.len.div_ceil(8))
            .map(|index| self.masked_byte(index) & other.masked_byte(index))
            .collect();
        Ok(Bitmap {
            bits: AlignedBuffer::from_slice(&bytes),
            len: self.len,
        })
    }

    pub fn or(&self, other: &Bitmap) -> Result<Bitmap, Error> {
        if self.len != other.len {
            return Err(Error::general_error(format!(
                "Bitmap::or: length mismatch ({} vs {})",
                self.len, other.len
            )));
        }
        let bytes: Vec<u8> = (0..self.len.div_ceil(8))
            .map(|index| self.masked_byte(index) | other.masked_byte(index))
            .collect();
        Ok(Bitmap {
            bits: AlignedBuffer::from_slice(&bytes),
            len: self.len,
        })
    }

    pub fn not(&self) -> Bitmap {
        let mut bytes: Vec<u8> = (0..self.len.div_ceil(8)).map(|index| !self.masked_byte(index)).collect();
        // The last byte may hold bits beyond `len` (padding out to a byte boundary); mask them
        // back to zero so `count_ones`/`iter_ones` never see a stray bit that a caller never set.
        let remainder = self.len % 8;
        if remainder != 0 {
            if let Some(last) = bytes.last_mut() {
                *last &= (1u8 << remainder) - 1;
            }
        }
        Bitmap {
            bits: AlignedBuffer::from_slice(&bytes),
            len: self.len,
        }
    }

    /// Set bits among the first `len` — never a padding bit beyond them.
    pub fn count_ones(&self) -> usize {
        (0..self.len.div_ceil(8))
            .map(|index| self.masked_byte(index).count_ones() as usize)
            .sum()
    }

    /// Positions of set bits, in order — how a mask becomes a `RowIndexView`'s indices.
    pub fn iter_ones(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.len).filter(move |&i| self.get(i))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligned_buffer_alignment_is_64_bytes() {
        let data = vec![0u8; 200];
        let buffer = AlignedBuffer::from_slice(&data);
        assert_eq!(buffer.as_bytes().as_ptr() as usize % 64, 0);
    }

    #[test]
    fn buffer_from_slice_and_as_slice_roundtrip() {
        let data = vec![1i64, 2, 3, 4, 5];
        let buffer = Buffer::from_slice(&data);
        assert_eq!(buffer.as_slice(), &data[..]);
    }

    #[test]
    fn buffer_cast_to_bytes_has_expected_length() {
        let data = vec![256u64, 512, 1024];
        let buffer = Buffer::from_slice(&data);
        assert_eq!(buffer.as_bytes().len(), data.len() * 8);
    }

    #[test]
    fn bitmap_new_is_all_clear() {
        let bitmap = Bitmap::new(5);
        assert_eq!(bitmap.len(), 5);
        assert_eq!(bitmap.count_ones(), 0);
        for i in 0..5 {
            assert!(!bitmap.get(i));
        }
    }

    #[test]
    fn bitmap_from_bools_roundtrips_through_get() {
        let bitmap = Bitmap::from_bools(&[true, false, true, false, false]);
        assert_eq!(bitmap.len(), 5);
        assert!(bitmap.get(0));
        assert!(!bitmap.get(1));
        assert!(bitmap.get(2));
    }

    #[test]
    fn bitmap_set_mutates_a_single_bit() {
        let mut bitmap = Bitmap::new(4);
        bitmap.set(2, true);
        assert!(!bitmap.get(0));
        assert!(!bitmap.get(1));
        assert!(bitmap.get(2));
        assert!(!bitmap.get(3));
        bitmap.set(2, false);
        assert!(!bitmap.get(2));
    }

    #[test]
    fn bitmap_and_combines_masks() {
        let a = Bitmap::from_bools(&[true, true, true, true, false, false, false, false]);
        let b = Bitmap::from_bools(&[true, false, true, false, true, false, true, false]);
        let result = a.and(&b).expect("and");
        assert!(result.get(0));
        assert!(!result.get(1));
        assert!(result.get(2));
        assert!(!result.get(4));
    }

    #[test]
    fn bitmap_or_combines_masks() {
        let a = Bitmap::from_bools(&[true, true, false, false]);
        let b = Bitmap::from_bools(&[false, false, true, true]);
        let result = a.or(&b).expect("or");
        assert_eq!(result.count_ones(), 4);
    }

    #[test]
    fn bitmap_not_inverts_mask() {
        let bitmap = Bitmap::from_bools(&[true, false, true, false, true, false, true, false]);
        assert_eq!(bitmap.not().count_ones(), 4);
    }

    #[test]
    fn bitmap_count_ones_returns_set_bit_count() {
        let bitmap = Bitmap::from_bools(&[true, true, true, true, false, true, false, true]);
        assert_eq!(bitmap.count_ones(), 6);
    }

    #[test]
    fn bitmap_iter_ones_yields_set_positions_in_order() {
        let bitmap = Bitmap::from_bools(&[true, false, true, false, true, false, false, false]);
        let positions: Vec<usize> = bitmap.iter_ones().collect();
        assert_eq!(positions, vec![0, 2, 4]);
    }

    #[test]
    fn bitmap_len_reports_bit_count_not_byte_count() {
        let bitmap = Bitmap::from_bools(&[true; 5]);
        assert_eq!(bitmap.len(), 5); // not 8, even though it is stored in one byte
    }

    #[test]
    fn bitmap_and_length_mismatch_error() {
        let a = Bitmap::new(8);
        let b = Bitmap::new(16);
        assert!(a.and(&b).is_err());
    }

    #[test]
    fn bitmap_or_length_mismatch_error() {
        let a = Bitmap::new(8);
        let b = Bitmap::new(16);
        assert!(a.or(&b).is_err());
    }

    // --- Additional coverage beyond Phase 3 §2.2 ---

    #[test]
    fn aligned_buffer_as_bytes_excludes_chunk_padding() {
        let data = vec![7u8; 5]; // well under one 64-byte chunk
        let buffer = AlignedBuffer::from_slice(&data);
        assert_eq!(buffer.as_bytes(), &data[..]);
    }

    #[test]
    fn aligned_buffer_serde_roundtrips_through_json() {
        let data: Vec<u8> = (0..130).collect(); // spans more than one 64-byte chunk
        let buffer = AlignedBuffer::from_slice(&data);
        let json = serde_json::to_string(&buffer).expect("serialize");
        let restored: AlignedBuffer = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, buffer);
        assert_eq!(restored.as_bytes(), &data[..]);
    }

    #[test]
    fn bitmap_serde_roundtrips_through_json() {
        let bitmap = Bitmap::from_bools(&[true, false, true, true, false]);
        let json = serde_json::to_string(&bitmap).expect("serialize");
        let restored: Bitmap = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, bitmap);
    }

    #[test]
    fn bitmap_not_masks_trailing_padding_bits() {
        // len = 5 leaves 3 unused bits in the backing byte; `not` must not count them.
        let bitmap = Bitmap::from_bools(&[false, false, false, false, false]);
        let inverted = bitmap.not();
        assert_eq!(inverted.len(), 5);
        assert_eq!(inverted.count_ones(), 5);
    }

    #[test]
    fn buffer_supports_float_element_type() {
        let data = vec![1.5f32, 2.5, 3.5];
        let buffer = Buffer::from_slice(&data);
        assert_eq!(buffer.as_slice(), &data[..]);
    }

    #[test]
    fn buffer_serde_roundtrips_through_json_as_readable_values() {
        let data = vec![10i64, -20, 30, 40];
        let buffer = Buffer::from_slice(&data);
        let json = serde_json::to_string(&buffer).expect("serialize");
        assert_eq!(json, "[10,-20,30,40]"); // readable numbers, not base64 bytes
        let restored: Buffer<i64> = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.as_slice(), &data[..]);
    }

    #[test]
    fn bitmap_as_bytes_round_trips_through_from_bytes() {
        let bitmap = Bitmap::from_bools(&[true, false, true, true, false, false, true, false, true]);
        let bytes = bitmap.as_bytes().to_vec();
        let restored = Bitmap::from_bytes(&bytes, bitmap.len());
        assert_eq!(restored, bitmap);
        for i in 0..bitmap.len() {
            assert_eq!(restored.get(i), bitmap.get(i));
        }
    }

    #[test]
    fn bitmap_from_bytes_ignores_trailing_padding_bits() {
        // 5 bits requested from a byte whose top 3 bits are set — from_bytes must not surface them.
        let restored = Bitmap::from_bytes(&[0b1110_1010], 5);
        assert_eq!(restored.len(), 5);
        assert!(!restored.get(0));
        assert!(restored.get(1));
        assert!(!restored.get(2));
        assert!(restored.get(3));
        assert!(!restored.get(4));
        // The padding bits are not part of the bitmap for any other operation either.
        assert_eq!(restored.count_ones(), 2);
        assert_eq!(restored, Bitmap::from_bools(&[false, true, false, true, false]));
        assert_eq!(restored.not().count_ones(), 3);
    }

    #[test]
    fn bitmap_from_bytes_keeps_only_the_bytes_it_needs() {
        // Trailing whole bytes beyond `len.div_ceil(8)` (an Arrow buffer padded to 64 bytes) and
        // trailing bits of the last byte are both dropped.
        let restored = Bitmap::from_bytes(&[0b1111_1111, 0b1111_1111, 0xFF, 0xFF], 10);
        assert_eq!(restored.len(), 10);
        assert_eq!(restored.count_ones(), 10);
        assert_eq!(restored, Bitmap::from_bools(&[true; 10]));
        assert_eq!(restored.not().count_ones(), 0);
        assert_eq!(restored.as_bytes().len(), 2);
        // A short input reads as clear bits rather than panicking later.
        let short = Bitmap::from_bytes(&[0b0000_0001], 12);
        assert_eq!(short.len(), 12);
        assert!(short.get(0));
        assert!(!short.get(11));
        assert_eq!(short.count_ones(), 1);
    }

    #[test]
    fn bitmap_equality_and_count_ignore_bits_beyond_len() {
        // Built through serde, which bypasses `from_bytes`: the stray bits must still not count.
        let json = serde_json::to_string(&Bitmap::from_bools(&[true, false, true])).expect("serialize");
        let mut value: serde_json::Value = serde_json::from_str(&json).expect("json");
        let bits = value.get_mut("bits").expect("bits field");
        *bits = serde_json::to_value(AlignedBuffer::from_slice(&[0b1111_0101])).expect("bits");
        let dirty: Bitmap = serde_json::from_value(value).expect("deserialize");
        assert_eq!(dirty.count_ones(), 2);
        assert_eq!(dirty, Bitmap::from_bools(&[true, false, true]));
    }

    #[test]
    fn aligned_buffer_clone_shares_storage_but_set_does_not_mutate_the_clone() {
        let mut bitmap = Bitmap::from_bools(&[false, false, false]);
        let clone = bitmap.clone();
        bitmap.set(1, true);
        assert!(bitmap.get(1));
        assert!(!clone.get(1));
    }
}
