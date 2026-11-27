// Command terraform-provider-bizkit serves the bizkit provider to Terraform
// and OpenTofu.
package main

import (
	"context"
	"flag"
	"log"

	"github.com/hashicorp/terraform-plugin-framework/providerserver"

	"github.com/hay-kot/bizkit/terraform-provider-bizkit/internal/provider"
)

//go:generate go tool tfplugindocs generate --provider-name bizkit

// version is set by the release build.
var version = "dev"

func main() {
	var debug bool
	flag.BoolVar(&debug, "debug", false, "run with support for debuggers such as delve")
	flag.Parse()

	err := providerserver.Serve(context.Background(), provider.New(version), providerserver.ServeOpts{
		// OpenTofu is what the repo tests and seeds with; the address only
		// matters for -debug reattach.
		Address: "registry.opentofu.org/hay-kot/bizkit",
		Debug:   debug,
	})
	if err != nil {
		log.Fatal(err)
	}
}
