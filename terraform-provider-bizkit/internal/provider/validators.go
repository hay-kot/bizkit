package provider

import (
	"context"
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
