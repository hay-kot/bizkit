package provider

import (
	"context"
	"fmt"

	"github.com/hashicorp/terraform-plugin-framework/diag"
	"github.com/hashicorp/terraform-plugin-framework/types"

	"github.com/hay-kot/bizkit/terraform-provider-bizkit/internal/client"
)

// clientFrom takes the client Configure handed over. It is nil while the
// provider isn't configured yet (validation), which callers allow.
func clientFrom(data any, diags *diag.Diagnostics) *client.Client {
	if data == nil {
		return nil
	}
	c, ok := data.(*client.Client)
	if !ok {
		diags.AddError("Unexpected provider data", fmt.Sprintf("expected *client.Client, got %T", data))
		return nil
	}
	return c
}

func apiError(diags *diag.Diagnostics, action string, err error) {
	diags.AddError("Couldn't "+action, err.Error())
}

// stringPtr gives nil for a null or unknown string.
func stringPtr(v types.String) *string {
	if v.IsNull() || v.IsUnknown() {
		return nil
	}
	s := v.ValueString()
	return &s
}

// clearable turns a planned optional string into a PATCH value: set when
// it changed, null when it was removed, left out when unchanged.
func clearable(plan, state types.String) client.Nullable[string] {
	if plan.Equal(state) {
		return client.Nullable[string]{}
	}
	if plan.IsNull() {
		return client.Null[string]()
	}
	return client.Value(plan.ValueString())
}

func stringSet(ctx context.Context, v types.Set, diags *diag.Diagnostics) []string {
	out := []string{}
	if v.IsNull() || v.IsUnknown() {
		return out
	}
	diags.Append(v.ElementsAs(ctx, &out, false)...)
	return out
}

func setOf(ctx context.Context, values []string, diags *diag.Diagnostics) types.Set {
	if values == nil {
		values = []string{}
	}
	s, d := types.SetValueFrom(ctx, types.StringType, values)
	diags.Append(d...)
	return s
}
