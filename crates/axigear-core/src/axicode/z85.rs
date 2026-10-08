//! Z85 (ZeroMQ RFC 32) decoding, as used by AxiCode build payloads.

use super::DecodeError;

const ALPHABET: &[u8; 85] =
    b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ.-:+=^!/*?&<>()[]{}@%$#";

pub fn decode(s: &str) -> Result<Vec<u8>, DecodeError> {
    let bytes = s.as_bytes();
    if bytes.len() % 5 != 0 {
        return Err(DecodeError::Corrupt);
    }
    let mut out = Vec::with_capacity(bytes.len() / 5 * 4);
    for group in bytes.chunks(5) {
        let mut value: u64 = 0;
        for &c in group {
            let digit = ALPHABET.iter().position(|&a| a == c).ok_or(DecodeError::Corrupt)?;
            value = value * 85 + digit as u64;
        }
        if value > u32::MAX as u64 {
            return Err(DecodeError::Corrupt);
        }
        out.extend_from_slice(&(value as u32).to_be_bytes());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_the_rfc_test_vector() {
        assert_eq!(
            decode("HelloWorld").unwrap(),
            vec![0x86, 0x4F, 0xD2, 0x6F, 0xB5, 0x59, 0xF7, 0x5B]
        );
    }

    #[test]
    fn rejects_length_not_multiple_of_five() {
        assert_eq!(decode("Hell"), Err(DecodeError::Corrupt));
    }

    #[test]
    fn rejects_characters_outside_the_alphabet() {
        assert_eq!(decode("Hell~"), Err(DecodeError::Corrupt));
        assert_eq!(decode("Hellé"), Err(DecodeError::Corrupt));
    }
}
