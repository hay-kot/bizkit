package provider

import (
	"context"

	"github.com/hashicorp/terraform-plugin-framework-validators/stringvalidator"
	"github.com/hashicorp/terraform-plugin-framework/attr"
	"github.com/hashicorp/terraform-plugin-framework/diag"
	"github.com/hashicorp/terraform-plugin-framework/path"
	"github.com/hashicorp/terraform-plugin-framework/resource"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema/booldefault"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema/int64default"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema/listdefault"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema/planmodifier"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema/setdefault"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema/stringdefault"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema/stringplanmodifier"
	"github.com/hashicorp/terraform-plugin-framework/schema/validator"
	"github.com/hashicorp/terraform-plugin-framework/types"

	"github.com/hay-kot/bizkit/terraform-provider-bizkit/internal/client"
)

var (
	_ resource.ResourceWithConfigure   = (*formResource)(nil)
	_ resource.ResourceWithImportState = (*formResource)(nil)
)

type formResource struct{ client *client.Client }

func newFormResource() resource.Resource { return &formResource{} }

type formModel struct {
	ID           types.String `tfsdk:"id"`
	ProjectID    types.String `tfsdk:"project_id"`
	Name         types.String `tfsdk:"name"`
	Enabled      types.Bool   `tfsdk:"enabled"`
	Fields       types.List   `tfsdk:"fields"`
	BotCheck     types.String `tfsdk:"bot_check"`
	MinFillMs    types.Int64  `tfsdk:"min_fill_ms"`
	NotifyEmails types.Set    `tfsdk:"notify_emails"`
	RedirectURL  types.String `tfsdk:"redirect_url"`
	Endpoint     types.String `tfsdk:"endpoint"`
	ChallengeURL types.String `tfsdk:"challenge_url"`
	CreatedAt    types.String `tfsdk:"created_at"`
}

type fieldModel struct {
	Name      types.String `tfsdk:"name"`
	Kind      types.String `tfsdk:"kind"`
	Label     types.String `tfsdk:"label"`
	Required  types.Bool   `tfsdk:"required"`
	MaxLength types.Int64  `tfsdk:"max_length"`
	Options   types.List   `tfsdk:"options"`
	Multiple  types.Bool   `tfsdk:"multiple"`
}

var fieldAttrTypes = map[string]attr.Type{
	"name":       types.StringType,
	"kind":       types.StringType,
	"label":      types.StringType,
	"required":   types.BoolType,
	"max_length": types.Int64Type,
	"options":    types.ListType{ElemType: types.StringType},
	"multiple":   types.BoolType,
}

func (m *formModel) fill(ctx context.Context, f *client.Form, diags *diag.Diagnostics) {
	m.ID = types.StringValue(f.ID)
	m.ProjectID = types.StringValue(f.ProjectID)
	m.Name = types.StringValue(f.Name)
	m.Enabled = types.BoolValue(f.Enabled)
	m.BotCheck = types.StringValue(f.BotCheck)
	m.MinFillMs = types.Int64Value(f.MinFillMs)
	m.NotifyEmails = setOf(ctx, f.NotifyEmails, diags)
	m.RedirectURL = types.StringPointerValue(f.RedirectURL)
	m.Endpoint = types.StringValue(f.Endpoint)
	m.ChallengeURL = types.StringValue(f.ChallengeURL)
	m.CreatedAt = types.StringValue(f.CreatedAt)

	fields := make([]fieldModel, 0, len(f.Fields))
	for _, fd := range f.Fields {
		options := fd.Options
		if options == nil {
			options = []string{}
		}
		opts, d := types.ListValueFrom(ctx, types.StringType, options)
		diags.Append(d...)
		fields = append(fields, fieldModel{
			Name:      types.StringValue(fd.Name),
			Kind:      types.StringValue(fd.Kind),
			Label:     types.StringPointerValue(fd.Label),
			Required:  types.BoolValue(fd.Required),
			MaxLength: types.Int64PointerValue(fd.MaxLength),
			Options:   opts,
			Multiple:  types.BoolValue(fd.Multiple),
		})
	}
	list, d := types.ListValueFrom(ctx, types.ObjectType{AttrTypes: fieldAttrTypes}, fields)
	diags.Append(d...)
	m.Fields = list
}

func (m *formModel) fieldDefs(ctx context.Context, diags *diag.Diagnostics) []client.FieldDef {
	var fields []fieldModel
	diags.Append(m.Fields.ElementsAs(ctx, &fields, false)...)
	out := make([]client.FieldDef, 0, len(fields))
	for _, f := range fields {
		options := []string{}
		if !f.Options.IsNull() && !f.Options.IsUnknown() {
			diags.Append(f.Options.ElementsAs(ctx, &options, false)...)
		}
		out = append(out, client.FieldDef{
			Name:      f.Name.ValueString(),
			Kind:      f.Kind.ValueString(),
			Label:     stringPtr(f.Label),
			Required:  f.Required.ValueBool(),
			MaxLength: f.MaxLength.ValueInt64Pointer(),
			Options:   options,
			Multiple:  f.Multiple.ValueBool(),
		})
	}
	return out
}

