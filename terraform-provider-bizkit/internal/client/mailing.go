package client

import (
	"context"
	"net/http"
)

// MailingList is the API's MailingList, without the contact counts.
type MailingList struct {
	ID          string   `json:"id"`
	ProjectID   string   `json:"project_id"`
	Name        string   `json:"name"`
	Description *string  `json:"description"`
	Footer      *string  `json:"footer"`
	DoubleOptIn bool     `json:"double_opt_in"`
	FormIDs     []string `json:"form_ids"`
	Archive     bool     `json:"archive"`
	ArchiveURL  string   `json:"archive_url"`
	CreatedAt   string   `json:"created_at"`
	UpdatedAt   string   `json:"updated_at"`
}

// CreateMailingList is mailing.lists.create's input.
type CreateMailingList struct {
	ProjectID   string   `json:"project_id"`
	Name        string   `json:"name"`
	Description *string  `json:"description,omitempty"`
	Footer      *string  `json:"footer,omitempty"`
	DoubleOptIn bool     `json:"double_opt_in"`
	FormIDs     []string `json:"form_ids"`
	Archive     bool     `json:"archive"`
}

// UpdateMailingList is mailing.lists.update's input. Description and
// Footer set to null clear them.
type UpdateMailingList struct {
	Name        *string          `json:"name,omitempty"`
	Description Nullable[string] `json:"description"`
	Footer      Nullable[string] `json:"footer"`
	DoubleOptIn *bool            `json:"double_opt_in,omitempty"`
	FormIDs     *[]string        `json:"form_ids,omitempty"`
	Archive     *bool            `json:"archive,omitempty"`
}

// CreateMailingList adds a list.
func (c *Client) CreateMailingList(ctx context.Context, in CreateMailingList) (*MailingList, error) {
	var out MailingList
	return &out, c.do(ctx, http.MethodPost, c.workspacePath("mailing", "lists"), nil, in, &out)
}

// GetMailingList reads a list by ID.
func (c *Client) GetMailingList(ctx context.Context, id string) (*MailingList, error) {
	var out MailingList
	return &out, c.do(ctx, http.MethodGet, c.workspacePath("mailing", "lists", id), nil, nil, &out)
}

// UpdateMailingList changes a list.
func (c *Client) UpdateMailingList(ctx context.Context, id string, in UpdateMailingList) (*MailingList, error) {
	body, err := patchBody(in, map[string]bool{"description": in.Description.Set, "footer": in.Footer.Set})
	if err != nil {
		return nil, err
	}
	var out MailingList
	return &out, c.do(ctx, http.MethodPatch, c.workspacePath("mailing", "lists", id), nil, body, &out)
}

// DeleteMailingList deletes a list with its contacts, broadcasts and
// sequences.
func (c *Client) DeleteMailingList(ctx context.Context, id string) error {
	return c.do(ctx, http.MethodDelete, c.workspacePath("mailing", "lists", id), nil, nil, nil)
}
