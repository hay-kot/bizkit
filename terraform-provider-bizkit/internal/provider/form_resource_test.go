package provider

import (
	"fmt"
	"testing"

	"github.com/hashicorp/terraform-plugin-testing/helper/resource"
)

func formConfig(slug, extra string) string {
	return fmt.Sprintf(`
resource "bizkit_project" "test" {
  slug = %q
  name = "Forms"
}

resource "bizkit_form" "test" {
  project_id = bizkit_project.test.id
  name       = "Contact"
  %s
}
`, slug, extra)
}

func TestAccForm(t *testing.T) {
	slug := "tfacc-" + uniq()
	resource.Test(t, resource.TestCase{
		PreCheck:                 func() { testAccPreCheck(t) },
		ProtoV6ProviderFactories: protoV6ProviderFactories,
		Steps: []resource.TestStep{
			{
				Config: formConfig(slug, ""),
				Check: resource.ComposeAggregateTestCheckFunc(
					resource.TestCheckResourceAttr("bizkit_form.test", "enabled", "true"),
					resource.TestCheckResourceAttr("bizkit_form.test", "bot_check", "honeypot"),
					resource.TestCheckResourceAttr("bizkit_form.test", "min_fill_ms", "2000"),
					resource.TestCheckResourceAttr("bizkit_form.test", "fields.#", "0"),
					resource.TestCheckResourceAttrSet("bizkit_form.test", "endpoint"),
				),
			},
			{
				Config: formConfig(slug, `
  enabled       = false
  bot_check     = "challenge"
  redirect_url  = "https://example.com/thanks"
  notify_emails = ["team@example.com"]
  fields = [
    { name = "email", kind = "email", required = true },
    { name = "plan", kind = "choice", label = "Plan", options = ["free", "pro"] },
  ]`),
				Check: resource.ComposeAggregateTestCheckFunc(
					resource.TestCheckResourceAttr("bizkit_form.test", "enabled", "false"),
					resource.TestCheckResourceAttr("bizkit_form.test", "redirect_url", "https://example.com/thanks"),
					resource.TestCheckResourceAttr("bizkit_form.test", "fields.#", "2"),
					resource.TestCheckResourceAttr("bizkit_form.test", "fields.1.options.1", "pro"),
				),
			},
			{ResourceName: "bizkit_form.test", ImportState: true, ImportStateVerify: true},
			{
				// Removing redirect_url clears it on the server.
				Config: formConfig(slug, `enabled = false`),
				Check:  resource.TestCheckNoResourceAttr("bizkit_form.test", "redirect_url"),
			},
		},
	})
}
