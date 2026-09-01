//! Extracting the JSON payload from a CMS-signed association file.
//!
//! Before iOS 10, `apple-app-site-association` had to be a PKCS#7 / CMS `SignedData` blob with the
//! JSON as its encapsulated content. Those files are still served in the wild, and handing one to a
//! JSON parser produces a baffling error about byte 0.
//!
//! This module recognises such a file and pulls the payload out. It does **not** verify the
//! signature — that needs a certificate chain, a trust store, and a crypto stack, none of which
//! belong in a semantics crate. Extraction is reported with a diagnostic saying exactly that, so
//! nobody mistakes "we read it" for "we checked it".

/// `1.2.840.113549.1.7.1` — the CMS `id-data` content type, whose `eContent` holds the JSON.
const OID_PKCS7_DATA: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x07, 0x01];

/// Tag *numbers*, i.e. the low five bits of an identifier octet, which is what `read` reports.
const TAG_OCTET_STRING: u8 = 0x04;
const TAG_OID: u8 = 0x06;
const TAG_SEQUENCE: u8 = 0x10;
/// The full identifier octet for a constructed SEQUENCE, used for sniffing and by the tests.
const DER_SEQUENCE_BYTE: u8 = 0x30;
/// Nested `SignedData` is shallow in practice; this only guards against malicious nesting.
const MAX_DEPTH: u8 = 24;

/// Whether `bytes` look like DER rather than JSON.
///
/// JSON documents start with `{` after optional whitespace; a DER `ContentInfo` starts with a
/// SEQUENCE tag. That is enough to tell them apart without parsing either.
#[must_use]
pub(crate) fn looks_like_der(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .find(|byte| !byte.is_ascii_whitespace())
        .is_some_and(|byte| *byte == DER_SEQUENCE_BYTE)
}

/// One DER element: its tag, its contents, and where the next element starts.
struct Element<'a> {
    tag: u8,
    constructed: bool,
    contents: &'a [u8],
    end: usize,
}

/// Reads the element beginning at `offset`, or `None` if the encoding is malformed.
///
/// Every length and offset is checked; malformed input yields `None` rather than a panic.
fn read(bytes: &[u8], offset: usize) -> Option<Element<'_>> {
    let tag = *bytes.get(offset)?;
    // A multi-byte tag number (low five bits all set) never appears in the structures we walk.
    if tag & 0x1f == 0x1f {
        return None;
    }
    let first_length = *bytes.get(offset + 1)?;
    let (length, header) = if first_length & 0x80 == 0 {
        (usize::from(first_length), 2)
    } else {
        let count = usize::from(first_length & 0x7f);
        // Indefinite length (0x80) is BER, not DER, and is not supported here.
        if count == 0 || count > 4 {
            return None;
        }
        let mut length = 0usize;
        for index in 0..count {
            let byte = *bytes.get(offset + 2 + index)?;
            length = length.checked_mul(256)?.checked_add(usize::from(byte))?;
        }
        (length, 2 + count)
    };

    let start = offset.checked_add(header)?;
    let end = start.checked_add(length)?;
    if end > bytes.len() {
        return None;
    }
    Some(Element {
        tag: tag & 0x1f,
        constructed: tag & 0x20 != 0,
        contents: &bytes[start..end],
        end,
    })
}

/// Concatenates a possibly-constructed OCTET STRING into one buffer.
fn octet_string(element: &Element<'_>, depth: u8) -> Option<Vec<u8>> {
    if !element.constructed {
        return Some(element.contents.to_vec());
    }
    if depth == 0 {
        return None;
    }
    let mut out = Vec::new();
    let mut offset = 0;
    while offset < element.contents.len() {
        let child = read(element.contents, offset)?;
        if child.tag != TAG_OCTET_STRING {
            return None;
        }
        out.extend_from_slice(&octet_string(&child, depth - 1)?);
        offset = child.end;
    }
    Some(out)
}

