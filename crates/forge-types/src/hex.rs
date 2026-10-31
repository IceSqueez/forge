const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";
const HIGH_NIBBLE_SHIFT: u8 = 4;
const LOW_NIBBLE_MASK: u8 = 0x0f;

pub fn to_lower_hex(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        hex.push(char::from(
            HEX_DIGITS[usize::from(byte >> HIGH_NIBBLE_SHIFT)],
        ));
        hex.push(char::from(HEX_DIGITS[usize::from(byte & LOW_NIBBLE_MASK)]));
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::to_lower_hex;

    #[test]
    fn renders_every_byte_as_two_lowercase_digits() {
        assert_eq!(to_lower_hex(&[]), "");
        assert_eq!(to_lower_hex(&[0x00, 0x0f, 0xa5, 0xff]), "000fa5ff");
    }
}
