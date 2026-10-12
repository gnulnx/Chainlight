package main

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

func testRPCClient(t *testing.T, h http.HandlerFunc) (*Client, string) {
	t.Helper()
	srv := httptest.NewServer(h)
	t.Cleanup(srv.Close)
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, ".cookie"), []byte("__cookie__:secret-one\n"), 0600); err != nil {
		t.Fatal(err)
	}
	c, err := newClient(srv.URL, dir)
	if err != nil {
		t.Fatal(err)
	}
	return c, dir
}
func TestCookieReloadsAfterRotation(t *testing.T) {
	var passwords []string
	c, dir := testRPCClient(t, func(w http.ResponseWriter, r *http.Request) {
		u, p, ok := r.BasicAuth()
		if !ok || u != "__cookie__" {
			t.Error("missing cookie auth")
		}
		passwords = append(passwords, p)
		fmt.Fprint(w, `{"result":17,"error":null}`)
	})
	var out int
	if err := c.call(context.Background(), "uptime", &out); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, ".cookie"), []byte("__cookie__:secret-two"), 0600); err != nil {
		t.Fatal(err)
	}
	if err := c.call(context.Background(), "uptime", &out); err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(passwords, []string{"secret-one", "secret-two"}) {
		t.Fatalf("rotation not honored: %v", passwords)
	}
}
func TestSnapshotOnlyReadsNodeState(t *testing.T) {
	var methods []string
	c, _ := testRPCClient(t, func(w http.ResponseWriter, r *http.Request) {
		var req struct {
			Method string `json:"method"`
			Params []any  `json:"params"`
		}
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			t.Error(err)
		}
		methods = append(methods, req.Method)
		if len(req.Params) != 0 {
			t.Error("unexpected RPC params")
		}
		result := `{}`
		switch req.Method {
		case "getblockchaininfo":
			result = `{"chain":"main","initialblockdownload":true}`
		case "getnetworkinfo":
			result = `{"networkactive":true}`
		case "getpeerinfo":
			result = `[]`
		case "uptime":
			result = `10`
		}
		fmt.Fprintf(w, `{"result":%s,"error":null}`, result)
	})
	s, err := c.snapshot(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if s.At.IsZero() {
		t.Error("missing observation time")
	}
	want := []string{"getblockchaininfo", "getnetworkinfo", "getnettotals", "getpeerinfo", "getmempoolinfo", "uptime"}
	if !reflect.DeepEqual(methods, want) {
		t.Fatalf("unexpected methods %v", methods)
	}
}
func TestRPCFailureStates(t *testing.T) {
	for _, tc := range []struct {
		name, body, want string
		status           int
	}{{"unauthorized", "secret-one", "authentication failed", 401}, {"malformed", "not-json", "invalid RPC response", 200}, {"missing", `{"error":null}`, "missing RPC result", 200}, {"warmup", `{"error":{"code":-28,"message":"Loading block index"}}`, "loading block databases", 200}, {"warmup-service-unavailable", `{"error":{"code":-28,"message":"Loading block index"}}`, "loading block databases", 503}} {
		t.Run(tc.name, func(t *testing.T) {
			c, _ := testRPCClient(t, func(w http.ResponseWriter, r *http.Request) { w.WriteHeader(tc.status); fmt.Fprint(w, tc.body) })
			var v any
			err := c.call(context.Background(), "uptime", &v)
			if err == nil || !strings.Contains(err.Error(), tc.want) {
				t.Fatalf("got %v, want %s", err, tc.want)
			}
			if strings.Contains(err.Error(), "secret-one") {
				t.Fatal("credential exposed")
			}
		})
	}
}
func TestRPCErrorDoesNotEchoCookie(t *testing.T) {
	c, _ := testRPCClient(t, func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprint(w, `{"error":{"code":-1,"message":"request contained __cookie__:secret-one"}}`)
	})
	var v any
	err := c.call(context.Background(), "uptime", &v)
	if err == nil {
		t.Fatal("wanted error")
	}
	if strings.Contains(err.Error(), "secret-one") || strings.Contains(err.Error(), "__cookie__") {
		t.Fatalf("RPC error exposed cookie: %s", err)
	}
}
func TestRejectUnsafeEndpoints(t *testing.T) {
	for _, u := range []string{"http://example.com:8332", "http://localhost:8332", "https://127.0.0.1:8332", "http://user:pass@127.0.0.1:8332", "http://127.0.0.1:8332/wallet/foo", "http://127.0.0.1:8332?token=secret", "http://127.0.0.1:8332#secret"} {
		if _, err := newClient(u, t.TempDir()); err == nil {
			t.Errorf("accepted %s", u)
		}
	}
}
func TestRedirectNeverReceivesCookie(t *testing.T) {
	redirected := false
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { redirected = true; fmt.Fprint(w, `{"result":1}`) }))
	defer srv.Close()
	c, _ := testRPCClient(t, func(w http.ResponseWriter, r *http.Request) { http.Redirect(w, r, srv.URL, http.StatusFound) })
	var out int
	if err := c.call(context.Background(), "uptime", &out); err == nil {
		t.Error("redirect accepted")
	}
	if redirected {
		t.Error("followed credential redirect")
	}
}
func TestUnavailableOptionalRPCPreservesCoreSnapshot(t *testing.T) {
	c, _ := testRPCClient(t, func(w http.ResponseWriter, r *http.Request) {
		var req struct {
			Method string `json:"method"`
		}
		json.NewDecoder(r.Body).Decode(&req)
		switch req.Method {
		case "getblockchaininfo":
			fmt.Fprint(w, `{"result":{"chain":"main","blocks":1234},"error":null}`)
		case "getnetworkinfo":
			fmt.Fprint(w, `{"result":{"networkactive":true},"error":null}`)
		default:
			fmt.Fprint(w, `{"error":{"code":-32601,"message":"Method not found"}}`)
		}
	})
	s, err := c.snapshot(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if s.Chain.Blocks != 1234 || len(s.Notes) != 4 {
		t.Fatalf("lost partial observation: %+v", s)
	}
}
func TestUnavailableCookieNeverMakesRequest(t *testing.T) {
	called := false
	c, dir := testRPCClient(t, func(w http.ResponseWriter, r *http.Request) { called = true })
	if err := os.Remove(filepath.Join(dir, ".cookie")); err != nil {
		t.Fatal(err)
	}
	var v any
	if err := c.call(context.Background(), "uptime", &v); err == nil || !strings.Contains(err.Error(), "cookie is not available") {
		t.Fatalf("unexpected error %v", err)
	}
	if called {
		t.Fatal("sent request without cookie")
	}
}
