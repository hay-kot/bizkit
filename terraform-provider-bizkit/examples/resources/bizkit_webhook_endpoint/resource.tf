resource "bizkit_webhook_endpoint" "stripe" {
  project_id  = bizkit_project.website.id
  name        = "Stripe"
  forward_url = "https://api.example.com/hooks/stripe"
}
