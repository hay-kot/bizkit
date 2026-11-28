terraform {
  required_providers {
    bizkit = {
      source = "hay-kot/bizkit"
    }
  }
}

# Or set BIZKIT_ENDPOINT, BIZKIT_API_KEY and BIZKIT_WORKSPACE_ID.
provider "bizkit" {
  endpoint     = "https://bizkit.example.com"
  api_key      = var.bizkit_api_key
  workspace_id = "01a20560-0a68-7703-8e0c-5c2b8d9e8f01"
}

variable "bizkit_api_key" {
  type      = string
  sensitive = true
}
