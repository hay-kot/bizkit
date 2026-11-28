resource "bizkit_form" "contact" {
  project_id    = bizkit_project.website.id
  name          = "Contact"
  notify_emails = ["team@example.com"]
  redirect_url  = "https://example.com/thanks"
  fields = [
    { name = "email", kind = "email", required = true },
    { name = "message", label = "Message", kind = "text", required = true },
    { name = "topic", kind = "choice", options = ["sales", "support"] },
  ]
}
