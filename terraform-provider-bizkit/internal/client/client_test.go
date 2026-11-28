package client

import (
	"context"
	"io"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestPatchLeavesOutUnsetNullablesAndSendsNull(t *testing.T) {
	var bodies []string
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		raw, _ := io.ReadAll(r.Body)
		bodies = append(bodies, string(raw))
		if got := r.Header.Get("Authorization"); got != "Bearer bk_x" {
			t.Errorf("Authorization = %q", got)
		}
		_, _ = w.Write([]byte(`{}`))
	}))
	defer srv.Close()
	c, err := New(srv.URL+"/", "bk_x", "ws", nil)
	if err != nil {
		t.Fatal(err)
	}
	ctx := context.Background()
	if _, err := c.UpdateEndpoint(ctx, "e", UpdateEndpoint{Name: Ptr("n")}); err != nil {
		t.Fatal(err)
	}
	if _, err := c.UpdateEndpoint(ctx, "e", UpdateEndpoint{ForwardURL: Null[string]()}); err != nil {
		t.Fatal(err)
	}
	if _, err := c.UpdateEndpoint(ctx, "e", UpdateEndpoint{ForwardURL: Value("https://x.test/")}); err != nil {
		t.Fatal(err)
	}
	want := []string{`{"name":"n"}`, `{"forward_url":null}`, `{"forward_url":"https://x.test/"}`}
	for i := range want {
		if bodies[i] != want[i] {
			t.Errorf("body %d = %s, want %s", i, bodies[i], want[i])
		}
	}
}

func TestErrorsCarryTheBodyAndNotFound(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path == "/api/v1/workspaces/ws/forms/gone" {
			w.WriteHeader(http.StatusNotFound)
			_, _ = w.Write([]byte(`{"error":{"code":"not_found","message":"no such form"}}`))
			return
		}
		w.WriteHeader(http.StatusUnprocessableEntity)
		_, _ = w.Write([]byte(`{"error":{"code":"invalid","message":"check the fields","fields":{"name":["is empty"]}}}`))
	}))
	defer srv.Close()
	c, _ := New(srv.URL, "bk_x", "ws", nil)
	_, err := c.GetForm(context.Background(), "gone")
	if !IsNotFound(err) {
		t.Fatalf("want not found, got %v", err)
	}
	_, err = c.CreateForm(context.Background(), CreateForm{})
	if IsNotFound(err) || err == nil {
		t.Fatalf("want a 422, got %v", err)
	}
	if got := err.Error(); got != "bizkit API: 422 invalid: check the fields\n  name: is empty" {
		t.Errorf("message = %q", got)
	}
}

func TestNewRejectsAnEndpointWithoutScheme(t *testing.T) {
	if _, err := New("bizkit.example.com", "k", "w", nil); err == nil {
		t.Fatal("want an error")
	}
}
