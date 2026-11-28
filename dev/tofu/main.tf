# The dev stack's demo workspace configuration, applied by
# scripts/seed-dev.sh through the provider in terraform-provider-bizkit.
# `bizkit dev seed-demo` runs after it and adds sample data (submissions,
# requests, contacts, events) to these rows by name, so keep the names in
# step with the module seeds.

terraform {
  required_providers {
    bizkit = {
      source = "hay-kot/bizkit"
    }
  }
}

# Endpoint, key and workspace come from BIZKIT_* in the stack's demo.env.
provider "bizkit" {}

locals {
  projects = {
    website = "Website"
    docs    = "Docs"
    # The dev stack reports its own usage here (self_usage in dev/bizkit.yaml).
    bizkit = "bizkit usage"
  }
  demo_inbox = ["demo@bizkit.test"]
}

resource "bizkit_project" "this" {
  for_each = local.projects
  slug     = each.key
  name     = each.value
}

resource "bizkit_form" "contact" {
  project_id    = bizkit_project.this["website"].id
  name          = "Contact"
  notify_emails = local.demo_inbox
  fields = [
    { name = "name", label = "Name", kind = "text", required = true },
    { name = "email", label = "Email", kind = "email", required = true },
    { name = "message", label = "Message", kind = "text", required = true },
  ]
}

resource "bizkit_form" "newsletter" {
  project_id    = bizkit_project.this["website"].id
  name          = "Newsletter"
  bot_check     = "challenge"
  notify_emails = local.demo_inbox
}

resource "bizkit_webhook_endpoint" "stripe" {
  project_id = bizkit_project.this["website"].id
  name       = "Stripe"
}

resource "bizkit_webhook_endpoint" "github" {
  project_id = bizkit_project.this["website"].id
  name       = "GitHub"
}

resource "bizkit_mailing_list" "newsletter" {
  project_id  = bizkit_project.this["website"].id
  name        = "Newsletter"
  description = "Product news, about once a month."
  footer      = "Tallyho, 1 Main Street, Springfield"
  archive     = true
  form_ids    = [bizkit_form.contact.id]
}

resource "bizkit_mailing_list" "updates" {
  project_id    = bizkit_project.this["website"].id
  name          = "Product updates"
  description   = "Release notes, when a version ships."
  footer        = "Tallyho, 1 Main Street, Springfield"
  double_opt_in = false
}