func (r *formResource) Metadata(_ context.Context, req resource.MetadataRequest, resp *resource.MetadataResponse) {
	resp.TypeName = req.ProviderTypeName + "_form"
}

func (r *formResource) Schema(_ context.Context, _ resource.SchemaRequest, resp *resource.SchemaResponse) {
	emptyStrings, _ := types.ListValue(types.StringType, nil)
	emptyFields, _ := types.ListValue(types.ObjectType{AttrTypes: fieldAttrTypes}, nil)
	emptySet, _ := types.SetValue(types.StringType, nil)
	resp.Schema = schema.Schema{
		MarkdownDescription: "A form that collects submissions from a site. Sites post to `endpoint`.",
		Attributes: map[string]schema.Attribute{
			"id": schema.StringAttribute{
				Computed: true, MarkdownDescription: "The form ID.",
				PlanModifiers: []planmodifier.String{stringplanmodifier.UseStateForUnknown()},
			},
			"project_id": schema.StringAttribute{
				Required: true, MarkdownDescription: "The project the form belongs to. Changing it replaces the form.",
				PlanModifiers: []planmodifier.String{stringplanmodifier.RequiresReplace()},
			},
			"name": schema.StringAttribute{Required: true, MarkdownDescription: "The display name."},
			"enabled": schema.BoolAttribute{
				Optional: true, Computed: true, Default: booldefault.StaticBool(true),
				MarkdownDescription: "Disabled forms answer 404 to submissions. Defaults to `true`.",
			},
			"fields": schema.ListNestedAttribute{
				Optional: true, Computed: true, Default: listdefault.StaticValue(emptyFields),
				MarkdownDescription: "The accepted fields. Empty keeps every field a site sends.",
				NestedObject: schema.NestedAttributeObject{Attributes: map[string]schema.Attribute{
					"name": schema.StringAttribute{Required: true, MarkdownDescription: "The input's `name`, unique in the form."},
					"kind": schema.StringAttribute{
						Required: true, MarkdownDescription: "One of `text`, `email`, `url`, `number`, `choice`.",
						Validators: []validator.String{stringvalidator.OneOf("text", "email", "url", "number", "choice")},
					},
					"label": schema.StringAttribute{Optional: true, MarkdownDescription: "A label for the UI and mail; the name when unset."},
					"required": schema.BoolAttribute{
						Optional: true, Computed: true, Default: booldefault.StaticBool(false),
						MarkdownDescription: "A non-blank value is needed. Defaults to `false`.",
					},
					"max_length": schema.Int64Attribute{Optional: true, MarkdownDescription: "The longest value, 1-10000 characters."},
					"options": schema.ListAttribute{
						ElementType: types.StringType, Optional: true, Computed: true, Default: listdefault.StaticValue(emptyStrings),
						MarkdownDescription: "The allowed values of a `choice` field.",
					},
					"multiple": schema.BoolAttribute{
						Optional: true, Computed: true, Default: booldefault.StaticBool(false),
						MarkdownDescription: "A `choice` field takes several values.",
					},
				}},
			},
			"bot_check": schema.StringAttribute{
				Optional: true, Computed: true, Default: stringdefault.StaticString("honeypot"),
				MarkdownDescription: "`honeypot` or `challenge` (ALTCHA proof of work). Defaults to `honeypot`.",
				Validators:          []validator.String{stringvalidator.OneOf("honeypot", "challenge")},
			},
			"min_fill_ms": schema.Int64Attribute{
				Optional: true, Computed: true, Default: int64default.StaticInt64(2000),
				MarkdownDescription: "Submissions faster than this after the page loads count as spam. Defaults to 2000.",
			},
			"notify_emails": schema.SetAttribute{
				ElementType: types.StringType, Optional: true, Computed: true, Default: setdefault.StaticValue(emptySet),
				MarkdownDescription: "Addresses mailed on each new submission, in lower case.",
				Validators:          []validator.Set{lowercaseElements()},
			},
			"redirect_url": schema.StringAttribute{
				Optional: true, MarkdownDescription: "Where browsers go after a plain HTML post.",
			},
			"endpoint": schema.StringAttribute{
				Computed: true, MarkdownDescription: "The URL sites post to.",
				PlanModifiers: []planmodifier.String{stringplanmodifier.UseStateForUnknown()},
			},
			"challenge_url": schema.StringAttribute{
				Computed: true, MarkdownDescription: "The URL the ALTCHA widget loads challenges from.",
				PlanModifiers: []planmodifier.String{stringplanmodifier.UseStateForUnknown()},
			},
			"created_at": schema.StringAttribute{
				Computed: true, MarkdownDescription: "When it was created (RFC 3339).",
				PlanModifiers: []planmodifier.String{stringplanmodifier.UseStateForUnknown()},
			},
		},
	}
}

