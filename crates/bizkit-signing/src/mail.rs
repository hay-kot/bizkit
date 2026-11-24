//! The mails signers get.

use bizkit_core::deps::Mail;
use url::Url;

use crate::store::{RequestRow, SignerRow};

/// The mail with a signer's link.
pub(crate) fn sign_request(request: &RequestRow, signer: &SignerRow, link: &Url) -> Mail {
    let note =
        if request.message.is_empty() { String::new() } else { format!("\n\n{}", request.message) };
    Mail {
        to: signer.email.clone(),
        subject: format!("Please sign: {}", request.title),
        text: format!(
            "Hi {name},\n\nYou're asked to sign \"{title}\".{note}\n\nRead and sign it here:\n\n{link}\n\n\
             This link is yours alone; don't forward it.",
            name = signer.name,
            title = request.title,
        ),
        html: None,
        unsubscribe: None,
        id: None,
    }
}

/// The mail when everyone has signed.
pub(crate) fn completed(request: &RequestRow, to: &str, link: &Url) -> Mail {
    Mail {
        to: to.to_owned(),
        subject: format!("Signed: {}", request.title),
        text: format!(
            "Everyone has signed \"{title}\".\n\nDownload the signed PDF, with the signatures and the \
             audit trail:\n\n{link}",
            title = request.title,
        ),
        html: None,
        unsubscribe: None,
        id: None,
    }
}
