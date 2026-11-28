package provider

import (
	"fmt"
	"os"
	"testing"
	"time"

	"github.com/hashicorp/terraform-plugin-framework/providerserver"
	"github.com/hashicorp/terraform-plugin-go/tfprotov6"
)

// The acceptance tests run against a live bizkit with TF_ACC=1 and
// BIZKIT_ENDPOINT, BIZKIT_API_KEY and BIZKIT_WORKSPACE_ID set;
// scripts/test-tfacc.sh seeds a workspace and sets them.
var protoV6ProviderFactories = map[string]func() (tfprotov6.ProviderServer, error){
	"bizkit": providerserver.NewProtocol6WithError(New("test")()),
}

func testAccPreCheck(t *testing.T) {
	t.Helper()
	for _, env := range []string{"BIZKIT_ENDPOINT", "BIZKIT_API_KEY", "BIZKIT_WORKSPACE_ID"} {
		if os.Getenv(env) == "" {
			t.Fatalf("%s must be set for acceptance tests; run scripts/test-tfacc.sh", env)
		}
	}
}

// uniq gives a slug-safe suffix so test runs never collide.
func uniq() string { return fmt.Sprintf("%x", time.Now().UnixNano())[6:] }
