package provider

import (
	"context"

	"github.com/hashicorp/terraform-plugin-framework-validators/datasourcevalidator"
	"github.com/hashicorp/terraform-plugin-framework/datasource"
	"github.com/hashicorp/terraform-plugin-framework/datasource/schema"
	"github.com/hashicorp/terraform-plugin-framework/path"
	"github.com/hashicorp/terraform-plugin-framework/types"

	"github.com/hay-kot/bizkit/terraform-provider-bizkit/internal/client"
)

var (
	_ datasource.DataSourceWithConfigure        = (*workspaceDataSource)(nil)
	_ datasource.DataSourceWithConfigure        = (*projectDataSource)(nil)
	_ datasource.DataSourceWithConfigValidators = (*projectDataSource)(nil)
)

type workspaceDataSource struct{ client *client.Client }

func newWorkspaceDataSource() datasource.DataSource { return &workspaceDataSource{} }

type workspaceModel struct {
	ID   types.String `tfsdk:"id"`
	Slug types.String `tfsdk:"slug"`
	Name types.String `tfsdk:"name"`
}

func (d *workspaceDataSource) Metadata(_ context.Context, req datasource.MetadataRequest, resp *datasource.MetadataResponse) {
	resp.TypeName = req.ProviderTypeName + "_workspace"
}

func (d *workspaceDataSource) Schema(_ context.Context, _ datasource.SchemaRequest, resp *datasource.SchemaResponse) {
	resp.Schema = schema.Schema{
		MarkdownDescription: "The workspace the provider manages.",
		Attributes: map[string]schema.Attribute{
			"id":   schema.StringAttribute{Computed: true, MarkdownDescription: "The workspace ID."},
			"slug": schema.StringAttribute{Computed: true, MarkdownDescription: "The URL-safe name."},
			"name": schema.StringAttribute{Computed: true, MarkdownDescription: "The display name."},
		},
	}
}

func (d *workspaceDataSource) Configure(_ context.Context, req datasource.ConfigureRequest, resp *datasource.ConfigureResponse) {
	d.client = clientFrom(req.ProviderData, &resp.Diagnostics)
}

func (d *workspaceDataSource) Read(ctx context.Context, _ datasource.ReadRequest, resp *datasource.ReadResponse) {
	w, err := d.client.GetWorkspace(ctx)
	if err != nil {
		apiError(&resp.Diagnostics, "read the workspace", err)
		return
	}
	resp.Diagnostics.Append(resp.State.Set(ctx, &workspaceModel{
		ID: types.StringValue(w.ID), Slug: types.StringValue(w.Slug), Name: types.StringValue(w.Name),
	})...)
}

type projectDataSource struct{ client *client.Client }

func newProjectDataSource() datasource.DataSource { return &projectDataSource{} }

type projectLookupModel struct {
	ID   types.String `tfsdk:"id"`
	Slug types.String `tfsdk:"slug"`
	Name types.String `tfsdk:"name"`
}

func (d *projectDataSource) Metadata(_ context.Context, req datasource.MetadataRequest, resp *datasource.MetadataResponse) {
	resp.TypeName = req.ProviderTypeName + "_project"
}

func (d *projectDataSource) Schema(_ context.Context, _ datasource.SchemaRequest, resp *datasource.SchemaResponse) {
	resp.Schema = schema.Schema{
		MarkdownDescription: "A project of the workspace, found by `id` or `slug`.",
		Attributes: map[string]schema.Attribute{
			"id":   schema.StringAttribute{Optional: true, Computed: true, MarkdownDescription: "The project ID."},
			"slug": schema.StringAttribute{Optional: true, Computed: true, MarkdownDescription: "The URL-safe name."},
			"name": schema.StringAttribute{Computed: true, MarkdownDescription: "The display name."},
		},
	}
}

func (d *projectDataSource) ConfigValidators(context.Context) []datasource.ConfigValidator {
	return []datasource.ConfigValidator{
		datasourcevalidator.ExactlyOneOf(path.MatchRoot("id"), path.MatchRoot("slug")),
	}
}

func (d *projectDataSource) Configure(_ context.Context, req datasource.ConfigureRequest, resp *datasource.ConfigureResponse) {
	d.client = clientFrom(req.ProviderData, &resp.Diagnostics)
}

func (d *projectDataSource) Read(ctx context.Context, req datasource.ReadRequest, resp *datasource.ReadResponse) {
	var cfg projectLookupModel
	resp.Diagnostics.Append(req.Config.Get(ctx, &cfg)...)
	if resp.Diagnostics.HasError() {
		return
	}
	var found *client.Project
	if !cfg.ID.IsNull() {
		p, err := d.client.GetProject(ctx, cfg.ID.ValueString())
		if err != nil {
			apiError(&resp.Diagnostics, "read the project", err)
			return
		}
		found = p
	} else {
		projects, err := d.client.ListProjects(ctx)
		if err != nil {
			apiError(&resp.Diagnostics, "list projects", err)
			return
		}
		for i := range projects {
			if projects[i].Slug == cfg.Slug.ValueString() {
				found = &projects[i]
			}
		}
		if found == nil {
			resp.Diagnostics.AddAttributeError(path.Root("slug"), "No such project",
				"The workspace has no project with slug "+cfg.Slug.ValueString()+".")
			return
		}
	}
	resp.Diagnostics.Append(resp.State.Set(ctx, &projectLookupModel{
		ID: types.StringValue(found.ID), Slug: types.StringValue(found.Slug), Name: types.StringValue(found.Name),
	})...)
}
