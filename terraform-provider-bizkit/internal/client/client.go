// Package client is a small typed client for the bizkit HTTP API, covering
// the operations the provider manages. spec_test.go checks every path and
// field against the repo's openapi.json.
package client

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"
)

// Client calls one workspace's API with an API key.
type Client struct {
	base        *url.URL
	apiKey      string
	workspaceID string
	http        *http.Client
}

// New returns a client for the workspace at endpoint, such as
// https://bizkit.example.com. A nil httpClient uses http.DefaultClient.
func New(endpoint, apiKey, workspaceID string, httpClient *http.Client) (*Client, error) {
	base, err := url.Parse(strings.TrimRight(endpoint, "/"))
	if err != nil {
		return nil, fmt.Errorf("parsing endpoint: %w", err)
	}
	if base.Scheme != "http" && base.Scheme != "https" {
		return nil, fmt.Errorf("endpoint %q needs an http or https scheme", endpoint)
	}
	if httpClient == nil {
		httpClient = http.DefaultClient
	}
	return &Client{base: base, apiKey: apiKey, workspaceID: workspaceID, http: httpClient}, nil
}

// WorkspaceID is the workspace every call goes to.
func (c *Client) WorkspaceID() string { return c.workspaceID }

// APIError is a non-2xx answer.
type APIError struct {
	Status  int
	Code    string
	Message string
	Fields  map[string][]string
}

func (e *APIError) Error() string {
	msg := fmt.Sprintf("bizkit API: %d %s: %s", e.Status, e.Code, e.Message)
	for field, problems := range e.Fields {
		msg += fmt.Sprintf("\n  %s: %s", field, strings.Join(problems, "; "))
	}
	return msg
}

// IsNotFound reports whether err is a 404, which a Read takes as "gone".
func IsNotFound(err error) bool {
	var apiErr *APIError
	return errors.As(err, &apiErr) && apiErr.Status == http.StatusNotFound
}

type errorBody struct {
	Error struct {
		Code    string              `json:"code"`
		Message string              `json:"message"`
		Fields  map[string][]string `json:"fields"`
	} `json:"error"`
}

// workspacePath joins segments under /api/v1/workspaces/{workspace_id},
// escaping each one.
func (c *Client) workspacePath(segments ...string) string {
	parts := []string{"api", "v1", "workspaces", url.PathEscape(c.workspaceID)}
	for _, s := range segments {
		parts = append(parts, url.PathEscape(s))
	}
	return "/" + strings.Join(parts, "/")
}

func (c *Client) do(ctx context.Context, method, path string, query url.Values, in, out any) error {
	target := *c.base
	target.Path = strings.TrimRight(target.Path, "/") + path
	target.RawQuery = query.Encode()

	var body io.Reader
	if in != nil {
		raw, err := json.Marshal(in)
		if err != nil {
			return fmt.Errorf("encoding the %s %s body: %w", method, path, err)
		}
		body = bytes.NewReader(raw)
	}
	req, err := http.NewRequestWithContext(ctx, method, target.String(), body)
	if err != nil {
		return fmt.Errorf("building %s %s: %w", method, path, err)
	}
	req.Header.Set("Authorization", "Bearer "+c.apiKey)
	req.Header.Set("Accept", "application/json")
	if in != nil {
		req.Header.Set("Content-Type", "application/json")
	}

	resp, err := c.http.Do(req)
	if err != nil {
		return fmt.Errorf("%s %s: %w", method, path, err)
	}
	defer resp.Body.Close()
	raw, err := io.ReadAll(resp.Body)
	if err != nil {
		return fmt.Errorf("reading the %s %s answer: %w", method, path, err)
	}

	if resp.StatusCode >= 300 {
		apiErr := &APIError{Status: resp.StatusCode, Code: http.StatusText(resp.StatusCode), Message: strings.TrimSpace(string(raw))}
		var parsed errorBody
		if json.Unmarshal(raw, &parsed) == nil && parsed.Error.Code != "" {
			apiErr.Code, apiErr.Message, apiErr.Fields = parsed.Error.Code, parsed.Error.Message, parsed.Error.Fields
		}
		return apiErr
	}
	if out == nil || len(raw) == 0 {
		return nil
	}
	if err := json.Unmarshal(raw, out); err != nil {
		return fmt.Errorf("decoding the %s %s answer: %w", method, path, err)
	}
	return nil
}

// Nullable is a PATCH field that can be left out, set, or set to null to
// clear it. Only the fields the API clears on null use it.
type Nullable[T any] struct {
	Set   bool
	Value *T
}

// Null clears the field.
func Null[T any]() Nullable[T] { return Nullable[T]{Set: true} }

// Value sets the field.
func Value[T any](v T) Nullable[T] { return Nullable[T]{Set: true, Value: &v} }

// MarshalJSON writes the value or null. A struct holding an unset Nullable
// must leave the key out through its own MarshalJSON, as encoding/json
// can't omit a struct value.
func (n Nullable[T]) MarshalJSON() ([]byte, error) {
	if n.Value == nil {
		return []byte("null"), nil
	}
	return json.Marshal(*n.Value)
}

// patchBody marshals v, then drops every key whose Nullable is unset.
func patchBody(v any, nullables map[string]bool) (json.RawMessage, error) {
	raw, err := json.Marshal(v)
	if err != nil {
		return nil, err
	}
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(raw, &fields); err != nil {
		return nil, err
	}
	for key, set := range nullables {
		if !set {
			delete(fields, key)
		}
	}
	return json.Marshal(fields)
}

// Ptr returns a pointer to v, for optional request fields.
func Ptr[T any](v T) *T { return &v }
