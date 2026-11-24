//! The platform signature over the final PDF: a `PAdES` B-B (`CAdES` detached)
//! signature with the instance's P-256 key and its self-signed certificate.
//!
//! bizkit writes the whole final file, so there is one signed revision. The
//! file is saved with placeholders for `/ByteRange` and `/Contents`, then
//! both are patched in place: the byte range around `/Contents`, and the CMS
//! signature over SHA-256 of that range.

use std::str::FromStr as _;
use std::time::Duration;

use anyhow::{Context as _, bail};
use bizkit_core::time::UnixMillis;
use cms::builder::{SignedDataBuilder, SignerInfoBuilder};
use cms::cert::{CertificateChoices, IssuerAndSerialNumber};
use cms::content_info::ContentInfo;
use cms::signed_data::{EncapsulatedContentInfo, SignerIdentifier};
use der::asn1::{GeneralizedTime, OctetString, SetOfVec};
use der::{Any, Decode as _, Encode as _, Sequence};
use lopdf::{Dictionary, Document, Object, ObjectId, StringFormat, dictionary};
use p256::ecdsa::{DerSignature, SigningKey};
use sha2_010::{Digest as _, Sha256};
use x509_cert::Certificate;
use x509_cert::attr::Attribute;
use x509_cert::builder::{Builder as _, CertificateBuilder, Profile};
use x509_cert::name::Name;
use x509_cert::serial_number::SerialNumber;
use x509_cert::spki::{AlgorithmIdentifierOwned, ObjectIdentifier, SubjectPublicKeyInfoOwned};
use x509_cert::time::{Time, Validity};

/// Room for the CMS signature, in bytes; written as twice as many hex digits.
const CONTENTS_SIZE: usize = 8192;
/// A `ByteRange` wide enough for any file this module writes, patched in place.
const BYTE_RANGE_PLACEHOLDER: [i64; 4] = [0, 9_999_999_999, 9_999_999_999, 9_999_999_999];
const CERT_YEARS: u64 = 20;

const ID_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.1");
const ID_SHA_256: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.1");
const ID_AA_SIGNING_CERTIFICATE_V2: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.16.2.47");

/// The instance's seal key and certificate.
pub(crate) struct SealKey {
    key: SigningKey,
    certificate: Certificate,
}

impl std::fmt::Debug for SealKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SealKey").field("key", &"[redacted]").finish_non_exhaustive()
    }
}

/// `ESSCertIDv2` (RFC 5035) with the default SHA-256 hash and no issuer serial.
#[derive(Debug, Sequence)]
struct EssCertIdV2 {
    cert_hash: OctetString,
}

/// `SigningCertificateV2` (RFC 5035) naming one certificate.
#[derive(Debug, Sequence)]
struct SigningCertificateV2 {
    certs: Vec<EssCertIdV2>,
}

fn generalized(at: UnixMillis) -> anyhow::Result<GeneralizedTime> {
    let secs = u64::try_from(at.0 / 1000).context("a time before 1970")?;
    GeneralizedTime::from_unix_duration(Duration::from_secs(secs)).context("encoding a time")
}

impl SealKey {
    /// A new key and a self-signed leaf certificate for `common_name`,
    /// valid for 20 years from `now`.
    pub(crate) fn generate(common_name: &str, now: UnixMillis) -> anyhow::Result<Self> {
        let key = loop {
            let seed = Sha256::digest(bizkit_core::auth::random_token().as_bytes());
            if let Ok(key) = SigningKey::from_slice(&seed) {
                break key;
            }
        };
        let mut serial: Vec<u8> = Sha256::digest(bizkit_core::auth::random_token().as_bytes())
            .iter()
            .take(16)
            .copied()
            .collect();
        // A positive serial: DER integers are signed.
        if let Some(first) = serial.first_mut() {
            *first &= 0x7f;
        }
        let serial = SerialNumber::new(&serial).context("a certificate serial")?;
        let name = format!(
            "CN={},O=bizkit",
            common_name.replace([',', '=', '+', '"', '\\', '<', '>', ';'], " ")
        );
        let subject = Name::from_str(&name).context("the certificate name")?;
        let validity = Validity {
            not_before: Time::GeneralTime(generalized(now)?),
            not_after: Time::GeneralTime(generalized(
                now.plus(Duration::from_hours(CERT_YEARS * 365 * 24)),
            )?),
        };
        let spki =
            SubjectPublicKeyInfoOwned::from_key(*key.verifying_key()).context("the public key")?;
        let profile = Profile::Leaf {
            issuer: subject.clone(),
            enable_key_agreement: false,
            enable_key_encipherment: false,
        };
        let builder = CertificateBuilder::new(profile, serial, validity, subject, spki, &key)
            .context("building the seal certificate")?;
        let certificate =
            builder.build::<DerSignature>().context("signing the seal certificate")?;
        Ok(Self { key, certificate })
    }

