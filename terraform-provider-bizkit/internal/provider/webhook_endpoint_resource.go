package provider

import (
	"context"

	"github.com/hashicorp/terraform-plugin-framework/path"
	"github.com/hashicorp/terraform-plugin-framework/resource"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema/booldefault"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema/planmodifier"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema/stringplanmodifier"
	"github.com/hashicorp/terraform-plugin-framework/types"

	"github.com/hay-kot/bizkit/terraform-provider-bizkit/internal/client"
)

var (
	_ resource.ResourceWithConfigure   = (*webhookEndpointResource)(nil)
	_ resource.ResourceWithImportState = (*webhookEndpointResource)(nil)
)

type webhookEndpointResource struct{ client *client.Client }

func newWebhookEndpointResource() resource.Resource { return &webhookEndpointResource{} }

type webhookEndpointModel struct {
	ID         types.String `tfsdk:"id"`
	ProjectID  types.String `tfsdk:"project_id"`
	Name       types.String `tfsdk:"name"`
	Enabled    types.Bool   `tfsdk:"enabled"`
	ForwardURL types.String `tfsdk:"forward_url"`
	URL        types.String `tfsdk:"url"`
	CreatedAt  types.String `tfsdk:"created_at"`
}

func (m *webhookEndpointModel) fill(e *client.Endpoint) {
	m.ID = types.StringValue(e.ID)
	m.ProjectID = types.StringValue(e.ProjectID)
	m.Name = types.StringValue(e.Name)
	m.Enabled = types.BoolValue(e.Enabled)
	m.ForwardURL = types.StringPointerValue(e.ForwardURL)
	m.URL = types.StringValue(e.URL)
	m.CreatedAt = types.StringValue(e.CreatedAt)
}

func (r *webhookEndpointResource) Metadata(_ context.Context, req resource.MetadataRequest, resp *resource.MetadataResponse) {
	resp.TypeName = req.ProviderTypeName + "_webhook_endpoint"
}

func (r *webhookEndpointResource) Schema(_ context.Context, _ resource.SchemaRequest, resp *resource.SchemaResponse) {
	resp.Schema = schema.Schema{
		MarkdownDescription: "A public URL that stores every request it gets, and can forward each one on.",
		Attributes: map[string]schema.Attribute{
			"id": schema.StringAttribute{
				Computed: true, MarkdownDescription: "The endpoint ID, also its public address.",
				PlanModifiers: []planmodifier.String{stringplanmodifier.UseStateForUnknown()},
			},
			"project_id": schema.StringAttribute{
				Required: true, MarkdownDescription: "The project it belongs to. Changing it replaces the endpoint.",
				PlanModifiers: []planmodifier.String{stringplanmodifier.RequiresReplace()},
			},
			"name": schema.StringAttribute{Required: true, MarkdownDescription: "The display name."},
			"enabled": schema.BoolAttribute{
				Optional: true, Computed: true, Default: booldefault.StaticBool(true),
				MarkdownDescription: "Disabled endpoints answer 404 and store nothing. Defaults to `true`.",
			},
			"forward_url": schema.StringAttribute{
				Optional:            true,
				MarkdownDescription: "Where each captured request is sent on. Public addresses only. Write it as bizkit normalizes URLs (a bare origin ends in `/`).",
			},
			"url": schema.StringAttribute{
				Computed: true, MarkdownDescription: "The public URL that captures requests.",
				PlanModifiers: []planmodifier.String{stringplanmodifier.UseStateForUnknown()},
			},
			"created_at": schema.StringAttribute{
				Computed: true, MarkdownDescription: "When it was created (RFC 3339).",
				PlanModifiers: []planmodifier.String{stringplanmodifier.UseStateForUnknown()},
			},
		},
	}
}

func (r *webhookEndpointResource) Configure(_ context.Context, req resource.ConfigureRequest, resp *resource.ConfigureResponse) {
	r.client = clientFrom(req.ProviderData, &resp.Diagnostics)
}

func (r *webhookEndpointResource) Create(ctx context.Context, req resource.CreateRequest, resp *resource.CreateResponse) {
	var plan webhookEndpointModel
	resp.Diagnostics.Append(req.Plan.Get(ctx, &plan)...)
	if resp.Diagnostics.HasError() {
		return
	}
	e, err := r.client.CreateEndpoint(ctx, client.CreateEndpoint{
		ProjectID:  plan.ProjectID.ValueString(),
		Name:       plan.Name.ValueString(),
		ForwardURL: stringPtr(plan.ForwardURL),
	})
	if err != nil {
		apiError(&resp.Diagnostics, "create the webhook endpoint", err)
		return
	}
	if !plan.Enabled.ValueBool() {
		e, err = r.client.UpdateEndpoint(ctx, e.ID, client.UpdateEndpoint{Enabled: client.Ptr(false)})
		if err != nil {
			apiError(&resp.Diagnostics, "disable the new webhook endpoint", err)
			return
		}
	}
	plan.fill(e)
	resp.Diagnostics.Append(resp.State.Set(ctx, &plan)...)
}

func (r *webhookEndpointResource) Read(ctx context.Context, req resource.ReadRequest, resp *resource.ReadResponse) {
	var state webhookEndpointModel
	resp.Diagnostics.Append(req.State.Get(ctx, &state)...)
	if resp.Diagnostics.HasError() {
		return
	}
	e, err := r.client.GetEndpoint(ctx, state.ID.ValueString())
	if client.IsNotFound(err) {
		resp.State.RemoveResource(ctx)
		return
	}
	if err != nil {
		apiError(&resp.Diagnostics, "read the webhook endpoint", err)
		return
	}
	state.fill(e)
	resp.Diagnostics.Append(resp.State.Set(ctx, &state)...)
}

func (r *webhookEndpointResource) Update(ctx context.Context, req resource.UpdateRequest, resp *resource.UpdateResponse) {
	var plan, state webhookEndpointModel
	resp.Diagnostics.Append(req.Plan.Get(ctx, &plan)...)
	resp.Diagnostics.Append(req.State.Get(ctx, &state)...)
	if resp.Diagnostics.HasError() {
		return
	}
	e, err := r.client.UpdateEndpoint(ctx, plan.ID.ValueString(), client.UpdateEndpoint{
		Name:       client.Ptr(plan.Name.ValueString()),
		Enabled:    client.Ptr(plan.Enabled.ValueBool()),
		ForwardURL: clearable(plan.ForwardURL, state.ForwardURL),
	})
	if err != nil {
		apiError(&resp.Diagnostics, "update the webhook endpoint", err)
		return
	}
	plan.fill(e)
	resp.Diagnostics.Append(resp.State.Set(ctx, &plan)...)
}

func (r *webhookEndpointResource) Delete(ctx context.Context, req resource.DeleteRequest, resp *resource.DeleteResponse) {
	var state webhookEndpointModel
	resp.Diagnostics.Append(req.State.Get(ctx, &state)...)
	if resp.Diagnostics.HasError() {
		return
	}
	if err := r.client.DeleteEndpoint(ctx, state.ID.ValueString()); err != nil && !client.IsNotFound(err) {
		apiError(&resp.Diagnostics, "delete the webhook endpoint", err)
	}
}

func (r *webhookEndpointResource) ImportState(ctx context.Context, req resource.ImportStateRequest, resp *resource.ImportStateResponse) {
	resource.ImportStatePassthroughID(ctx, path.Root("id"), req, resp)
}
