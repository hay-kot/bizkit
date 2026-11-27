package client

import (
	"context"
	"net/http"
	"net/url"
)

// Workspace is the API's Workspace.
type Workspace struct {
	ID        string `json:"id"`
	Slug      string `json:"slug"`
	Name      string `json:"name"`
	CreatedAt string `json:"created_at"`
}

// GetWorkspace reads the client's workspace.
func (c *Client) GetWorkspace(ctx context.Context) (*Workspace, error) {
	var out Workspace
	return &out, c.do(ctx, http.MethodGet, c.workspacePath(), nil, nil, &out)
}

// Project is the API's Project.
type Project struct {
	ID          string `json:"id"`
	WorkspaceID string `json:"workspace_id"`
	Slug        string `json:"slug"`
	Name        string `json:"name"`
	CreatedAt   string `json:"created_at"`
}

// CreateProject is projects.create's input.
type CreateProject struct {
	Name string `json:"name"`
	Slug string `json:"slug"`
}

// UpdateProject is projects.update's input.
type UpdateProject struct {
	Name string `json:"name"`
}

type projectList struct {
	Items []Project `json:"items"`
}

// CreateProject adds a project.
func (c *Client) CreateProject(ctx context.Context, in CreateProject) (*Project, error) {
	var out Project
	return &out, c.do(ctx, http.MethodPost, c.workspacePath("projects"), nil, in, &out)
}

// GetProject reads a project by ID.
func (c *Client) GetProject(ctx context.Context, id string) (*Project, error) {
	var out Project
	return &out, c.do(ctx, http.MethodGet, c.workspacePath("projects", id), nil, nil, &out)
}

// ListProjects lists the workspace's projects, at most 500.
func (c *Client) ListProjects(ctx context.Context) ([]Project, error) {
	var out projectList
	err := c.do(ctx, http.MethodGet, c.workspacePath("projects"), url.Values{"limit": {"500"}}, nil, &out)
	return out.Items, err
}

// UpdateProject renames a project.
func (c *Client) UpdateProject(ctx context.Context, id string, in UpdateProject) (*Project, error) {
	var out Project
	return &out, c.do(ctx, http.MethodPatch, c.workspacePath("projects", id), nil, in, &out)
}

// DeleteProject deletes a project and everything in it.
func (c *Client) DeleteProject(ctx context.Context, id string) error {
	return c.do(ctx, http.MethodDelete, c.workspacePath("projects", id), nil, nil, nil)
}