    /// Reads a key stored with [`SealKey::to_parts`].
    pub(crate) fn from_parts(certificate: &[u8], key: &[u8]) -> anyhow::Result<Self> {
        let certificate =
            Certificate::from_der(certificate).context("reading the seal certificate")?;
        let key = SigningKey::from_slice(key).context("reading the seal key")?;
        Ok(Self { key, certificate })
    }

    /// The certificate (DER) and the private scalar, to store.
    pub(crate) fn to_parts(&self) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
        let certificate = self.certificate.to_der().context("encoding the seal certificate")?;
        Ok((certificate, self.key.to_bytes().to_vec()))
    }

    #[cfg(test)]
    pub(crate) const fn certificate(&self) -> &Certificate {
        &self.certificate
    }

    /// A detached CMS `SignedData` over `digest` (SHA-256 of the signed bytes).
    fn cms(&self, digest: &[u8]) -> anyhow::Result<Vec<u8>> {
        let content = EncapsulatedContentInfo { econtent_type: ID_DATA, econtent: None };
        let sha256 = AlgorithmIdentifierOwned { oid: ID_SHA_256, parameters: None };
        let tbs = &self.certificate.tbs_certificate;
        let sid = SignerIdentifier::IssuerAndSerialNumber(IssuerAndSerialNumber {
            issuer: tbs.issuer.clone(),
            serial_number: tbs.serial_number.clone(),
        });
        let cert_der = self.certificate.to_der().context("encoding the certificate")?;
        let ess = SigningCertificateV2 {
            certs: vec![EssCertIdV2 {
                cert_hash: OctetString::new(Sha256::digest(&cert_der).to_vec())
                    .context("a cert hash")?,
            }],
        };
        let ess_value = Any::encode_from(&ess).context("encoding signing-certificate-v2")?;
        let mut values = SetOfVec::new();
        values.insert(ess_value).context("signing-certificate-v2 values")?;
        let ess_attr = Attribute { oid: ID_AA_SIGNING_CERTIFICATE_V2, values };

        let mut info =
            SignerInfoBuilder::new(&self.key, sid, sha256.clone(), &content, Some(digest))
                .map_err(|err| cms_error(&err, "starting the signer info"))?;
        info.add_signed_attribute(ess_attr)
            .map_err(|err| cms_error(&err, "adding signing-certificate-v2"))?;
        let mut signed = SignedDataBuilder::new(&content);
        signed
            .add_digest_algorithm(sha256)
            .map_err(|err| cms_error(&err, "adding the digest algorithm"))?;
        signed
            .add_certificate(CertificateChoices::Certificate(self.certificate.clone()))
            .map_err(|err| cms_error(&err, "adding the certificate"))?;
        signed
            .add_signer_info::<_, DerSignature>(info)
            .map_err(|err| cms_error(&err, "signing"))?;
        let content_info: ContentInfo =
            signed.build().map_err(|err| cms_error(&err, "building SignedData"))?;
        content_info.to_der().context("encoding SignedData")
    }
}

/// cms's builder error is not a std error.
fn cms_error(err: &cms::builder::Error, doing: &str) -> anyhow::Error {
    anyhow::anyhow!("{doing}: {err:?}")
}

/// The signature dictionary's `/M`: `D:YYYYMMDDHHmmSSZ`.
fn pdf_date(at: UnixMillis) -> anyhow::Result<String> {
    let text = at.to_rfc3339().context("formatting the signing time")?;
    let digits: String = text.chars().filter(char::is_ascii_digit).take(14).collect();
    Ok(format!("D:{digits}Z"))
}