/// Walks `bytes` looking for an `EncapsulatedContentInfo` holding `id-data`, and returns its
/// `eContent`.
fn find_payload(bytes: &[u8], depth: u8) -> Option<Vec<u8>> {
    if depth == 0 {
        return None;
    }
    let mut offset = 0;
    while offset < bytes.len() {
        let element = read(bytes, offset)?;

        if element.tag == TAG_SEQUENCE && element.constructed {
            // EncapsulatedContentInfo ::= SEQUENCE { eContentType OID, eContent [0] EXPLICIT ... }
            if let Some(first) = read(element.contents, 0) {
                if first.tag == TAG_OID && first.contents == OID_PKCS7_DATA {
                    if let Some(wrapper) = read(element.contents, first.end) {
                        // The [0] EXPLICIT wrapper contains the OCTET STRING.
                        if let Some(inner) = read(wrapper.contents, 0) {
                            if inner.tag == TAG_OCTET_STRING {
                                if let Some(payload) = octet_string(&inner, MAX_DEPTH) {
                                    if !payload.is_empty() {
                                        return Some(payload);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if element.constructed {
            if let Some(found) = find_payload(element.contents, depth - 1) {
                return Some(found);
            }
        }
        offset = element.end;
    }
    None
}

/// Extracts the JSON payload from a CMS-signed association file.
///
/// Returns `None` when `bytes` are not a recognisable `SignedData` carrying `id-data`. The
/// signature is never checked.
#[must_use]
pub(crate) fn extract_payload(bytes: &[u8]) -> Option<Vec<u8>> {
    let start = bytes.iter().position(|byte| !byte.is_ascii_whitespace())?;
    find_payload(&bytes[start..], MAX_DEPTH)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal but structurally real CMS `SignedData` around `payload`.
    fn wrap(payload: &[u8]) -> Vec<u8> {
        fn tlv(tag: u8, contents: &[u8]) -> Vec<u8> {
            let mut out = vec![tag];
            let length = contents.len();
            if length < 0x80 {
                #[allow(clippy::cast_possible_truncation)]
                out.push(length as u8);
            } else if length < 0x100 {
                #[allow(clippy::cast_possible_truncation)]
                out.extend_from_slice(&[0x81, length as u8]);
            } else {
                #[allow(clippy::cast_possible_truncation)]
                out.extend_from_slice(&[0x82, (length >> 8) as u8, (length & 0xff) as u8]);
            }
            out.extend_from_slice(contents);
            out
        }

        let econtent = tlv(0xa0, &tlv(TAG_OCTET_STRING, payload));
        let mut encap = tlv(TAG_OID, OID_PKCS7_DATA);
        encap.extend_from_slice(&econtent);
        let encap = tlv(DER_SEQUENCE_BYTE, &encap);

        let mut signed_data = tlv(0x02, &[0x01]); // version
        signed_data.extend_from_slice(&tlv(0x31, &[])); // digestAlgorithms
        signed_data.extend_from_slice(&encap);
        let signed_data = tlv(DER_SEQUENCE_BYTE, &signed_data);

        let mut content_info = tlv(
            TAG_OID,
            &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x07, 0x02],
        );
        content_info.extend_from_slice(&tlv(0xa0, &signed_data));
        tlv(DER_SEQUENCE_BYTE, &content_info)
    }

    #[test]
    fn json_is_not_mistaken_for_der() {
        assert!(!looks_like_der(br#"{"applinks":{}}"#));
        assert!(!looks_like_der(b"   \n{}"));
        assert!(!looks_like_der(b""));
    }

    #[test]
    fn a_signed_payload_round_trips() {
        let payload = br#"{"applinks":{"details":[]}}"#;
        let signed = wrap(payload);
        assert!(looks_like_der(&signed));
        assert_eq!(extract_payload(&signed).as_deref(), Some(&payload[..]));
    }

    #[test]
    fn a_long_payload_round_trips() {
        let payload = format!(r#"{{"comment":"{}"}}"#, "x".repeat(1000));
        let signed = wrap(payload.as_bytes());
        assert_eq!(
            extract_payload(&signed).as_deref(),
            Some(payload.as_bytes())
        );
    }

    #[test]
    fn malformed_der_never_panics() {
        let payload = wrap(br#"{"a":1}"#);
        for cut in 0..payload.len() {
            let _ = extract_payload(&payload[..cut]);
        }
        for index in 0..payload.len() {
            let mut damaged = payload.clone();
            damaged[index] ^= 0xff;
            let _ = extract_payload(&damaged);
        }
        let _ = extract_payload(&[0x30, 0x80]);
        let _ = extract_payload(&[0x30, 0x84, 0xff, 0xff, 0xff, 0xff]);
    }
}
