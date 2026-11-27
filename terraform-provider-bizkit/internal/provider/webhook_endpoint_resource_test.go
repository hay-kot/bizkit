package provider

import (
	"fmt"
	"testing"

	"github.com/hashicorp/terraform-plugin-testing/helper/resource"
)

func endpointConfig(slug, extra string) string {
	return fmt.Sprintf(`
resource "bizkit_project" "test" {
  slug = %q
  name = "Webhooks"
}

resource "bizkit_webhook_endpoint" "test" {
  project_id = bizkit_project.test.id
  name       = "Stripe"
  %s
}
`, slug, extra)
}

func TestAccWebhookEndpoint(t *testing.T) {
	slug := "tfacc-" + uniq()
	resource.Test(t, resource.TestCase{
		PreCheck:                 func() { testAccPreCheck(t) },
		ProtoV6ProviderFactories: protoV6ProviderFactories,
		Steps: []resource.TestStep{
			{
				Config: endpointConfig(slug, `enabled = false`),
				Check: resource.ComposeAggregateTestCheckFunc(
					resource.TestCheckResourceAttr("bizkit_webhook_endpoint.test", "enabled", "false"),
					resource.TestCheckResourceAttrSet("bizkit_webhook_endpoint.test", "url"),
				),
			},
			{
				Config: endpointConfig(slug, `forward_url = "https://example.com/hook"`),
				Check: resource.ComposeAggregateTestCheckFunc(
					resource.TestCheckResourceAttr("bizkit_webhook_endpoint.test", "enabled", "true"),
					resource.TestCheckResourceAttr("bizkit_webhook_endpoint.test", "forward_url", "https://example.com/hook"),
				),
			},
			{ResourceName: "bizkit_webhook_endpoint.test", ImportState: true, ImportStateVerify: true},
			{
				Config: endpointConfig(slug, ""),
				Check:  resource.TestCheckNoResourceAttr("bizkit_webhook_endpoint.test", "forward_url"),
			},
		},
	})
}
