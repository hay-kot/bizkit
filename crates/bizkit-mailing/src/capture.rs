//! Adds addresses that forms collect to the lists linked to the form.

use anyhow::Context as _;
use bizkit_core::auth::{is_plausible_email, normalize_email};
use bizkit_core::contacts::ContactCaptured;
use bizkit_core::jobs::{JobCtx, Lane, Subscriber};

use crate::api::Attributes;
use crate::contacts::{self, NewContact, Source};
use crate::send::MAILING;
use crate::store;

/// The hook target kind forms declares; a capture source of this kind is a
/// form.
pub(crate) const FORM_KIND: &str = "forms.form";

/// Subscribes [`ContactCaptured`] addresses to linked lists. Every capture
/// needs confirming, whatever the list says.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CaptureSubscriber;

impl Subscriber<ContactCaptured> for CaptureSubscriber {
    const NAME: &'static str = "mailing.capture";
    const LANE: Lane = MAILING;

    async fn handle(&self, cx: JobCtx, event: ContactCaptured) -> anyhow::Result<()> {
        if event.source.kind != FORM_KIND {
            return Ok(());
        }
        let email = normalize_email(&event.email);
        if !is_plausible_email(&email) {
            return Ok(());
        }
        let platform = &cx.platform;
        // A capture while mailing is off is new intake, like a public route,
        // so it adds nobody and mails nobody.
        if !platform.feature_enabled(event.workspace_id, "mailing").await? {
            return Ok(());
        }
        let mut tx = platform.db().write().await?;
        let lists =
            store::lists_for_form(&mut *tx, event.workspace_id, event.project_id, &event.source.id)
                .await
                .context("finding the form's lists")?;
        for list in &lists {
            let input = NewContact {
                email: email.clone(),
                name: event.name.clone(),
                tags: Vec::new(),
                attributes: Attributes::default(),
                source: Source::Form(event.source.id.clone()),
            };
            contacts::subscribe(platform, &mut tx, list, input).await?;
        }
        tx.commit().await
    }
}
