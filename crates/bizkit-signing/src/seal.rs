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
use der::asn1::{GeneralizedTime, ObjectIdentifier, OctetString, SetOfVec};
use der::{Any, Decode as _, Encode as _, Sequence, ValueOrd};
use lopdf::{Dictionary, Document, Object, ObjectId, StringFormat, dictionary};
use p256::ecdsa::signature::Signer as _;
use p256::ecdsa::{DerSignature, SigningKey};
use sha2::{Digest as _, Sha256};
use x509_cert::Certificate;
use x509_cert::attr::Attribute;
use x509_cert::builder::profile::BuilderProfile;
use x509_cert::builder::{Builder as _, CertificateBuilder};
use x509_cert::certificate::TbsCertificate;
use x509_cert::ext::pkix::{BasicConstraints, KeyUsage, KeyUsages, SubjectKeyIdentifier};
use x509_cert::ext::{Extension, ToExtension as _};
use x509_cert::name::Name;
use x509_cert::serial_number::SerialNumber;
use x509_cert::spki::{
    AlgorithmIdentifierOwned, SubjectPublicKeyInfoOwned, SubjectPublicKeyInfoRef,
};
use x509_cert::time::{Time, Validity};

/// Room for the CMS signature, in bytes; written as twice as many hex digits.
const CONTENTS_SIZE: usize = 8192;
/// A `/ByteRange` wide enough for any file this module writes, patched in place.
const BYTE_RANGE_PLACEHOLDER: [i64; 4] = [0, 9_999_999_999, 9_999_999_999, 9_999_999_999];
const CERT_YEARS: u64 = 20;

const ID_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.1");
const ID_SIGNED_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.2");
const ID_CONTENT_TYPE: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.3");
const ID_MESSAGE_DIGEST: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.4");
const ID_SHA_256: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.1");
const ID_ECDSA_WITH_SHA_256: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.2");
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

/// CMS `ContentInfo` (RFC 5652) holding a `SignedData`.
#[derive(Debug, Sequence)]
struct ContentInfo {
    content_type: ObjectIdentifier,
    #[asn1(context_specific = "0", tag_mode = "EXPLICIT")]
    content: Any,
}

/// CMS `EncapsulatedContentInfo` with no content: the signature is detached.
#[derive(Debug, Clone, Sequence)]
struct EncapsulatedContentInfo {
    econtent_type: ObjectIdentifier,
}

/// CMS `IssuerAndSerialNumber`.
#[derive(Debug, Clone, Sequence, ValueOrd)]
struct IssuerAndSerialNumber {
    issuer: Name,
    serial_number: SerialNumber,
}

/// CMS `SignerInfo`, version 1 (the signer is named by issuer and serial).
#[derive(Debug, Clone, Sequence, ValueOrd)]
struct SignerInfo {
    version: u8,
    sid: IssuerAndSerialNumber,
    digest_algorithm: AlgorithmIdentifierOwned,
    #[asn1(context_specific = "0", tag_mode = "IMPLICIT")]
    signed_attrs: SetOfVec<Attribute>,
    signature_algorithm: AlgorithmIdentifierOwned,
    signature: OctetString,
}

/// CMS `SignedData`, version 1.
#[derive(Debug, Sequence)]
struct SignedData {
    version: u8,
    digest_algorithms: SetOfVec<AlgorithmIdentifierOwned>,
    encap_content_info: EncapsulatedContentInfo,
    #[asn1(context_specific = "0", tag_mode = "IMPLICIT")]
    certificates: SetOfVec<Certificate>,
    signer_infos: SetOfVec<SignerInfo>,
}

/// A self-signed end-entity certificate that may sign documents: not a CA,
/// key usage digitalSignature and nonRepudiation, which signature checkers
/// look for.
struct SealProfile {
    subject: Name,
}

impl BuilderProfile for SealProfile {
    fn get_issuer(&self, subject: &Name) -> Name {
        subject.clone()
    }

    fn get_subject(&self) -> Name {
        self.subject.clone()
    }

    fn build_extensions(
        &self,
        spk: SubjectPublicKeyInfoRef<'_>,
        _issuer_spk: SubjectPublicKeyInfoRef<'_>,
        tbs: &TbsCertificate,
    ) -> x509_cert::builder::Result<Vec<Extension>> {
        let mut extensions = Vec::new();
        let constraints = BasicConstraints { ca: false, path_len_constraint: None };
        extensions.push(constraints.to_extension(tbs.subject(), &extensions)?);
        let usage = KeyUsage(KeyUsages::DigitalSignature | KeyUsages::NonRepudiation);
        extensions.push(usage.to_extension(tbs.subject(), &extensions)?);
        let ski = SubjectKeyIdentifier::try_from(spk)?;
        extensions.push(ski.to_extension(tbs.subject(), &extensions)?);
        Ok(extensions)
    }
}