/// Adds the invisible signature field to `doc`'s last page and the
/// `AcroForm`, with placeholder `/ByteRange` and `/Contents`. Returns the
/// signature dictionary's object ID.
fn add_signature_field(doc: &mut Document, at: UnixMillis) -> anyhow::Result<ObjectId> {
    let byte_range: Vec<Object> =
        BYTE_RANGE_PLACEHOLDER.iter().map(|n| Object::Integer(*n)).collect();
    let sig = doc.add_object(dictionary! {
        "Type" => "Sig",
        "Filter" => "Adobe.PPKLite",
        "SubFilter" => "ETSI.CAdES.detached",
        "ByteRange" => byte_range,
        "Contents" => Object::String(vec![0; CONTENTS_SIZE], StringFormat::Hexadecimal),
        "M" => Object::string_literal(pdf_date(at)?),
        "Name" => Object::string_literal("bizkit"),
        "Reason" => Object::string_literal("Sealed by bizkit after every signer signed"),
    });
    let page = *doc.get_pages().values().last().context("the PDF has no pages")?;
    let field = doc.add_object(dictionary! {
        "FT" => "Sig",
        "Type" => "Annot",
        "Subtype" => "Widget",
        "Rect" => vec![0.into(), 0.into(), 0.into(), 0.into()],
        "F" => 132,
        "P" => page,
        "T" => Object::string_literal("bizkit seal"),
        "V" => sig,
    });
    push_ref(doc, page, "Annots", field)?;
    let catalog = doc.catalog_mut().context("the PDF has no catalog")?;
    let form = match catalog.get(b"AcroForm") {
        Ok(Object::Reference(id)) => Some(*id),
        Ok(Object::Dictionary(_)) => None,
        _ => {
            catalog.set("AcroForm", Dictionary::new());
            None
        }
    };
    let form: &mut Dictionary = match form {
        Some(id) => doc.get_dictionary_mut(id).context("reading the AcroForm")?,
        None => doc
            .catalog_mut()?
            .get_mut(b"AcroForm")
            .and_then(Object::as_dict_mut)
            .context("reading the AcroForm")?,
    };
    form.remove(b"XFA");
    form.set("SigFlags", 3);
    match form.get_mut(b"Fields") {
        Ok(Object::Array(fields)) => fields.push(Object::Reference(field)),
        _ => form.set("Fields", vec![Object::Reference(field)]),
    }
    Ok(sig)
}

/// Appends `item` to the array at `key` of the dictionary `owner`, following
/// a reference to the array if there is one.
fn push_ref(doc: &mut Document, owner: ObjectId, key: &str, item: ObjectId) -> anyhow::Result<()> {
    let existing = doc.get_dictionary(owner)?.get(key.as_bytes()).ok().cloned();
    match existing {
        Some(Object::Reference(array_id)) => {
            if let Ok(Object::Array(items)) = doc.get_object_mut(array_id) {
                items.push(Object::Reference(item));
                return Ok(());
            }
            doc.get_dictionary_mut(owner)?.set(key, vec![Object::Reference(item)]);
        }
        Some(Object::Array(mut items)) => {
            items.push(Object::Reference(item));
            doc.get_dictionary_mut(owner)?.set(key, items);
        }
        _ => doc.get_dictionary_mut(owner)?.set(key, vec![Object::Reference(item)]),
    }
    Ok(())
}

fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    haystack.get(from..)?.windows(needle.len()).position(|w| w == needle).map(|at| at + from)
}

