//! Just enough protobuf wire format for the collection service's few small messages, whose
//! definitions librespot doesn't compile in.

/// One decoded field: its number and its value.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Field<'a> {
    Varint(u32, u64),
    Bytes(u32, &'a [u8]),
}

fn read_varint(input: &mut &[u8]) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let (&byte, rest) = input.split_first()?;
        *input = rest;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

/// The fields of a message in order. Fixed-width fields are skipped; a truncated or
/// malformed message yields `None`.
pub(super) fn fields(mut input: &[u8]) -> Option<Vec<Field<'_>>> {
    let mut fields = Vec::new();
    while !input.is_empty() {
        let key = read_varint(&mut input)?;
        let number = u32::try_from(key >> 3).ok()?;
        match key & 7 {
            0 => fields.push(Field::Varint(number, read_varint(&mut input)?)),
            1 => input = input.get(8..)?,
            2 => {
                let length = usize::try_from(read_varint(&mut input)?).ok()?;
                let (bytes, rest) = input.split_at_checked(length)?;
                fields.push(Field::Bytes(number, bytes));
                input = rest;
            }
            5 => input = input.get(4..)?,
            _ => return None,
        }
    }
    Some(fields)
}

fn write_varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push(value as u8 | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

pub(super) fn put_bytes(out: &mut Vec<u8>, number: u32, bytes: &[u8]) {
    write_varint(out, u64::from(number) << 3 | 2);
    write_varint(out, bytes.len() as u64);
    out.extend_from_slice(bytes);
}

pub(super) fn put_varint(out: &mut Vec<u8>, number: u32, value: u64) {
    write_varint(out, u64::from(number) << 3);
    write_varint(out, value);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_written_reads_back() {
        let mut inner = Vec::new();
        put_bytes(&mut inner, 1, b"spotify:track:x");
        put_varint(&mut inner, 2, 1_794_349_469);
        put_varint(&mut inner, 3, 1);
        let mut message = Vec::new();
        put_bytes(&mut message, 1, &inner);
        put_bytes(&mut message, 2, b"next");
        let outer = fields(&message).unwrap();
        assert_eq!(outer[1], Field::Bytes(2, b"next"));
        let Field::Bytes(1, item) = outer[0] else { panic!("the item is field 1") };
        assert_eq!(fields(item).unwrap(), [Field::Bytes(1, b"spotify:track:x"), Field::Varint(2, 1_794_349_469), Field::Varint(3, 1)]);
    }

    #[test]
    fn fixed_width_fields_are_skipped_and_truncation_is_refused() {
        let with_fixed = [0x0d, 1, 2, 3, 4, 0x11, 1, 2, 3, 4, 5, 6, 7, 8, 0x18, 0x96, 0x01];
        assert_eq!(fields(&with_fixed).unwrap(), [Field::Varint(3, 150)]);
        assert_eq!(fields(&[0x0a, 0x05, b'a']), None);
        assert_eq!(fields(&[0x08, 0x80]), None);
        assert_eq!(fields(&[]), Some(Vec::new()));
    }
}
