package client

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"reflect"
	"sort"
	"strings"
	"testing"
)

// spec is the parts of the repo's openapi.json the contract test reads.
type spec struct {
	Paths      map[string]map[string]specOp `json:"paths"`
	Components struct {
		Schemas map[string]specSchema `json:"schemas"`
	} `json:"components"`
}

type specOp struct {
	RequestBody *struct {
		Content map[string]struct {
			Schema specSchema `json:"schema"`
		} `json:"content"`
	} `json:"requestBody"`
}

type specSchema struct {
	Ref        string                `json:"$ref"`
	Properties map[string]specSchema `json:"properties"`
}

func loadSpec(t *testing.T) spec {
	t.Helper()
	raw, err := os.ReadFile("../../../openapi.json")
	if err != nil {
		t.Fatalf("reading openapi.json: %v", err)
	}
	var s spec
	if err := json.Unmarshal(raw, &s); err != nil {
		t.Fatalf("parsing openapi.json: %v", err)
	}
	return s
}

func (s spec) resolve(schema specSchema) specSchema {
	for schema.Ref != "" {
		schema = s.Components.Schemas[strings.TrimPrefix(schema.Ref, "#/components/schemas/")]
	}
	return schema
}

type call struct{ method, path string }

// recordCalls runs every client method against a server that answers 200
// with {} and returns the method and path of each request.
func recordCalls(t *testing.T) []call {
	t.Helper()
	var calls []call
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		calls = append(calls, call{r.Method, r.URL.Path})
		_, _ = w.Write([]byte("{}"))
	}))
	defer srv.Close()
	c, err := New(srv.URL, "bk_test", "WS", nil)
	if err != nil {
		t.Fatal(err)
	}
	ctx := context.Background()
	for _, run := range []func() error{
		func() error { _, err := c.GetWorkspace(ctx); return err },
		func() error { _, err := c.CreateProject(ctx, CreateProject{}); return err },
		func() error { _, err := c.GetProject(ctx, "ID"); return err },
		func() error { _, err := c.ListProjects(ctx); return err },
		func() error { _, err := c.UpdateProject(ctx, "ID", UpdateProject{}); return err },
		func() error { return c.DeleteProject(ctx, "ID") },
		func() error { _, err := c.CreateForm(ctx, CreateForm{}); return err },
		func() error { _, err := c.GetForm(ctx, "ID"); return err },
		func() error { _, err := c.UpdateForm(ctx, "ID", UpdateForm{}); return err },
		func() error { return c.DeleteForm(ctx, "ID") },
		func() error { _, err := c.CreateEndpoint(ctx, CreateEndpoint{}); return err },
		func() error { _, err := c.GetEndpoint(ctx, "ID"); return err },
		func() error { _, err := c.UpdateEndpoint(ctx, "ID", UpdateEndpoint{}); return err },
		func() error { return c.DeleteEndpoint(ctx, "ID") },
		func() error { _, err := c.CreateMailingList(ctx, CreateMailingList{}); return err },
		func() error { _, err := c.GetMailingList(ctx, "ID"); return err },
		func() error { _, err := c.UpdateMailingList(ctx, "ID", UpdateMailingList{}); return err },
		func() error { return c.DeleteMailingList(ctx, "ID") },
	} {
		if err := run(); err != nil {
			t.Fatal(err)
		}
	}
	return calls
}

// matches reports whether a request path fits a spec path template.
func matches(template, path string) bool {
	want, got := strings.Split(template, "/"), strings.Split(path, "/")
	if len(want) != len(got) {
		return false
	}
	for i := range want {
		if !strings.HasPrefix(want[i], "{") && want[i] != got[i] {
			return false
		}
	}
	return true
}

func TestEveryCallIsInTheSpec(t *testing.T) {
	s := loadSpec(t)
	for _, c := range recordCalls(t) {
		found := false
		for path, ops := range s.Paths {
			if _, ok := ops[strings.ToLower(c.method)]; ok && matches(path, c.path) {
				found = true
			}
		}
		if !found {
			t.Errorf("%s %s is not an operation in openapi.json", c.method, c.path)
		}
	}
}

// jsonFields lists the JSON keys of a struct type.
func jsonFields(typ reflect.Type) []string {
	var out []string
	for i := range typ.NumField() {
		name, _, _ := strings.Cut(typ.Field(i).Tag.Get("json"), ",")
		if name != "" && name != "-" {
			out = append(out, name)
		}
	}
	sort.Strings(out)
	return out
}

func TestStructFieldsMatchTheSpec(t *testing.T) {
	s := loadSpec(t)
	body := func(path, method string) specSchema {
		op := s.Paths[path][method]
		if op.RequestBody == nil {
			t.Fatalf("%s %s has no request body", method, path)
		}
		return op.RequestBody.Content["application/json"].Schema
	}
	ws := "/api/v1/workspaces/{workspace_id}"
	cases := []struct {
		typ    any
		schema specSchema
		// exact means every schema property must be in the struct too.
		exact bool
	}{
		{Workspace{}, specSchema{Ref: "Workspace"}, true},
		{Project{}, specSchema{Ref: "Project"}, true},
		{CreateProject{}, body(ws+"/projects", "post"), true},
		{UpdateProject{}, body(ws+"/projects/{project_id}", "patch"), true},
		{FieldDef{}, specSchema{Ref: "FieldDef"}, true},
		{Form{}, specSchema{Ref: "Form"}, true},
		{CreateForm{}, body(ws+"/forms", "post"), true},
		{UpdateForm{}, body(ws+"/forms/{form_id}", "patch"), true},
		{Endpoint{}, specSchema{Ref: "Endpoint"}, true},
		{CreateEndpoint{}, body(ws+"/webhooks", "post"), true},
		{UpdateEndpoint{}, body(ws+"/webhooks/{endpoint_id}", "patch"), true},
		// counts is only on get and the provider doesn't read it.
		{MailingList{}, specSchema{Ref: "MailingList"}, false},
		{CreateMailingList{}, body(ws+"/mailing/lists", "post"), true},
		{UpdateMailingList{}, body(ws+"/mailing/lists/{list_id}", "patch"), true},
	}
	for _, tc := range cases {
		typ := reflect.TypeOf(tc.typ)
		schema := tc.schema
		if schema.Ref != "" && !strings.HasPrefix(schema.Ref, "#") {
			schema.Ref = "#/components/schemas/" + schema.Ref
		}
		props := s.resolve(schema).Properties
		if len(props) == 0 {
			t.Errorf("%s: schema has no properties", typ.Name())
			continue
		}
		fields := jsonFields(typ)
		for _, f := range fields {
			if _, ok := props[f]; !ok {
				t.Errorf("%s.%s is not in the spec", typ.Name(), f)
			}
		}
		if tc.exact {
			for p := range props {
				if !contains(fields, p) {
					t.Errorf("%s lacks the spec's %q", typ.Name(), p)
				}
			}
		}
	}
}

func contains(list []string, v string) bool {
	for _, x := range list {
		if x == v {
			return true
		}
	}
	return false
}
