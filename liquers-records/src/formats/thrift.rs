//! A minimal Thrift compact-protocol encoder for the handful of structures the Parquet writer
//! (`formats/parquet.rs`) needs — not a general Thrift implementation, and no reader: this crate
//! never reads Parquet (`specs/design/record-streams/phase2-architecture.md` §"Tier 3 — Parquet:
//! writing is cheap, reading is not"). Matches `formats/ipc.rs`'s "no generated code" decision: a
//! small, from-scratch encoder against the wire format, not a generated client of a Thrift crate.
//!
//! Reference: the [Thrift compact protocol
//! specification](https://github.com/apache/thrift/blob/master/doc/specs/thrift-compact-protocol.md).
//! Field ids used by `formats/parquet.rs` were cross-checked (never executed) against the
//! generated Rust in the `polars-parquet-format` crate's sources
//! (`~/.cargo/registry/src/*/polars-parquet-format-*/src/parquet_format.rs`), which carries the
//! same field ids `parquet.thrift` declares.
//!
//! **Structs carry no length or type marker of their own in the compact protocol** — only field
//! headers plus a trailing stop byte (`0x00`) represent them, and a nested struct's field-id-delta
//! tracking restarts at each nesting level (what the protocol's own `write_struct_begin`/
//! `write_struct_end` would otherwise track on a stack). This encoder does not keep that stack
//! itself; instead every `write_*_struct`-shaped helper in `parquet.rs` opens with its own fresh
//! `let mut last_id: i16 = 0;` and passes it by `&mut` through this module's field writers — the
//! caller supplies exactly the discipline nesting requires.

/// Compact-protocol field-type nibbles (Thrift's `CompactType`), used both as a field header's low
/// nibble and as a list's element-type byte.
pub const TYPE_BOOL_TRUE: u8 = 1;
pub const TYPE_BOOL_FALSE: u8 = 2;
pub const TYPE_BYTE: u8 = 3;
pub const TYPE_I32: u8 = 5;
pub const TYPE_I64: u8 = 6;
pub const TYPE_BINARY: u8 = 8;
pub const TYPE_LIST: u8 = 9;
pub const TYPE_STRUCT: u8 = 12;

/// Zigzag-encodes a signed integer to its unsigned varint payload — `(n << 1) ^ (n >> BITS-1)`,
/// generalized to whichever width `n` is at the call site. The shifts never panic: Rust's `<<`/`>>`
/// only panic when the *shift amount* exceeds the type's bit width, not when bits are shifted out
/// of value range, which is exactly what a zigzag encoding relies on.
fn zigzag16(n: i16) -> u16 {
    ((n << 1) ^ (n >> 15)) as u16
}
fn zigzag32(n: i32) -> u32 {
    ((n << 1) ^ (n >> 31)) as u32
}
fn zigzag64(n: i64) -> u64 {
    ((n << 1) ^ (n >> 63)) as u64
}

/// The unsigned LEB128 varint every Thrift compact integer (after zigzag) and every list/binary
/// length uses: 7 payload bits per byte, low-to-high, continuation bit set on every byte but the
/// last. Also reused by `formats/parquet.rs` to encode a definition-levels run header, which uses
/// the identical unsigned varint (Parquet's "RLE / Bit-Packed Hybrid" encoding borrows Thrift's
/// varint verbatim).
pub fn write_unsigned_varint(buf: &mut Vec<u8>, mut n: u64) {
    loop {
        let byte = (n & 0x7F) as u8;
        n >>= 7;
        if n == 0 {
            buf.push(byte);
            break;
        }
        buf.push(byte | 0x80);
    }
}

/// A byte sink for the Thrift compact-protocol shapes `formats/parquet.rs` needs: struct field
/// headers (short form when the id delta from the previous field is `1..=15`, long form
/// otherwise), scalar field values, and list headers/elements.
#[derive(Debug, Default)]
pub struct CompactWriter {
    pub buf: Vec<u8>,
}