fn generalized(at: UnixMillis) -> anyhow::Result<GeneralizedTime> {
    let secs = u64::try_from(at.0 / 1000).context("a time before 1970")?;
    GeneralizedTime::from_unix_duration(Duration::from_secs(secs)).context("encoding a time")
}

fn attribute(oid: ObjectIdentifier, value: Any) -> anyhow::Result<Attribute> {
    let mut values = SetOfVec::new();
    values.insert(value).context("an attribute value")?;
    Ok(Attribute { oid, values })
}

impl SealKey {
    /// A new key and a self-signed certificate for `common_name`, valid for
    /// 20 years from `now`.
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
        let clean = common_name.replace([',', '=', '+', '"', '\\', '<', '>', ';'], " ");
        let subject =
            Name::from_str(&format!("CN={clean},O=bizkit")).context("the certificate name")?;
        let validity = Validity::new(
            Time::GeneralTime(generalized(now)?),
            Time::GeneralTime(generalized(now.plus(Duration::from_hours(CERT_YEARS * 365 * 24)))?),
        );
        let spki =
            SubjectPublicKeyInfoOwned::from_key(key.verifying_key()).context("the public key")?;
        let builder = CertificateBuilder::new(SealProfile { subject }, serial, validity, spki)
            .context("building the seal certificate")?;
        let certificate =
            builder.build::<_, DerSignature>(&key).context("signing the seal certificate")?;
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

    /// A detached CMS `SignedData` over `digest` (SHA-256 of the signed bytes),
    /// with the signed attributes `PAdES` B-B asks for and no signing-time.
    fn cms(&self, digest: &[u8]) -> anyhow::Result<Vec<u8>> {
        let sha256 = AlgorithmIdentifierOwned { oid: ID_SHA_256, parameters: None };
        let ecdsa = AlgorithmIdentifierOwned { oid: ID_ECDSA_WITH_SHA_256, parameters: None };
        let cert_der = self.certificate.to_der().context("encoding the certificate")?;
        let ess = SigningCertificateV2 {
            certs: vec![EssCertIdV2 {
                cert_hash: OctetString::new(Sha256::digest(&cert_der).to_vec())
                    .context("a certificate hash")?,
            }],
        };
        let mut signed_attrs = SetOfVec::new();
        for attr in [
            attribute(ID_CONTENT_TYPE, Any::encode_from(&ID_DATA)?)?,
            attribute(ID_MESSAGE_DIGEST, Any::encode_from(&OctetString::new(digest.to_vec())?)?)?,
            attribute(ID_AA_SIGNING_CERTIFICATE_V2, Any::encode_from(&ess)?)?,
        ] {
            signed_attrs.insert(attr).context("a signed attribute")?;
        }
        // The signature covers the attributes encoded as a SET (RFC 5652 5.4).
        let to_sign = signed_attrs.to_der().context("encoding the signed attributes")?;
        let signature: DerSignature = self.key.sign(&to_sign);
        let tbs = &self.certificate.tbs_certificate();
        let signer = SignerInfo {
            version: 1,
            sid: IssuerAndSerialNumber {
                issuer: tbs.issuer().clone(),
                serial_number: tbs.serial_number().clone(),
            },
            digest_algorithm: sha256.clone(),
            signed_attrs,
            signature_algorithm: ecdsa,
            signature: OctetString::new(signature.to_bytes().to_vec()).context("the signature")?,
        };
        let mut digest_algorithms = SetOfVec::new();
        digest_algorithms.insert(sha256).context("the digest algorithm")?;
        let mut certificates = SetOfVec::new();
        certificates.insert(self.certificate.clone()).context("the certificate")?;
        let mut signer_infos = SetOfVec::new();
        signer_infos.insert(signer).context("the signer info")?;
        let signed_data = SignedData {
            version: 1,
            digest_algorithms,
            encap_content_info: EncapsulatedContentInfo { econtent_type: ID_DATA },
            certificates,
            signer_infos,
        };
        let info = ContentInfo {
            content_type: ID_SIGNED_DATA,
            content: Any::encode_from(&signed_data).context("encoding SignedData")?,
        };
        info.to_der().context("encoding ContentInfo")
    }
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
        assert_eq!(info.content_type, ID_SIGNED_DATA);
        let data: SignedData = info.content.decode_as().unwrap();
        let signer = data.signer_infos.as_slice().first().unwrap();
        let attrs = &signer.signed_attrs;
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

        let spki = key.certificate().tbs_certificate().subject_public_key_info();
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
        let len: usize = header.length().try_into().unwrap();
        len + usize::try_from(u32::from(header.encoded_len().unwrap())).unwrap()
    }
}
