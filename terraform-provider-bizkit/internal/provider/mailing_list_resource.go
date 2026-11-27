package provider

import (
	"context"

	"github.com/hashicorp/terraform-plugin-framework-validators/stringvalidator"
	"github.com/hashicorp/terraform-plugin-framework/diag"
	"github.com/hashicorp/terraform-plugin-framework/path"
	"github.com/hashicorp/terraform-plugin-framework/resource"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema/booldefault"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema/planmodifier"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema/setdefault"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema/stringplanmodifier"
	"github.com/hashicorp/terraform-plugin-framework/schema/validator"
	"github.com/hashicorp/terraform-plugin-framework/types"

	"github.com/hay-kot/bizkit/terraform-provider-bizkit/internal/client"
)

var (
	_ resource.ResourceWithConfigure   = (*mailingListResource)(nil)
	_ resource.ResourceWithImportState = (*mailingListResource)(nil)
)

type mailingListResource struct{ client *client.Client }

func newMailingListResource() resource.Resource { return &mailingListResource{} }

type mailingListModel struct {
	ID          types.String `tfsdk:"id"`
	ProjectID   types.String `tfsdk:"project_id"`
	Name        types.String `tfsdk:"name"`
	Description types.String `tfsdk:"description"`
	Footer      types.String `tfsdk:"footer"`
	DoubleOptIn types.Bool   `tfsdk:"double_opt_in"`
	FormIDs     types.Set    `tfsdk:"form_ids"`
	Archive     types.Bool   `tfsdk:"archive"`
	ArchiveURL  types.String `tfsdk:"archive_url"`
	CreatedAt   types.String `tfsdk:"created_at"`
}

func (m *mailingListModel) fill(ctx context.Context, l *client.MailingList, diags *diag.Diagnostics) {
	m.ID = types.StringValue(l.ID)
	m.ProjectID = types.StringValue(l.ProjectID)
	m.Name = types.StringValue(l.Name)
	m.Description = types.StringPointerValue(l.Description)
	m.Footer = types.StringPointerValue(l.Footer)
	m.DoubleOptIn = types.BoolValue(l.DoubleOptIn)
	m.FormIDs = setOf(ctx, l.FormIDs, diags)
	m.Archive = types.BoolValue(l.Archive)
	m.ArchiveURL = types.StringValue(l.ArchiveURL)
	m.CreatedAt = types.StringValue(l.CreatedAt)
}

func (r *mailingListResource) Metadata(_ context.Context, req resource.MetadataRequest, resp *resource.MetadataResponse) {
	resp.TypeName = req.ProviderTypeName + "_mailing_list"
}

func (r *mailingListResource) Schema(_ context.Context, _ resource.SchemaRequest, resp *resource.SchemaResponse) {
	emptySet, _ := types.SetValue(types.StringType, nil)
	notBlank := []validator.String{stringvalidator.RegexMatches(nonBlank, "must not be blank; leave it unset instead")}
	resp.Schema = schema.Schema{
		MarkdownDescription: "A mailing list. Contacts and their consent belong to one list. Needs an API key with `mailing:admin`. " +
			"Destroying a list deletes its contacts, broadcasts and sequences.",
		Attributes: map[string]schema.Attribute{
			"id": schema.StringAttribute{
				Computed: true, MarkdownDescription: "The list ID.",
				PlanModifiers: []planmodifier.String{stringplanmodifier.UseStateForUnknown()},
			},
			"project_id": schema.StringAttribute{
				Required: true, MarkdownDescription: "The project it belongs to. Changing it replaces the list.",
				PlanModifiers: []planmodifier.String{stringplanmodifier.RequiresReplace()},
			},
			"name":        schema.StringAttribute{Required: true, MarkdownDescription: "The display name."},
			"description": schema.StringAttribute{Optional: true, Validators: notBlank, MarkdownDescription: "What subscribers get, for the team."},
			"footer":      schema.StringAttribute{Optional: true, Validators: notBlank, MarkdownDescription: "Text at the end of every mail, such as a postal address."},
			"double_opt_in": schema.BoolAttribute{
				Optional: true, Computed: true, Default: booldefault.StaticBool(true),
				MarkdownDescription: "API and admin adds must confirm by mail. Form captures always confirm. Defaults to `true`.",
			},
			"form_ids": schema.SetAttribute{
				ElementType: types.StringType, Optional: true, Computed: true, Default: setdefault.StaticValue(emptySet),
				MarkdownDescription: "Forms of the same project whose submissions with an email join the list.",
			},
			"archive": schema.BoolAttribute{
				Optional: true, Computed: true, Default: booldefault.StaticBool(false),
				MarkdownDescription: "Whether the public archive shows broadcasts marked archived. Defaults to `false`.",
			},
			"archive_url": schema.StringAttribute{
				Computed: true, MarkdownDescription: "The public archive page, set even while the archive is off.",
				PlanModifiers: []planmodifier.String{stringplanmodifier.UseStateForUnknown()},
			},
			"created_at": schema.StringAttribute{
				Computed: true, MarkdownDescription: "When it was created (RFC 3339).",
				PlanModifiers: []planmodifier.String{stringplanmodifier.UseStateForUnknown()},
			},
		},
	}
}