impl CompactWriter {
    pub fn new() -> Self {
        CompactWriter { buf: Vec::new() }
    }

    /// A field header: short form (one byte, `(delta << 4) | type`) when `id - *last_id` is in
    /// `1..=15`; the long form otherwise (a bare type byte, then the zigzag-varint field id).
    /// Updates `*last_id` either way.
    fn write_field_header(&mut self, last_id: &mut i16, id: i16, ctype: u8) {
        let delta = id.wrapping_sub(*last_id);
        if (1..=15).contains(&delta) {
            self.buf.push(((delta as u8) << 4) | ctype);
        } else {
            self.buf.push(ctype);
            write_unsigned_varint(&mut self.buf, zigzag16(id) as u64);
        }
        *last_id = id;
    }

    /// `bool` fields fold their value into the field header's type nibble
    /// (`TYPE_BOOL_TRUE`/`TYPE_BOOL_FALSE`) — the compact protocol writes no separate value byte
    /// for a struct-field bool.
    pub fn write_bool_field(&mut self, last_id: &mut i16, id: i16, value: bool) {
        let ctype = if value { TYPE_BOOL_TRUE } else { TYPE_BOOL_FALSE };
        self.write_field_header(last_id, id, ctype);
    }

    /// An `i8` field: a single raw byte after the header (no zigzag, no varint).
    pub fn write_i8_field(&mut self, last_id: &mut i16, id: i16, value: i8) {
        self.write_field_header(last_id, id, TYPE_BYTE);
        self.buf.push(value.to_le_bytes()[0]);
    }

    pub fn write_i32_field(&mut self, last_id: &mut i16, id: i16, value: i32) {
        self.write_field_header(last_id, id, TYPE_I32);
        write_unsigned_varint(&mut self.buf, zigzag32(value) as u64);
    }

    pub fn write_i64_field(&mut self, last_id: &mut i16, id: i16, value: i64) {
        self.write_field_header(last_id, id, TYPE_I64);
        write_unsigned_varint(&mut self.buf, zigzag64(value));
    }

    pub fn write_binary_field(&mut self, last_id: &mut i16, id: i16, bytes: &[u8]) {
        self.write_field_header(last_id, id, TYPE_BINARY);
        write_unsigned_varint(&mut self.buf, bytes.len() as u64);
        self.buf.extend_from_slice(bytes);
    }

    pub fn write_string_field(&mut self, last_id: &mut i16, id: i16, value: &str) {
        self.write_binary_field(last_id, id, value.as_bytes());
    }

    /// Opens a `TYPE_STRUCT` field; the caller writes the nested struct's own fields (with a
    /// fresh `last_id`) and that struct's own stop byte immediately after.
    pub fn write_struct_field_header(&mut self, last_id: &mut i16, id: i16) {
        self.write_field_header(last_id, id, TYPE_STRUCT);
    }

    /// Opens a `TYPE_LIST` field; the caller writes [`CompactWriter::write_list_header`] and the
    /// elements immediately after.
    pub fn write_list_field_header(&mut self, last_id: &mut i16, id: i16) {
        self.write_field_header(last_id, id, TYPE_LIST);
    }

    /// A list's own header: short form (`(size << 4) | elem_type`) for `size < 15`, else a
    /// `0xF0 | elem_type` marker followed by the size as a plain (non-zigzag — sizes are never
    /// negative) varint.
    pub fn write_list_header(&mut self, elem_type: u8, size: usize) {
        if size < 15 {
            self.buf.push(((size as u8) << 4) | elem_type);
        } else {
            self.buf.push(0xF0 | elem_type);
            write_unsigned_varint(&mut self.buf, size as u64);
        }
    }

    /// A list-of-`i32` element: headerless (the list header already named the element type), just
    /// the zigzag varint.
    pub fn write_i32_elem(&mut self, value: i32) {
        write_unsigned_varint(&mut self.buf, zigzag32(value) as u64);
    }

