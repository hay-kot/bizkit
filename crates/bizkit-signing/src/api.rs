//! The signing module's API types.

use bizkit_core::ids::ProjectId;
use bizkit_core::time::UnixMillis;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::fields::Field;

bizkit_core::id_type!(SigningRequestId, "The ID of a signature request.");
bizkit_core::id_type!(SignerId, "The ID of one signer of a signature request.");

/// Where a request is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RequestStatus {
    /// Not sent yet; nobody has a link.
    Draft,
    /// Signers have their links.
    Sent,
    /// Everyone signed, and the sealed PDF exists.
    Completed,
    /// Cancelled; the links no longer work.
    Voided,
    /// Everyone signed, but sealing the PDF failed; `failure` says why.
    /// `signing.requests.retry` tries again.
    Failed,
}

impl RequestStatus {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Sent => "sent",
            Self::Completed => "completed",
            Self::Voided => "voided",
            Self::Failed => "failed",
        }
    }

    pub(crate) fn parse(text: &str) -> anyhow::Result<Self> {
        Ok(match text {
            "failed" => Self::Failed,
            "draft" => Self::Draft,
            "sent" => Self::Sent,
            "completed" => Self::Completed,
            "voided" => Self::Voided,
            other => anyhow::bail!("unknown request status {other}"),
        })
    }
}

/// One person asked to sign.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Signer {
    /// The signer ID.
    pub id: SignerId,
    /// Their name, as the sender gave it.
    pub name: String,
    /// Where their link goes.
    pub email: String,
    /// Their group, 1 to 10. The lowest group with someone yet to sign
    /// gets its links; equal numbers sign in parallel.
    pub signing_order: u8,
    /// True once they signed.
    pub signed: bool,
    /// When they signed.
    pub signed_at: Option<UnixMillis>,
}

/// A PDF sent for signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SigningRequest {
    /// The request ID.
    pub id: SigningRequestId,
    /// The project it belongs to.
    pub project_id: ProjectId,
    /// What signers see as the subject.
    pub title: String,
    /// A note for signers, shown on the signing page.
    pub message: String,
    /// Where it is.
    pub status: RequestStatus,
    /// Why sealing failed, while `status` is `failed`.
    pub failure: Option<String>,
    /// Who sent it, as `Name <email>` or the API key that did.
    pub sender: String,
    /// The uploaded file's name.
    pub file_name: String,
    /// SHA-256 of the uploaded PDF, hex.
    pub original_sha256: String,
    /// SHA-256 of the sealed PDF, hex, once completed.
    pub signed_sha256: Option<String>,
    /// Pages in the uploaded PDF. Empty only on requests made before
    /// fields existed, until `signing.requests.fields.put` reads the file.
    pub page_count: Option<u32>,
    /// In the order given.
    pub signers: Vec<Signer>,
    /// When it was created.
    pub created_at: UnixMillis,
    /// When the links went out.
    pub sent_at: Option<UnixMillis>,
    /// When the last signer signed and the PDF was sealed.
    pub completed_at: Option<UnixMillis>,
    /// When it was voided.
    pub voided_at: Option<UnixMillis>,
}

/// Requests of a project.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SigningRequestList {
    /// Newest first, at most 200.
    pub items: Vec<SigningRequest>,
}

/// What happened to a request, for the audit trail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// The request was created.
    Created,
    /// The links went out.
    Sent,
    /// A signer opened the signing page. Some mail scanners run scripts,
    /// so this means the page loaded, not that a person read it.
    Viewed,
    /// A signer agreed and signed.
    Signed,
    /// The PDF was sealed.
    Completed,
    /// The request was voided.
    Voided,
    /// Sealing the PDF failed.
    Failed,
}

impl EventKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Sent => "sent",
            Self::Viewed => "viewed",
            Self::Signed => "signed",
            Self::Completed => "completed",
            Self::Voided => "voided",
            Self::Failed => "failed",
        }
    }

    pub(crate) fn parse(text: &str) -> anyhow::Result<Self> {
        Ok(match text {
            "failed" => Self::Failed,
            "created" => Self::Created,
            "sent" => Self::Sent,
            "viewed" => Self::Viewed,
            "signed" => Self::Signed,
            "completed" => Self::Completed,
            "voided" => Self::Voided,
            other => anyhow::bail!("unknown signing event {other}"),
        })
    }
}

/// One audit trail entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AuditEvent {
    /// What happened.
    pub kind: EventKind,
    /// When.
    pub at: UnixMillis,
    /// The signer it concerns, if any.
    pub signer_id: Option<SignerId>,
    /// The client address, for signer actions.
    pub ip: Option<String>,
    /// The browser, for signer actions.
    pub user_agent: Option<String>,
    /// More about it, such as the consent text a signer agreed to.
    pub detail: Option<String>,
}

/// A request with its fields and audit trail.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SigningRequestDetail {
    /// The request.
    pub request: SigningRequest,
    /// The fields on its pages, in the order given.
    pub fields: Vec<Field>,
    /// Oldest first.
    pub events: Vec<AuditEvent>,
}

/// Which PDF of a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DocumentVersion {
    /// The PDF as uploaded.
    #[default]
    Original,
    /// The sealed PDF with signatures and audit trail, once completed.
    Signed,
}

impl DocumentVersion {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Original => "original",
            Self::Signed => "signed",
        }
    }
}

/// A PDF, base64-encoded.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Document {
    /// A file name to save it as.
    pub file_name: String,
    /// SHA-256 of the bytes, hex.
    pub sha256: String,
    /// The PDF, standard base64.
    pub pdf_base64: String,
}
