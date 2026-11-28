// Package provider implements the bizkit Terraform/OpenTofu provider.
package provider

import (
	"context"
	"os"

	"github.com/hashicorp/terraform-plugin-framework/datasource"
	"github.com/hashicorp/terraform-plugin-framework/path"
	"github.com/hashicorp/terraform-plugin-framework/provider"
	"github.com/hashicorp/terraform-plugin-framework/provider/schema"
	"github.com/hashicorp/terraform-plugin-framework/resource"
	"github.com/hashicorp/terraform-plugin-framework/types"

	"github.com/hay-kot/bizkit/terraform-provider-bizkit/internal/client"
)

var _ provider.Provider = (*bizkitProvider)(nil)

type bizkitProvider struct {
	version string
}

// New returns the provider factory providerserver.Serve takes.
func New(version string) func() provider.Provider {
	return func() provider.Provider { return &bizkitProvider{version: version} }
}

type providerModel struct {
	Endpoint    types.String `tfsdk:"endpoint"`
	APIKey      types.String `tfsdk:"api_key"`
	WorkspaceID types.String `tfsdk:"workspace_id"`
}

func (p *bizkitProvider) Metadata(_ context.Context, _ provider.MetadataRequest, resp *provider.MetadataResponse) {
	resp.TypeName = "bizkit"
	resp.Version = p.version
}

func (p *bizkitProvider) Schema(_ context.Context, _ provider.SchemaRequest, resp *provider.SchemaResponse) {
	resp.Schema = schema.Schema{
		MarkdownDescription: "Manages one bizkit workspace's configuration with a workspace API key. " +
			"The key's scopes decide which resources it can manage: most need `<area>:write`, " +
			"and mailing lists need `mailing:admin`.",
		Attributes: map[string]schema.Attribute{
			"endpoint": schema.StringAttribute{
				MarkdownDescription: "The bizkit base URL, such as `https://bizkit.example.com`. Defaults to `BIZKIT_ENDPOINT`.",
				Optional:            true,
			},
			"api_key": schema.StringAttribute{
				MarkdownDescription: "A workspace API key (`bk_...`). Defaults to `BIZKIT_API_KEY`.",
				Optional:            true,
				Sensitive:           true,
			},
			"workspace_id": schema.StringAttribute{
				MarkdownDescription: "The ID of the workspace the key belongs to. Defaults to `BIZKIT_WORKSPACE_ID`.",
				Optional:            true,
			},
		},
	}
}

func (p *bizkitProvider) Configure(ctx context.Context, req provider.ConfigureRequest, resp *provider.ConfigureResponse) {
	var cfg providerModel
	resp.Diagnostics.Append(req.Config.Get(ctx, &cfg)...)
	if resp.Diagnostics.HasError() {
		return
	}
	value := func(attr types.String, env, name string) string {
		if attr.IsUnknown() {
			resp.Diagnostics.AddAttributeError(path.Root(name), "Unknown "+name,
				"The provider can't connect until "+name+" is known. Set it to a value known at plan time.")
			return ""
		}
		v := attr.ValueString()
		if v == "" {
			v = os.Getenv(env)
		}
		if v == "" {
			resp.Diagnostics.AddAttributeError(path.Root(name), "Missing "+name,
				"Set "+name+" in the provider block or the "+env+" environment variable.")
		}
		return v
	}
	endpoint := value(cfg.Endpoint, "BIZKIT_ENDPOINT", "endpoint")
	apiKey := value(cfg.APIKey, "BIZKIT_API_KEY", "api_key")
	workspaceID := value(cfg.WorkspaceID, "BIZKIT_WORKSPACE_ID", "workspace_id")
	if resp.Diagnostics.HasError() {
		return
	}
	c, err := client.New(endpoint, apiKey, workspaceID, nil)
	if err != nil {
		resp.Diagnostics.AddAttributeError(path.Root("endpoint"), "Invalid endpoint", err.Error())
		return
	}
	resp.DataSourceData = c
	resp.ResourceData = c
}

func (p *bizkitProvider) Resources(context.Context) []func() resource.Resource {
	return []func() resource.Resource{
		newProjectResource,
		newFormResource,
		newWebhookEndpointResource,
		newMailingListResource,
	}
}

func (p *bizkitProvider) DataSources(context.Context) []func() datasource.DataSource {
	return []func() datasource.DataSource{
		newWorkspaceDataSource,
		newProjectDataSource,
	}
}