func (r *mailingListResource) Configure(_ context.Context, req resource.ConfigureRequest, resp *resource.ConfigureResponse) {
	r.client = clientFrom(req.ProviderData, &resp.Diagnostics)
}

func (r *mailingListResource) Create(ctx context.Context, req resource.CreateRequest, resp *resource.CreateResponse) {
	var plan mailingListModel
	resp.Diagnostics.Append(req.Plan.Get(ctx, &plan)...)
	if resp.Diagnostics.HasError() {
		return
	}
	l, err := r.client.CreateMailingList(ctx, client.CreateMailingList{
		ProjectID:   plan.ProjectID.ValueString(),
		Name:        plan.Name.ValueString(),
		Description: stringPtr(plan.Description),
		Footer:      stringPtr(plan.Footer),
		DoubleOptIn: plan.DoubleOptIn.ValueBool(),
		FormIDs:     stringSet(ctx, plan.FormIDs, &resp.Diagnostics),
		Archive:     plan.Archive.ValueBool(),
	})
	if err != nil {
		apiError(&resp.Diagnostics, "create the mailing list", err)
		return
	}
	plan.fill(ctx, l, &resp.Diagnostics)
	resp.Diagnostics.Append(resp.State.Set(ctx, &plan)...)
}

func (r *mailingListResource) Read(ctx context.Context, req resource.ReadRequest, resp *resource.ReadResponse) {
	var state mailingListModel
	resp.Diagnostics.Append(req.State.Get(ctx, &state)...)
	if resp.Diagnostics.HasError() {
		return
	}
	l, err := r.client.GetMailingList(ctx, state.ID.ValueString())
	if client.IsNotFound(err) {
		resp.State.RemoveResource(ctx)
		return
	}
	if err != nil {
		apiError(&resp.Diagnostics, "read the mailing list", err)
		return
	}
	state.fill(ctx, l, &resp.Diagnostics)
	resp.Diagnostics.Append(resp.State.Set(ctx, &state)...)
}

func (r *mailingListResource) Update(ctx context.Context, req resource.UpdateRequest, resp *resource.UpdateResponse) {
	var plan, state mailingListModel
	resp.Diagnostics.Append(req.Plan.Get(ctx, &plan)...)
	resp.Diagnostics.Append(req.State.Get(ctx, &state)...)
	if resp.Diagnostics.HasError() {
		return
	}
	forms := stringSet(ctx, plan.FormIDs, &resp.Diagnostics)
	l, err := r.client.UpdateMailingList(ctx, plan.ID.ValueString(), client.UpdateMailingList{
		Name:        client.Ptr(plan.Name.ValueString()),
		Description: clearable(plan.Description, state.Description),
		Footer:      clearable(plan.Footer, state.Footer),
		DoubleOptIn: client.Ptr(plan.DoubleOptIn.ValueBool()),
		FormIDs:     &forms,
		Archive:     client.Ptr(plan.Archive.ValueBool()),
	})
	if err != nil {
		apiError(&resp.Diagnostics, "update the mailing list", err)
		return
	}
	plan.fill(ctx, l, &resp.Diagnostics)
	resp.Diagnostics.Append(resp.State.Set(ctx, &plan)...)
}

func (r *mailingListResource) Delete(ctx context.Context, req resource.DeleteRequest, resp *resource.DeleteResponse) {
	var state mailingListModel
	resp.Diagnostics.Append(req.State.Get(ctx, &state)...)
	if resp.Diagnostics.HasError() {
		return
	}
	if err := r.client.DeleteMailingList(ctx, state.ID.ValueString()); err != nil && !client.IsNotFound(err) {
		apiError(&resp.Diagnostics, "delete the mailing list", err)
	}
}

func (r *mailingListResource) ImportState(ctx context.Context, req resource.ImportStateRequest, resp *resource.ImportStateResponse) {
	resource.ImportStatePassthroughID(ctx, path.Root("id"), req, resp)
}
