//! Only raw ISF belongs in a v2 note. Check its outer frame before entering
//! Windows' native decoder; never pass GIF, Base64 or a mismatched length.
pub fn validate(bytes: &[u8]) -> windows::core::Result<()> {
    let invalid = crate::validation::invalid;
    if bytes.len() < 2 || bytes.len() > crate::note::MAX_PAGE_INK_BYTES || bytes[0] != 0 {
        return Err(invalid());
    }
    let mut length = 0u32;
    for index in 0..5 {
        let byte = *bytes.get(index + 1).ok_or_else(invalid)?;
        if index == 4 && byte & 0xf0 != 0 {
            return Err(invalid());
        }
        length |= u32::from(byte & 0x7f) << (index * 7);
        if byte & 0x80 == 0 {
            if index > 0 && byte == 0 {
                return Err(invalid());
            }
            return if length as usize == bytes.len() - (index + 2) {
                Ok(())
            } else {
                Err(invalid())
            };
        }
    }
    Err(invalid())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_image_wrappers_truncation_trailing_bytes_and_integer_overflow() {
        for bytes in [
            b"".as_slice(),
            b"GIF89a",
            b"base64:AAAA",
            &[0],
            &[0, 1],
            &[0, 0, 1],
            &[0, 0x80, 0],
            &[0, 0xff, 0xff, 0xff, 0xff, 0x10],
            &[0, 0x80, 0x80, 0x80, 0x80, 0x80],
        ] {
            assert!(validate(bytes).is_err(), "{bytes:x?}");
        }
        assert!(validate(&[0, 0]).is_ok());
        assert!(validate(&[0, 1, 0]).is_ok());
        let mut large = vec![0, 0x80, 1];
        large.resize(131, 0);
        assert!(validate(&large).is_ok());
    }
}
