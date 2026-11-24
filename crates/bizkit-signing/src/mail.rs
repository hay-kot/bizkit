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
        subject: format!("Please sign: {}", one_line(&request.title)),
        text: format!(
            "Hi {name},\n\nYou're asked to sign \"{title}\".{note}\n\nRead and sign it here:\n\n{link}\n\n\
             This link is yours alone; don't forward it.",
            name = signer.name,
            title = request.title,
        ),
        html: None,
        unsubscribe: None,
        id: None,
        workspace_id: Some(request.workspace_id),
        reply_to: None,
    }
}

/// The mail when everyone has signed.
pub(crate) fn completed(request: &RequestRow, to: &str, link: &Url, sha256: &str) -> Mail {
    Mail {
        to: to.to_owned(),
        subject: format!("Signed: {}", one_line(&request.title)),
        text: format!(
            "Everyone has signed \"{title}\".\n\nDownload the signed PDF, with the signatures and the \
             audit trail:\n\n{link}\n\nIts SHA-256 is {sha256}. Keep this mail: the hash shows whether a \
             copy is the one bizkit sealed.",
            title = request.title,
        ),
        html: None,
        unsubscribe: None,
        id: None,
        workspace_id: Some(request.workspace_id),
        reply_to: None,
    }
}

/// `text` without line breaks, for a subject line.
fn one_line(text: &str) -> String {
    text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}
