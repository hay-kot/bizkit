package provider

import (
	"context"
	"testing"

	"github.com/hashicorp/terraform-plugin-framework/path"
	"github.com/hashicorp/terraform-plugin-framework/schema/validator"
	"github.com/hashicorp/terraform-plugin-framework/types"
)

func validate(v validator.String, value string) bool {
	resp := &validator.StringResponse{}
	v.ValidateString(context.Background(), validator.StringRequest{Path: path.Root("x"), ConfigValue: types.StringValue(value)}, resp)
	return !resp.Diagnostics.HasError()
}

func TestTrimmed(t *testing.T) {
	for value, ok := range map[string]bool{"Contact": true, "Two words": true, "Contact ": false, "\tx": false, "x\n": false} {
		if got := validate(trimmed(), value); got != ok {
			t.Errorf("trimmed(%q) = %v, want %v", value, got, ok)
		}
	}
}

func TestCanonicalURL(t *testing.T) {
	for value, ok := range map[string]bool{
		"https://example.com/":         true,
		"https://example.com/thanks":   true,
		"http://example.com:8080/hook": true,
		"https://example.com":          false,
		"https://Example.com/":         false,
		"https://example.com:443/x":    false,
		"ftp://example.com/":           false,
		"example.com/":                 false,
	} {
		if got := validate(canonicalURL(), value); got != ok {
			t.Errorf("canonicalURL(%q) = %v, want %v", value, got, ok)
		}
	}
}
