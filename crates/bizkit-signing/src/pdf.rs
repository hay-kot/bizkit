//! Reading uploaded PDFs, and the final PDF's signatures and audit pages.

use std::time::Duration;

/// How long parsing an upload may take. Parsing runs off the async
/// runtime; a hostile file that takes longer is refused.
const PARSE_TIMEOUT: Duration = Duration::from_secs(5);

/// Checks that `bytes` is a PDF bizkit can add pages to. The error is a
/// message for the `pdf_base64` field.
pub(crate) async fn check(bytes: Vec<u8>) -> Result<(), &'static str> {
    if !bytes.starts_with(b"%PDF-") {
        return Err("must be a PDF");
    }
    let parsed = tokio::task::spawn_blocking(move || {
        let doc = lopdf::Document::load_mem(&bytes).map_err(|_| "must be a PDF that opens")?;
        if doc.is_encrypted() {
            return Err("must not be encrypted or password-protected");
        }
        if doc.get_pages().is_empty() {
            return Err("must have at least one page");
        }
        Ok(())
    });
    match tokio::time::timeout(PARSE_TIMEOUT, parsed).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) | Err(_) => Err("must be a PDF that opens"),
    }
}

/// The sealed PDF's file name: `contract.pdf` becomes `contract-signed.pdf`.
pub(crate) fn signed_file_name(original: &str) -> String {
    let stem =
        original.strip_suffix(".pdf").or_else(|| original.strip_suffix(".PDF")).unwrap_or(original);
    format!("{stem}-signed.pdf")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_names_keep_the_stem() {
        assert_eq!(signed_file_name("contract.pdf"), "contract-signed.pdf");
        assert_eq!(signed_file_name("NDA.PDF"), "NDA-signed.pdf");
        assert_eq!(signed_file_name("plain"), "plain-signed.pdf");
    }
}
