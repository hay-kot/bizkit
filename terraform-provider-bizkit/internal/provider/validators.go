package provider

import (
	"context"
	"net/url"
	"regexp"
	"strings"

	"github.com/hashicorp/terraform-plugin-framework/schema/validator"
	"github.com/hashicorp/terraform-plugin-framework/types"
)

// lowercaseElements rejects set elements with upper case or surrounding
// space, which the API would rewrite and so show as a diff on every plan.
func lowercaseElements() validator.Set { return lowercaseValidator{} }

type lowercaseValidator struct{}

func (lowercaseValidator) Description(context.Context) string {
	return "every value is lower case, with no surrounding space"
}

func (v lowercaseValidator) MarkdownDescription(ctx context.Context) string {
	return v.Description(ctx)
}

func (lowercaseValidator) ValidateSet(ctx context.Context, req validator.SetRequest, resp *validator.SetResponse) {
	if req.ConfigValue.IsNull() || req.ConfigValue.IsUnknown() {
		return
	}
	var values []types.String
	resp.Diagnostics.Append(req.ConfigValue.ElementsAs(ctx, &values, false)...)
	for _, v := range values {
		if v.IsUnknown() || v.IsNull() {
			continue
		}
		s := v.ValueString()
		if s != strings.ToLower(strings.TrimSpace(s)) {
			resp.Diagnostics.AddAttributeError(req.Path, "Value isn't normalized",
				"bizkit stores "+s+" as "+strings.ToLower(strings.TrimSpace(s))+"; write it that way.")
		}
	}
}

// nonBlank matches text with at least one non-space character. The API
// stores blank text as null, so a blank value would never match its read.
var nonBlank = regexp.MustCompile(`\S`)

// trimmed rejects text with surrounding whitespace, which the API trims
// and so would read back differently from the config.
func trimmed() validator.String { return trimmedValidator{} }

type trimmedValidator struct{}

func (trimmedValidator) Description(context.Context) string {
	return "no leading or trailing whitespace"
}

func (v trimmedValidator) MarkdownDescription(ctx context.Context) string { return v.Description(ctx) }

func (trimmedValidator) ValidateString(_ context.Context, req validator.StringRequest, resp *validator.StringResponse) {
	if req.ConfigValue.IsNull() || req.ConfigValue.IsUnknown() {
		return
	}
	if s := req.ConfigValue.ValueString(); s != strings.TrimSpace(s) {
		resp.Diagnostics.AddAttributeError(req.Path, "Surrounding whitespace",
			"bizkit trims this value; remove the leading or trailing whitespace (a heredoc ends in a newline; use chomp()).")
	}
}

// canonicalURL rejects http(s) URLs that the API's URL parser would
// rewrite: an upper-case scheme or host, a default port, or a bare origin
// without its "/" path.
func canonicalURL() validator.String { return canonicalURLValidator{} }

type canonicalURLValidator struct{}

func (canonicalURLValidator) Description(context.Context) string {
	return "an http or https URL in the form bizkit stores it"
}

func (v canonicalURLValidator) MarkdownDescription(ctx context.Context) string {
	return v.Description(ctx)
}

func (canonicalURLValidator) ValidateString(_ context.Context, req validator.StringRequest, resp *validator.StringResponse) {
	if req.ConfigValue.IsNull() || req.ConfigValue.IsUnknown() {
		return
	}
	raw := req.ConfigValue.ValueString()
	u, err := url.Parse(raw)
	if err != nil || (u.Scheme != "http" && u.Scheme != "https") || u.Host == "" {
		resp.Diagnostics.AddAttributeError(req.Path, "Invalid URL", raw+" is not an http or https URL.")
		return
	}
	want := *u
	want.Host = strings.ToLower(u.Host)
	if (u.Scheme == "https" && u.Port() == "443") || (u.Scheme == "http" && u.Port() == "80") {
		want.Host = strings.ToLower(u.Hostname())
	}
	if want.Path == "" && want.RawPath == "" {
		want.Path = "/"
	}
	if canonical := want.String(); canonical != raw {
		resp.Diagnostics.AddAttributeError(req.Path, "URL isn't in canonical form",
			"bizkit stores "+raw+" as "+canonical+"; write it that way.")
	}
}
