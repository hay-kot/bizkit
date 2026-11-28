package client

import (
	"context"
	"net/http"
)

// FieldDef is a field a form accepts.
type FieldDef struct {
	Name      string   `json:"name"`
	Kind      string   `json:"kind"`
	Label     *string  `json:"label,omitempty"`
	Required  bool     `json:"required"`
	MaxLength *int64   `json:"max_length,omitempty"`
	Options   []string `json:"options"`
	Multiple  bool     `json:"multiple"`
}

// Form is the API's Form.
type Form struct {
	ID           string     `json:"id"`
	ProjectID    string     `json:"project_id"`
	Name         string     `json:"name"`
	Enabled      bool       `json:"enabled"`
	Fields       []FieldDef `json:"fields"`
	BotCheck     string     `json:"bot_check"`
	MinFillMs    int64      `json:"min_fill_ms"`
	NotifyEmails []string   `json:"notify_emails"`
	RedirectURL  *string    `json:"redirect_url"`
	Endpoint     string     `json:"endpoint"`
	ChallengeURL string     `json:"challenge_url"`
	InboxCount   int64      `json:"inbox_count"`
	CreatedAt    string     `json:"created_at"`
	UpdatedAt    string     `json:"updated_at"`
}

// CreateForm is forms.create's input. The API has no enabled field on
// create; a new form starts enabled.
type CreateForm struct {
	ProjectID    string     `json:"project_id"`
	Name         string     `json:"name"`
	Fields       []FieldDef `json:"fields"`
	BotCheck     string     `json:"bot_check"`
	MinFillMs    int64      `json:"min_fill_ms"`
	NotifyEmails []string   `json:"notify_emails"`
	RedirectURL  *string    `json:"redirect_url,omitempty"`
}

// UpdateForm is forms.update's input. Nil fields are left as they are;
// RedirectURL set to null clears it.
type UpdateForm struct {
	Name         *string          `json:"name,omitempty"`
	Enabled      *bool            `json:"enabled,omitempty"`
	Fields       *[]FieldDef      `json:"fields,omitempty"`
	BotCheck     *string          `json:"bot_check,omitempty"`
	MinFillMs    *int64           `json:"min_fill_ms,omitempty"`
	NotifyEmails *[]string        `json:"notify_emails,omitempty"`
	RedirectURL  Nullable[string] `json:"redirect_url"`
}

// CreateForm adds a form.
func (c *Client) CreateForm(ctx context.Context, in CreateForm) (*Form, error) {
	var out Form
	return &out, c.do(ctx, http.MethodPost, c.workspacePath("forms"), nil, in, &out)
}

// GetForm reads a form by ID.
func (c *Client) GetForm(ctx context.Context, id string) (*Form, error) {
	var out Form
	return &out, c.do(ctx, http.MethodGet, c.workspacePath("forms", id), nil, nil, &out)
}

// UpdateForm changes a form.
func (c *Client) UpdateForm(ctx context.Context, id string, in UpdateForm) (*Form, error) {
	body, err := patchBody(in, map[string]bool{"redirect_url": in.RedirectURL.Set})
	if err != nil {
		return nil, err
	}
	var out Form
	return &out, c.do(ctx, http.MethodPatch, c.workspacePath("forms", id), nil, body, &out)
}

// DeleteForm deletes a form and its submissions.
func (c *Client) DeleteForm(ctx context.Context, id string) error {
	return c.do(ctx, http.MethodDelete, c.workspacePath("forms", id), nil, nil, nil)
}