/// Signs `doc` and returns the sealed file.
pub(crate) fn seal(mut doc: Document, key: &SealKey, at: UnixMillis) -> anyhow::Result<Vec<u8>> {
    let sig = add_signature_field(&mut doc, at)?;
    doc.reference_table.cross_reference_type = lopdf::xref::XrefType::CrossReferenceTable;
    let mut out = Vec::new();
    doc.save_to(&mut out).context("writing the PDF")?;

    // Search from the signature object's own header, so bytes inside an
    // uploaded stream can't match first.
    let header = format!("\n{} {} obj", sig.0, sig.1);
    let start = out
        .windows(header.len())
        .rposition(|w| w == header.as_bytes())
        .context("finding the signature object")?;
    let range_key = find(&out, b"/ByteRange", start).context("finding /ByteRange")?;
    let range_open = find(&out, b"[", range_key).context("finding the ByteRange array")?;
    let range_close = find(&out, b"]", range_open).context("finding the ByteRange end")?;
    let contents_key = find(&out, b"/Contents", start).context("finding /Contents")?;
    let open = find(&out, b"<", contents_key).context("finding the Contents string")?;
    let close = find(&out, b">", open).context("finding the Contents end")?;
    if close - open - 1 != CONTENTS_SIZE * 2 {
        bail!("the Contents placeholder has an unexpected size");
    }

    let after = close + 1;
    let ranges = [0, open, after, out.len() - after];
    let text = format!("{} {} {} {}", ranges[0], ranges[1], ranges[2], ranges[3]);
    let width = range_close - range_open - 1;
    if text.len() > width {
        bail!("the PDF is too large for the ByteRange placeholder");
    }
    let padded = format!("{text:<width$}");
    out.splice(range_open + 1..range_close, padded.into_bytes());

    let mut hasher = Sha256::new();
    hasher.update(out.get(..open).context("the bytes before the signature")?);
    hasher.update(out.get(after..).context("the bytes after the signature")?);
    let digest = hasher.finalize();
    let signature = key.cms(&digest)?;
    let hex = crate::ops::hex(&signature).to_ascii_uppercase();
    if hex.len() > CONTENTS_SIZE * 2 {
        bail!("the signature is larger than its placeholder");
    }
    out.splice(open + 1..open + 1 + hex.len(), hex.into_bytes());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use cms::signed_data::SignedData;
    use p256::ecdsa::signature::Verifier as _;
    use p256::ecdsa::{Signature, VerifyingKey};

    use super::*;

    const NOW: UnixMillis = UnixMillis(1_795_000_000_000);

    /// The byte range and the CMS blob a sealed file carries.
    fn parts(out: &[u8]) -> ([usize; 4], Vec<u8>) {
        let start = out.windows(10).rposition(|w| w == b"/ByteRange").unwrap();
        let open = find(out, b"[", start).unwrap();
        let close = find(out, b"]", open).unwrap();
        let text = std::str::from_utf8(&out[open + 1..close]).unwrap();
        let numbers: Vec<usize> = text.split_whitespace().map(|n| n.parse().unwrap()).collect();
        let range = [numbers[0], numbers[1], numbers[2], numbers[3]];
        let hex = std::str::from_utf8(&out[range[1] + 1..range[2] - 1]).unwrap();
        let der: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(hex.get(i..i + 2).unwrap(), 16).unwrap())
            .collect();
        (range, der)
    }

    #[test]
    fn the_seal_signs_every_byte_but_its_own() {
        let key = SealKey::generate("bizkit.test", NOW).unwrap();
        let doc = Document::load_mem(&crate::testing::sample_pdf(&["One", "Two"])).unwrap();
        let out = seal(doc, &key, NOW).unwrap();
        let (range, der) = parts(&out);
        assert_eq!(range[0], 0);
        assert_eq!(range[2] + range[3], out.len(), "the range reaches the end");
        assert_eq!(out[range[1]], b'<');
        assert_eq!(out[range[2] - 1], b'>');

        let mut hasher = Sha256::new();
        hasher.update(&out[..range[1]]);
        hasher.update(&out[range[2]..]);
        let digest = hasher.finalize();

        // The CMS may be followed by zero padding; DER decoding stops at its end.
        let info = ContentInfo::from_der(&der[..der_len(&der)]).unwrap();
        let data: SignedData = info.content.decode_as().unwrap();
        let signer = data.signer_infos.0.as_slice().first().unwrap();
        let attrs = signer.signed_attrs.as_ref().unwrap();
        let md = attrs
            .iter()
            .find(|attr| attr.oid == ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.4"))
            .unwrap();
        let md: OctetString = md.values.as_slice().first().unwrap().decode_as().unwrap();
        assert_eq!(md.as_bytes(), digest.as_slice(), "the message digest covers the byte range");
        assert!(
            attrs
                .iter()
                .all(|attr| attr.oid != ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.5")),
            "PAdES baseline leaves out signing-time"
        );
        assert!(attrs.iter().any(|attr| attr.oid == ID_AA_SIGNING_CERTIFICATE_V2));

        let spki = &key.certificate().tbs_certificate.subject_public_key_info;
        let verifying = VerifyingKey::from_sec1_bytes(spki.subject_public_key.raw_bytes()).unwrap();
        let signature = Signature::from_der(signer.signature.as_bytes()).unwrap();
        verifying.verify(&attrs.to_der().unwrap(), &signature).unwrap();

        let reloaded = Document::load_mem(&out).unwrap();
        assert_eq!(reloaded.get_pages().len(), 2, "the pages survive");
    }

    #[test]
    fn a_stored_key_signs_the_same() {
        let key = SealKey::generate("bizkit.test", NOW).unwrap();
        let (cert, scalar) = key.to_parts().unwrap();
        let back = SealKey::from_parts(&cert, &scalar).unwrap();
        assert_eq!(back.to_parts().unwrap(), (cert, scalar));
        assert!(
            !format!("{back:?}").contains(&format!("{:?}", back.key.to_bytes())),
            "Debug hides the key"
        );
    }

    /// The length of the DER value at the start of `bytes`.
    fn der_len(bytes: &[u8]) -> usize {
        let mut reader = der::SliceReader::new(bytes).unwrap();
        let header = <der::Header as der::Decode>::decode(&mut reader).unwrap();
        let len: usize = header.length.try_into().unwrap();
        len + usize::try_from(u32::from(header.encoded_len().unwrap())).unwrap()
    }
}
