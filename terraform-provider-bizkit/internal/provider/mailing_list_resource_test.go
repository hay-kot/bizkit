package provider

import (
	"fmt"
	"testing"

	"github.com/hashicorp/terraform-plugin-testing/helper/resource"
)

func listConfig(slug, extra string) string {
	return fmt.Sprintf(`
resource "bizkit_project" "test" {
  slug = %q
  name = "Mailing"
}

resource "bizkit_form" "signup" {
  project_id = bizkit_project.test.id
  name       = "Signup"
}

resource "bizkit_mailing_list" "test" {
  project_id = bizkit_project.test.id
  name       = "Newsletter"
  %s
}
`, slug, extra)
}

func TestAccMailingList(t *testing.T) {
	slug := "tfacc-" + uniq()
	resource.Test(t, resource.TestCase{
		PreCheck:                 func() { testAccPreCheck(t) },
		ProtoV6ProviderFactories: protoV6ProviderFactories,
		Steps: []resource.TestStep{
			{
				Config: listConfig(slug, ""),
				Check: resource.ComposeAggregateTestCheckFunc(
					resource.TestCheckResourceAttr("bizkit_mailing_list.test", "double_opt_in", "true"),
					resource.TestCheckResourceAttr("bizkit_mailing_list.test", "archive", "false"),
					resource.TestCheckResourceAttrSet("bizkit_mailing_list.test", "archive_url"),
				),
			},
			{
				Config: listConfig(slug, `
  description   = "Product news"
  footer        = "1 Main Street"
  double_opt_in = false
  archive       = true
  form_ids      = [bizkit_form.signup.id]`),
				Check: resource.ComposeAggregateTestCheckFunc(
					resource.TestCheckResourceAttr("bizkit_mailing_list.test", "description", "Product news"),
					resource.TestCheckResourceAttr("bizkit_mailing_list.test", "form_ids.#", "1"),
					resource.TestCheckResourceAttrPair("bizkit_mailing_list.test", "form_ids.0", "bizkit_form.signup", "id"),
				),
			},
			{ResourceName: "bizkit_mailing_list.test", ImportState: true, ImportStateVerify: true},
			{
				Config: listConfig(slug, ""),
				Check: resource.ComposeAggregateTestCheckFunc(
					resource.TestCheckNoResourceAttr("bizkit_mailing_list.test", "description"),
					resource.TestCheckNoResourceAttr("bizkit_mailing_list.test", "footer"),
					resource.TestCheckResourceAttr("bizkit_mailing_list.test", "form_ids.#", "0"),
				),
			},
		},
	})
}
