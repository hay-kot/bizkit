data "bizkit_workspace" "current" {}

output "workspace_slug" {
  value = data.bizkit_workspace.current.slug
}
