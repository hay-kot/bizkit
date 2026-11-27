package provider

import (
	"fmt"
	"testing"

	"github.com/hashicorp/terraform-plugin-testing/helper/resource"
)

func projectConfig(slug, name string) string {
	return fmt.Sprintf(`
resource "bizkit_project" "test" {
  slug = %q
  name = %q
}

data "bizkit_project" "by_slug" {
  slug = bizkit_project.test.slug
}

data "bizkit_workspace" "current" {}
`, slug, name)
}

func TestAccProject(t *testing.T) {
	slug := "tfacc-" + uniq()
	resource.Test(t, resource.TestCase{
		PreCheck:                 func() { testAccPreCheck(t) },
		ProtoV6ProviderFactories: protoV6ProviderFactories,
		Steps: []resource.TestStep{
			{
				Config: projectConfig(slug, "Acceptance"),
				Check: resource.ComposeAggregateTestCheckFunc(
					resource.TestCheckResourceAttr("bizkit_project.test", "slug", slug),
					resource.TestCheckResourceAttrSet("bizkit_project.test", "id"),
					resource.TestCheckResourceAttrPair("data.bizkit_project.by_slug", "id", "bizkit_project.test", "id"),
					resource.TestCheckResourceAttrSet("data.bizkit_workspace.current", "slug"),
				),
			},
			{
				Config: projectConfig(slug, "Acceptance renamed"),
				Check:  resource.TestCheckResourceAttr("bizkit_project.test", "name", "Acceptance renamed"),
			},
			{ResourceName: "bizkit_project.test", ImportState: true, ImportStateVerify: true},
		},
	})
}
