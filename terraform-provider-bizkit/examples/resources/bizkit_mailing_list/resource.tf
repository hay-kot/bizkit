resource "bizkit_mailing_list" "newsletter" {
  project_id  = bizkit_project.website.id
  name        = "Newsletter"
  description = "Product news, about once a month."
  footer      = "Example Inc, 1 Main Street, Springfield"
  archive     = true
  form_ids    = [bizkit_form.contact.id]
}