    pub fn write_string_elem(&mut self, value: &str) {
        write_unsigned_varint(&mut self.buf, value.len() as u64);
        self.buf.extend_from_slice(value.as_bytes());
    }

    /// The stop byte (`0x00`) ending a struct's field list — the compact protocol's only
    /// struct-framing byte; there is no length or begin marker.
    pub fn write_stop(&mut self) {
        self.buf.push(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_form_field_header_encodes_delta_in_the_high_nibble() {
        let mut w = CompactWriter::new();
        let mut last = 0i16;
        w.write_i32_field(&mut last, 3, 0); // delta 3, I32 type (5) => 0x35
        assert_eq!(w.buf[0], 0x35);
        assert_eq!(last, 3);
    }

    #[test]
    fn long_form_field_header_used_when_delta_exceeds_fifteen() {
        let mut w = CompactWriter::new();
        let mut last = 0i16;
        w.write_i32_field(&mut last, 20, 0);
        assert_eq!(w.buf[0], TYPE_I32); // bare type byte, no delta folded in
        assert_eq!(w.buf[1], 40); // zigzag(20) = 40, fits one varint byte
    }

    #[test]
    fn bool_field_folds_the_value_into_the_type_nibble() {
        let mut w = CompactWriter::new();
        let mut last = 0i16;
        w.write_bool_field(&mut last, 1, true);
        assert_eq!(w.buf, vec![(1 << 4) | TYPE_BOOL_TRUE]);
        let mut w = CompactWriter::new();
        let mut last = 0i16;
        w.write_bool_field(&mut last, 1, false);
        assert_eq!(w.buf, vec![(1 << 4) | TYPE_BOOL_FALSE]);
    }

    #[test]
    fn zigzag_round_trips_negative_and_positive_values() {
        assert_eq!(zigzag32(0), 0);
        assert_eq!(zigzag32(-1), 1);
        assert_eq!(zigzag32(1), 2);
        assert_eq!(zigzag32(-2), 3);
        assert_eq!(zigzag64(-1), 1);
        assert_eq!(zigzag16(-1), 1);
    }

    #[test]
    fn binary_field_writes_length_then_bytes() {
        let mut w = CompactWriter::new();
        let mut last = 0i16;
        w.write_binary_field(&mut last, 1, b"ab");
        assert_eq!(w.buf, vec![(1 << 4) | TYPE_BINARY, 2, b'a', b'b']);
    }

    #[test]
    fn list_header_short_form_for_small_sizes() {
        let mut w = CompactWriter::new();
        w.write_list_header(TYPE_STRUCT, 3);
        assert_eq!(w.buf, vec![(3 << 4) | TYPE_STRUCT]);
    }

    #[test]
    fn list_header_long_form_for_large_sizes() {
        let mut w = CompactWriter::new();
        w.write_list_header(TYPE_I32, 20);
        assert_eq!(w.buf[0], 0xF0 | TYPE_I32);
        assert_eq!(w.buf[1], 20); // varint(20), one byte
    }

    #[test]
    fn stop_byte_is_zero() {
        let mut w = CompactWriter::new();
        w.write_stop();
        assert_eq!(w.buf, vec![0]);
    }

    #[test]
    fn unsigned_varint_multi_byte_round_trip_shape() {
        let mut buf = Vec::new();
        write_unsigned_varint(&mut buf, 300); // 300 = 0b1_0010_1100 -> needs 2 bytes
        assert_eq!(buf, vec![0b1010_1100, 0b0000_0010]);
    }

    #[test]
    fn i64_field_uses_the_i64_type_and_a_zigzag_varint() {
        let mut w = CompactWriter::new();
        let mut last = 0i16;
        w.write_i64_field(&mut last, 1, -5);
        assert_eq!(w.buf[0] & 0x0F, TYPE_I64);
        // zigzag(-5) = 9, fits in one varint byte
        assert_eq!(w.buf[1], 9);
    }
}