func (r *formResource) Configure(_ context.Context, req resource.ConfigureRequest, resp *resource.ConfigureResponse) {
	r.client = clientFrom(req.ProviderData, &resp.Diagnostics)
}

func (r *formResource) Create(ctx context.Context, req resource.CreateRequest, resp *resource.CreateResponse) {
	var plan formModel
	resp.Diagnostics.Append(req.Plan.Get(ctx, &plan)...)
	if resp.Diagnostics.HasError() {
		return
	}
	in := client.CreateForm{
		ProjectID:    plan.ProjectID.ValueString(),
		Name:         plan.Name.ValueString(),
		Fields:       plan.fieldDefs(ctx, &resp.Diagnostics),
		BotCheck:     plan.BotCheck.ValueString(),
		MinFillMs:    plan.MinFillMs.ValueInt64(),
		NotifyEmails: stringSet(ctx, plan.NotifyEmails, &resp.Diagnostics),
		RedirectURL:  stringPtr(plan.RedirectURL),
	}
	if resp.Diagnostics.HasError() {
		return
	}
	f, err := r.client.CreateForm(ctx, in)
	if err != nil {
		apiError(&resp.Diagnostics, "create the form", err)
		return
	}
	// A new form starts enabled; create has no field for it.
	if !plan.Enabled.ValueBool() {
		f, err = r.client.UpdateForm(ctx, f.ID, client.UpdateForm{Enabled: client.Ptr(false)})
		if err != nil {
			apiError(&resp.Diagnostics, "disable the new form", err)
			return
		}
	}
	plan.fill(ctx, f, &resp.Diagnostics)
	resp.Diagnostics.Append(resp.State.Set(ctx, &plan)...)
}

func (r *formResource) Read(ctx context.Context, req resource.ReadRequest, resp *resource.ReadResponse) {
	var state formModel
	resp.Diagnostics.Append(req.State.Get(ctx, &state)...)
	if resp.Diagnostics.HasError() {
		return
	}
	f, err := r.client.GetForm(ctx, state.ID.ValueString())
	if client.IsNotFound(err) {
		resp.State.RemoveResource(ctx)
		return
	}
	if err != nil {
		apiError(&resp.Diagnostics, "read the form", err)
		return
	}
	state.fill(ctx, f, &resp.Diagnostics)
	resp.Diagnostics.Append(resp.State.Set(ctx, &state)...)
}

func (r *formResource) Update(ctx context.Context, req resource.UpdateRequest, resp *resource.UpdateResponse) {
	var plan, state formModel
	resp.Diagnostics.Append(req.Plan.Get(ctx, &plan)...)
	resp.Diagnostics.Append(req.State.Get(ctx, &state)...)
	if resp.Diagnostics.HasError() {
		return
	}
	fields := plan.fieldDefs(ctx, &resp.Diagnostics)
	emails := stringSet(ctx, plan.NotifyEmails, &resp.Diagnostics)
	in := client.UpdateForm{
		Name:         client.Ptr(plan.Name.ValueString()),
		Enabled:      client.Ptr(plan.Enabled.ValueBool()),
		Fields:       &fields,
		BotCheck:     client.Ptr(plan.BotCheck.ValueString()),
		MinFillMs:    client.Ptr(plan.MinFillMs.ValueInt64()),
		NotifyEmails: &emails,
		RedirectURL:  clearable(plan.RedirectURL, state.RedirectURL),
	}
	if resp.Diagnostics.HasError() {
		return
	}
	f, err := r.client.UpdateForm(ctx, plan.ID.ValueString(), in)
	if err != nil {
		apiError(&resp.Diagnostics, "update the form", err)
		return
	}
	plan.fill(ctx, f, &resp.Diagnostics)
	resp.Diagnostics.Append(resp.State.Set(ctx, &plan)...)
}

func (r *formResource) Delete(ctx context.Context, req resource.DeleteRequest, resp *resource.DeleteResponse) {
	var state formModel
	resp.Diagnostics.Append(req.State.Get(ctx, &state)...)
	if resp.Diagnostics.HasError() {
		return
	}
	if err := r.client.DeleteForm(ctx, state.ID.ValueString()); err != nil && !client.IsNotFound(err) {
		apiError(&resp.Diagnostics, "delete the form", err)
	}
}

func (r *formResource) ImportState(ctx context.Context, req resource.ImportStateRequest, resp *resource.ImportStateResponse) {
	resource.ImportStatePassthroughID(ctx, path.Root("id"), req, resp)
}
