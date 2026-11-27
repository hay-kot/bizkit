package client

import (
	"context"
	"net/http"
)

// Endpoint is the API's webhook Endpoint.
type Endpoint struct {
	ID             string  `json:"id"`
	ProjectID      string  `json:"project_id"`
	Name           string  `json:"name"`
	Enabled        bool    `json:"enabled"`
	ForwardURL     *string `json:"forward_url"`
	URL            string  `json:"url"`
	RequestCount   int64   `json:"request_count"`
	LastReceivedAt *string `json:"last_received_at"`
	CreatedAt      string  `json:"created_at"`
	UpdatedAt      string  `json:"updated_at"`
}

// CreateEndpoint is webhooks.endpoints.create's input. A new endpoint
// starts enabled.
type CreateEndpoint struct {
	ProjectID  string  `json:"project_id"`
	Name       string  `json:"name"`
	ForwardURL *string `json:"forward_url,omitempty"`
}

// UpdateEndpoint is webhooks.endpoints.update's input. ForwardURL set to
// null stops forwarding.
type UpdateEndpoint struct {
	Name       *string          `json:"name,omitempty"`
	Enabled    *bool            `json:"enabled,omitempty"`
	ForwardURL Nullable[string] `json:"forward_url"`
}

// CreateEndpoint adds a webhook endpoint.
func (c *Client) CreateEndpoint(ctx context.Context, in CreateEndpoint) (*Endpoint, error) {
	var out Endpoint
	return &out, c.do(ctx, http.MethodPost, c.workspacePath("webhooks"), nil, in, &out)
}

// GetEndpoint reads an endpoint by ID.
func (c *Client) GetEndpoint(ctx context.Context, id string) (*Endpoint, error) {
	var out Endpoint
	return &out, c.do(ctx, http.MethodGet, c.workspacePath("webhooks", id), nil, nil, &out)
}

// UpdateEndpoint changes an endpoint.
func (c *Client) UpdateEndpoint(ctx context.Context, id string, in UpdateEndpoint) (*Endpoint, error) {
	body, err := patchBody(in, map[string]bool{"forward_url": in.ForwardURL.Set})
	if err != nil {
		return nil, err
	}
	var out Endpoint
	return &out, c.do(ctx, http.MethodPatch, c.workspacePath("webhooks", id), nil, body, &out)
}

// DeleteEndpoint deletes an endpoint and its stored requests.
func (c *Client) DeleteEndpoint(ctx context.Context, id string) error {
	return c.do(ctx, http.MethodDelete, c.workspacePath("webhooks", id), nil, nil, nil)
}
